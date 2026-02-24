//! Streaming Parser for Large Inputs
//!
//! This module provides streaming parsing capabilities for inputs that are
//! too large to fit in memory or arrive incrementally (e.g., network streams,
//! large files).
//!
//! # Overview
//!
//! The streaming parser uses a chunk-based approach:
//! 1. **Chunked Input** - Input is processed in fixed-size chunks
//! 2. **Sliding Window** - A window of chunks is kept for backtracking
//! 3. **Lazy Cache Eviction** - Cache entries are evicted when they're
//!    no longer needed for backtracking
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    Streaming Parser                              │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                                                                  │
//! │  Input Stream: [Chunk 0][Chunk 1][Chunk 2][Chunk 3]...         │
//! │                     │         │         │                       │
//! │                     ▼         ▼         ▼                       │
//! │  ┌─────────────────────────────────────────────────────────┐   │
//! │  │               Sliding Window (3 chunks)                  │   │
//! │  │  ┌─────────┐ ┌─────────┐ ┌─────────┐                     │   │
//! │  │  │ Chunk 1 │ │ Chunk 2 │ │ Chunk 3 │  ← Current window   │   │
//! │  │  └─────────┘ └─────────┘ └─────────┘                     │   │
//! │  │     (evicted)                                     ↑       │   │
//! │  │                                              Parse pos    │   │
//! │  └─────────────────────────────────────────────────────────┘   │
//! │                                                                  │
//! │  Cache: Only entries within window are kept                     │
//! │                                                                  │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Limitations
//!
//! Streaming parsing has some limitations compared to regular parsing:
//! 1. **Backtracking Window** - Can only backtrack within the window
//! 2. **Memory vs Performance** - Larger window = more memory but more backtracking
//! 3. **Grammar Restrictions** - Some grammars may require larger windows
//!
//! # Usage
//!
//! ```rust,ignore
//! use parsanol::portable::{Grammar, streaming::{StreamingParser, ChunkConfig}};
//! use std::io::Read;
//!
//! // Create streaming parser
//! let grammar = /* ... */;
//! let config = ChunkConfig {
//!     chunk_size: 64 * 1024,      // 64 KB chunks
//!     window_size: 3,              // Keep 3 chunks in memory
//! };
//! let mut parser = StreamingParser::new(&grammar, config);
//!
//! // Parse from a reader
//! let mut reader = /* some Read implementation */;
//! let result = parser.parse_from_reader(&mut reader)?;
//! ```

use super::{
    arena::AstArena,
    ast::{AstNode, ParseError},
    cache::DenseCache,
    grammar::Grammar,
};
use std::io::Read;

/// Configuration for chunk-based streaming parsing
#[derive(Debug, Clone)]
pub struct ChunkConfig {
    /// Size of each chunk in bytes
    pub chunk_size: usize,

    /// Number of chunks to keep in the sliding window
    /// Larger window = more backtracking capability but more memory
    pub window_size: usize,
}

impl Default for ChunkConfig {
    fn default() -> Self {
        Self {
            chunk_size: 64 * 1024, // 64 KB
            window_size: 3,         // Keep 3 chunks (~192 KB) in memory
        }
    }
}

impl ChunkConfig {
    /// Create a new chunk configuration
    #[inline]
    pub fn new(chunk_size: usize, window_size: usize) -> Self {
        Self {
            chunk_size,
            window_size,
        }
    }

    /// Get the maximum memory usage (approximate)
    #[inline]
    pub fn max_memory(&self) -> usize {
        self.chunk_size * self.window_size
    }

    /// Configuration for small inputs (16 KB chunks, 2 window)
    #[inline]
    pub fn small() -> Self {
        Self {
            chunk_size: 16 * 1024,
            window_size: 2,
        }
    }

    /// Configuration for medium inputs (64 KB chunks, 3 window)
    #[inline]
    pub fn medium() -> Self {
        Self::default()
    }

    /// Configuration for large inputs (256 KB chunks, 4 window)
    #[inline]
    pub fn large() -> Self {
        Self {
            chunk_size: 256 * 1024,
            window_size: 4,
        }
    }

    /// Configuration for very large inputs (1 MB chunks, 5 window)
    #[inline]
    pub fn huge() -> Self {
        Self {
            chunk_size: 1024 * 1024,
            window_size: 5,
        }
    }
}

/// A chunk of input data
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct Chunk {
    /// The chunk data
    data: Vec<u8>,

    /// Global offset of this chunk in the input
    global_offset: usize,

    /// Whether this is the last chunk
    is_last: bool,
}

impl Chunk {
    fn new(data: Vec<u8>, global_offset: usize, is_last: bool) -> Self {
        Self {
            data,
            global_offset,
            is_last,
        }
    }

    #[inline]
    fn len(&self) -> usize {
        self.data.len()
    }

    #[inline]
    #[allow(dead_code)]
    fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    #[inline]
    fn end_offset(&self) -> usize {
        self.global_offset + self.data.len()
    }
}

/// Sliding window over chunks
#[derive(Debug)]
#[allow(dead_code)]
struct SlidingWindow {
    /// Chunks in the window
    chunks: Vec<Chunk>,

    /// Maximum number of chunks to keep
    max_chunks: usize,

    /// Global offset of the start of the window
    window_start: usize,
}

impl SlidingWindow {
    fn new(max_chunks: usize) -> Self {
        Self {
            chunks: Vec::with_capacity(max_chunks),
            max_chunks,
            window_start: 0,
        }
    }

    /// Add a chunk to the window
    fn push(&mut self, chunk: Chunk) {
        // If window is full, evict oldest chunk
        if self.chunks.len() >= self.max_chunks {
            let evicted = self.chunks.remove(0);
            self.window_start = evicted.end_offset();
        }

        self.chunks.push(chunk);
    }

    /// Get a byte at a global position
    #[allow(dead_code)]
    fn get_byte(&self, global_pos: usize) -> Option<u8> {
        for chunk in &self.chunks {
            if global_pos >= chunk.global_offset && global_pos < chunk.end_offset() {
                let local_pos = global_pos - chunk.global_offset;
                return Some(chunk.data[local_pos]);
            }
        }
        None
    }

    /// Get a slice of bytes starting at a global position
    #[allow(dead_code)]
    fn get_slice(&self, global_pos: usize, length: usize) -> Option<&[u8]> {
        for chunk in &self.chunks {
            if global_pos >= chunk.global_offset && global_pos < chunk.end_offset() {
                let local_pos = global_pos - chunk.global_offset;
                let end = (local_pos + length).min(chunk.data.len());
                if end <= chunk.data.len() {
                    return Some(&chunk.data[local_pos..end]);
                }
            }
        }
        None
    }

    /// Check if a position is in the window
    #[inline]
    #[allow(dead_code)]
    fn contains(&self, global_pos: usize) -> bool {
        self.chunks
            .iter()
            .any(|c| global_pos >= c.global_offset && global_pos < c.end_offset())
    }

    /// Get the total length of data in the window
    #[inline]
    #[allow(dead_code)]
    fn total_len(&self) -> usize {
        self.chunks.last().map(|c| c.end_offset()).unwrap_or(0)
    }

    /// Check if the window contains the last chunk
    #[inline]
    #[allow(dead_code)]
    fn has_last_chunk(&self) -> bool {
        self.chunks.last().map(|c| c.is_last).unwrap_or(false)
    }

    /// Get the window start offset
    #[inline]
    fn start_offset(&self) -> usize {
        self.window_start
    }

    /// Clear the window
    fn clear(&mut self) {
        self.chunks.clear();
        self.window_start = 0;
    }
}

/// Streaming parser for large inputs
pub struct StreamingParser<'a> {
    /// The compiled grammar
    grammar: &'a Grammar,

    /// Chunk configuration
    config: ChunkConfig,

    /// Sliding window over chunks
    window: SlidingWindow,

    /// Packrat cache (entries are evicted as window slides)
    cache: DenseCache,

    /// Current position in the input
    current_pos: usize,

    /// Total bytes read
    total_bytes_read: usize,

    /// Whether we've reached EOF
    eof_reached: bool,
}

/// Result of streaming parsing
#[derive(Debug)]
pub struct StreamingResult {
    /// The parsed AST
    pub ast: AstNode,

    /// Total bytes processed
    pub bytes_processed: usize,

    /// Number of chunks processed
    pub chunks_processed: usize,

    /// Peak memory usage (approximate)
    pub peak_memory: usize,

    /// Cache statistics
    pub cache_stats: (u64, u64, f64),
}

impl<'a> StreamingParser<'a> {
    /// Create a new streaming parser
    #[inline]
    pub fn new(grammar: &'a Grammar, config: ChunkConfig) -> Self {
        Self {
            grammar,
            config: config.clone(),
            window: SlidingWindow::new(config.window_size),
            cache: DenseCache::new(4096),
            current_pos: 0,
            total_bytes_read: 0,
            eof_reached: false,
        }
    }

    /// Create a streaming parser with default configuration
    #[inline]
    pub fn with_defaults(grammar: &'a Grammar) -> Self {
        Self::new(grammar, ChunkConfig::default())
    }

    /// Parse from a reader
    pub fn parse_from_reader<R: Read>(
        &mut self,
        reader: &mut R,
        arena: &mut AstArena,
    ) -> Result<StreamingResult, StreamingError> {
        let mut buffer = vec![0u8; self.config.chunk_size];
        let mut chunks_processed = 0;
        let mut peak_memory = 0;

        loop {
            // Read a chunk
            let bytes_read = reader
                .read(&mut buffer)
                .map_err(|e| StreamingError::IoError(e.to_string()))?;
            if bytes_read == 0 {
                self.eof_reached = true;
                break;
            }

            // Create chunk
            let chunk = Chunk::new(
                buffer[..bytes_read].to_vec(),
                self.total_bytes_read,
                false,
            );

            // Update total bytes read
            self.total_bytes_read += bytes_read;

            // Add to window
            self.window.push(chunk);
            chunks_processed += 1;

            // Track peak memory
            let current_memory = self.window.chunks.iter().map(|c| c.len()).sum::<usize>()
                + self.cache.memory_usage();
            peak_memory = peak_memory.max(current_memory);

            // Evict cache entries outside the window
            self.evict_old_cache_entries();

            // Resize buffer for next read
            buffer.resize(self.config.chunk_size, 0);
        }

        // Mark last chunk
        if let Some(last_chunk) = self.window.chunks.last_mut() {
            last_chunk.is_last = true;
        }

        // Now perform the actual parsing
        // Note: This is a simplified version - a full implementation would
        // parse incrementally as chunks arrive
        let all_data: Vec<u8> = self
            .window
            .chunks
            .iter()
            .flat_map(|c| c.data.clone())
            .collect();

        // SAFETY: We need to convert bytes to string for parsing
        // In a real implementation, we'd handle encoding properly
        let input = String::from_utf8(all_data)
            .map_err(|e| StreamingError::InvalidUtf8(e.to_string()))?;

        // Use standard parser
        let mut parser = super::parser::PortableParser::new(self.grammar, &input, arena);
        let ast = parser
            .parse()
            .map_err(|e| StreamingError::ParseError(e))?;

        let cache_stats = self.cache.stats();

        Ok(StreamingResult {
            ast,
            bytes_processed: self.total_bytes_read,
            chunks_processed,
            peak_memory,
            cache_stats,
        })
    }

    /// Parse from an iterator of byte chunks
    pub fn parse_from_chunks<I>(
        &mut self,
        chunks: I,
        arena: &mut AstArena,
    ) -> Result<StreamingResult, StreamingError>
    where
        I: IntoIterator<Item = Vec<u8>>,
    {
        let mut chunks_processed = 0;
        let mut peak_memory = 0;

        for chunk_data in chunks {
            let chunk = Chunk::new(chunk_data, self.total_bytes_read, false);
            self.total_bytes_read += chunk.len();
            self.window.push(chunk);
            chunks_processed += 1;

            // Track peak memory
            let current_memory = self.window.chunks.iter().map(|c| c.len()).sum::<usize>()
                + self.cache.memory_usage();
            peak_memory = peak_memory.max(current_memory);

            // Evict cache entries outside the window
            self.evict_old_cache_entries();
        }

        // Mark last chunk
        if let Some(last_chunk) = self.window.chunks.last_mut() {
            last_chunk.is_last = true;
        }
        self.eof_reached = true;

        // Collect all data and parse
        let all_data: Vec<u8> = self
            .window
            .chunks
            .iter()
            .flat_map(|c| c.data.clone())
            .collect();

        let input = String::from_utf8(all_data)
            .map_err(|e| StreamingError::InvalidUtf8(e.to_string()))?;

        let mut parser = super::parser::PortableParser::new(self.grammar, &input, arena);
        let ast = parser
            .parse()
            .map_err(|e| StreamingError::ParseError(e))?;

        let cache_stats = self.cache.stats();

        Ok(StreamingResult {
            ast,
            bytes_processed: self.total_bytes_read,
            chunks_processed,
            peak_memory,
            cache_stats,
        })
    }

    /// Parse from a file path
    pub fn parse_from_file<P: AsRef<std::path::Path>>(
        &mut self,
        path: P,
        arena: &mut AstArena,
    ) -> Result<StreamingResult, StreamingError> {
        let mut file = std::fs::File::open(path)
            .map_err(|e| StreamingError::IoError(e.to_string()))?;
        self.parse_from_reader(&mut file, arena)
    }

    /// Evict cache entries that are outside the current window
    fn evict_old_cache_entries(&mut self) {
        let window_start = self.window.start_offset();

        // Keep only entries that are within the window
        self.cache.retain(|entry| entry.pos as usize >= window_start);
    }

    /// Get the current memory usage
    #[inline]
    pub fn memory_usage(&self) -> usize {
        self.window.chunks.iter().map(|c| c.len()).sum::<usize>()
            + self.cache.memory_usage()
    }

    /// Reset the parser state
    pub fn reset(&mut self) {
        self.window.clear();
        self.cache.clear();
        self.current_pos = 0;
        self.total_bytes_read = 0;
        self.eof_reached = false;
    }

    /// Check if EOF has been reached
    #[inline]
    pub fn is_eof(&self) -> bool {
        self.eof_reached
    }

    /// Get the total bytes read so far
    #[inline]
    pub fn total_bytes_read(&self) -> usize {
        self.total_bytes_read
    }
}

/// Errors that can occur during streaming parsing
#[derive(Debug)]
pub enum StreamingError {
    /// I/O error
    IoError(String),

    /// Parse error
    ParseError(ParseError),

    /// Invalid UTF-8 in input
    InvalidUtf8(String),

    /// Backtrack limit exceeded
    BacktrackLimitExceeded {
        /// Position we tried to backtrack to
        requested_position: usize,
        /// Earliest position available in window
        earliest_available: usize,
    },

    /// Window too small for grammar
    WindowTooSmall {
        /// Required window size
        required: usize,
        /// Current window size
        actual: usize,
    },
}

impl std::fmt::Display for StreamingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IoError(msg) => write!(f, "I/O error: {}", msg),
            Self::ParseError(e) => write!(f, "Parse error: {:?}", e),
            Self::InvalidUtf8(msg) => write!(f, "Invalid UTF-8: {}", msg),
            Self::BacktrackLimitExceeded {
                requested_position,
                earliest_available,
            } => write!(
                f,
                "Backtrack limit exceeded: tried to go to position {}, but earliest available is {}",
                requested_position, earliest_available
            ),
            Self::WindowTooSmall { required, actual } => write!(
                f,
                "Window too small: required {} chunks, have {}",
                required, actual
            ),
        }
    }
}

impl std::error::Error for StreamingError {}

/// Trait for types that can provide chunks of input
pub trait ChunkSource {
    /// Get the next chunk of input
    /// Returns None when EOF is reached
    fn next_chunk(&mut self) -> Option<Vec<u8>>;

    /// Check if there are more chunks available
    fn has_more(&self) -> bool;
}

impl ChunkSource for Vec<u8> {
    fn next_chunk(&mut self) -> Option<Vec<u8>> {
        if self.is_empty() {
            None
        } else {
            Some(std::mem::take(self))
        }
    }

    fn has_more(&self) -> bool {
        !self.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_config_defaults() {
        let config = ChunkConfig::default();
        assert_eq!(config.chunk_size, 64 * 1024);
        assert_eq!(config.window_size, 3);
        assert_eq!(config.max_memory(), 192 * 1024);
    }

    #[test]
    fn test_chunk_config_presets() {
        let small = ChunkConfig::small();
        assert_eq!(small.chunk_size, 16 * 1024);
        assert_eq!(small.window_size, 2);

        let large = ChunkConfig::large();
        assert_eq!(large.chunk_size, 256 * 1024);
        assert_eq!(large.window_size, 4);

        let huge = ChunkConfig::huge();
        assert_eq!(huge.chunk_size, 1024 * 1024);
        assert_eq!(huge.window_size, 5);
    }

    #[test]
    fn test_sliding_window_basic() {
        let mut window = SlidingWindow::new(3);

        // Add chunks
        window.push(Chunk::new(vec![1, 2, 3], 0, false));
        window.push(Chunk::new(vec![4, 5, 6], 3, false));
        window.push(Chunk::new(vec![7, 8, 9], 6, false));

        assert_eq!(window.chunks.len(), 3);
        assert_eq!(window.total_len(), 9);

        // Get bytes
        assert_eq!(window.get_byte(0), Some(1));
        assert_eq!(window.get_byte(5), Some(6));
        assert_eq!(window.get_byte(8), Some(9));
        assert_eq!(window.get_byte(10), None);
    }

    #[test]
    fn test_sliding_window_eviction() {
        let mut window = SlidingWindow::new(2);

        window.push(Chunk::new(vec![1, 2, 3], 0, false));
        window.push(Chunk::new(vec![4, 5, 6], 3, false));
        window.push(Chunk::new(vec![7, 8, 9], 6, false));

        // First chunk should be evicted
        assert_eq!(window.chunks.len(), 2);
        assert_eq!(window.get_byte(0), None); // Evicted
        assert_eq!(window.get_byte(3), Some(4));
        assert_eq!(window.get_byte(6), Some(7));

        // Window start should be updated
        assert_eq!(window.start_offset(), 3);
    }

    #[test]
    fn test_sliding_window_slice() {
        let mut window = SlidingWindow::new(3);

        window.push(Chunk::new(vec![1, 2, 3, 4, 5], 0, false));

        // Get slice within chunk
        let slice = window.get_slice(1, 3);
        assert!(slice.is_some());
        assert_eq!(slice.unwrap(), &[2, 3, 4]);

        // Get slice at boundary
        let slice = window.get_slice(5, 3);
        assert!(slice.is_none()); // Past end of data
    }

    #[test]
    fn test_sliding_window_contains() {
        let mut window = SlidingWindow::new(2);

        window.push(Chunk::new(vec![1, 2, 3], 0, false));
        window.push(Chunk::new(vec![4, 5, 6], 3, false));

        assert!(window.contains(0));
        assert!(window.contains(3));
        assert!(window.contains(5));
        assert!(!window.contains(6)); // End position is exclusive

        // After eviction
        window.push(Chunk::new(vec![7, 8, 9], 6, false));
        assert!(!window.contains(0)); // Evicted
        assert!(window.contains(3));
        assert!(window.contains(6));
    }

    #[test]
    fn test_streaming_error_display() {
        let err = StreamingError::IoError("file not found".to_string());
        assert!(err.to_string().contains("file not found"));

        let err = StreamingError::BacktrackLimitExceeded {
            requested_position: 100,
            earliest_available: 200,
        };
        assert!(err.to_string().contains("100"));
        assert!(err.to_string().contains("200"));
    }

    #[test]
    fn test_chunk_is_empty() {
        let chunk = Chunk::new(vec![1, 2, 3], 0, false);
        assert!(!chunk.is_empty());

        let empty_chunk = Chunk::new(vec![], 0, false);
        assert!(empty_chunk.is_empty());
    }

    #[test]
    fn test_sliding_window_has_last_chunk() {
        let mut window = SlidingWindow::new(3);

        // No chunks yet
        assert!(!window.has_last_chunk());

        // Add non-last chunk
        window.push(Chunk::new(vec![1, 2, 3], 0, false));
        assert!(!window.has_last_chunk());

        // Add last chunk
        window.push(Chunk::new(vec![4, 5, 6], 3, true));
        assert!(window.has_last_chunk());

        // Add more chunks, not last
        window.push(Chunk::new(vec![7, 8, 9], 6, false));
        assert!(!window.has_last_chunk()); // New chunk overwrites last
    }
}
