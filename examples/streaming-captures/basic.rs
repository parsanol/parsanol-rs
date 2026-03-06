//! Streaming Parser with Captures Example
//!
//! Demonstrates the streaming parser API for memory-efficient parsing of large inputs.
//! Shows chunk-based parsing configuration and capture extraction.
//!
//! Run with: cargo run --example streaming-captures --no-default-features

#![allow(clippy::print_literal)]

use parsanol::portable::{
    arena::AstArena,
    parser_dsl::{capture, dynamic, re, GrammarBuilder},
    streaming::{ChunkConfig, StreamingParser},
};
use std::io::Cursor;

// =========================================================================
// Example 1: Basic Streaming with Captures
// =========================================================================

fn example_basic_streaming() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Example 1: Basic Streaming with Captures ---\n");

    // Simple grammar: match an identifier and capture it
    let grammar = GrammarBuilder::new()
        .rule("word", capture("word", dynamic(re(r"[a-zA-Z]+"))))
        .build();

    // Configure streaming with small chunks for demo
    let config = ChunkConfig {
        chunk_size: 64,
        window_size: 2,
    };

    let input = "hello";

    let mut parser = StreamingParser::new(&grammar, config);
    let mut arena = AstArena::for_input(64);
    let mut cursor = Cursor::new(input.as_bytes());

    let result = parser.parse_from_reader(&mut cursor, &mut arena)?;

    println!("  Input: {:?}", input);
    println!("  Bytes processed: {}", result.bytes_processed);
    println!("  Chunks processed: {}", result.chunks_processed);
    println!("  Peak memory: {} bytes", result.peak_memory);

    if let Some(captures) = &result.capture_state {
        println!("  Captures found:");
        for name in captures.names() {
            if let Some(value) = captures.get(&name) {
                println!("    {} = {:?}", name, value.get_text(input));
            }
        }
    }

    Ok(())
}

// =========================================================================
// Example 2: Chunk Configuration Options
// =========================================================================

fn example_chunk_config() {
    println!("\n--- Example 2: Chunk Configuration Options ---\n");

    println!("  ChunkConfig {{");
    println!("    chunk_size: 65536,   // 64 KB - size of each chunk");
    println!("    window_size: 2,      // Number of chunks to keep in memory");
    println!("  }}");
    println!();

    println!("  Preset configurations:");
    println!("  * ChunkConfig::small()  - 16 KB chunks, window=2");
    println!("  * ChunkConfig::medium() - 64 KB chunks, window=3 (default)");
    println!("  * ChunkConfig::large()  - 256 KB chunks, window=4");
    println!("  * ChunkConfig::huge()   - 1 MB chunks, window=5");
}

// =========================================================================
// Example 3: Chunk Size Selection Guide
// =========================================================================

fn example_chunk_selection() {
    println!("\n--- Example 3: Chunk Size Selection Guide ---\n");

    println!("  | Use Case              | Chunk Size   | Reason |");
    println!("  |----------------------|--------------|--------|");
    println!("  | Real-time feeds      | 4-16 KB      | Low latency |");
    println!("  | Log files            | 256 KB - 1 MB | Throughput |");
    println!("  | Network streams      | 8-64 KB      | Balance |");
    println!("  | Large files          | 1-4 MB       | Fewer syscalls |");

    println!("\n  Window size guidelines:");
    println!("  | Grammar type         | Window | Reason |");
    println!("  |----------------------|--------|--------|");
    println!("  | Sequential           | 1-2    | Minimal backtracking |");
    println!("  | Moderate backtracking| 2-3    | Default |");
    println!("  | Heavy backtracking   | 4-5    | Complex grammars |");

    println!("\n  Memory formula: memory = chunk_size * window_size + capture_state");
}

// =========================================================================
// Example 4: StreamingResult Structure
// =========================================================================

fn example_result_structure() {
    println!("\n--- Example 4: StreamingResult Structure ---\n");

    println!("  pub struct StreamingResult {{");
    println!("    pub ast: AstNode,               // Parse tree");
    println!("    pub bytes_processed: usize,     // Total bytes read");
    println!("    pub chunks_processed: usize,    // Number of chunks used");
    println!("    pub peak_memory: usize,         // Maximum memory used");
    println!("    pub cache_stats: (u64, u64, f64), // (hits, misses, hit_rate)");
    println!("    pub capture_state: Option<CaptureState>,  // Extracted captures");
    println!("  }}");
}

// =========================================================================
// Example 5: CaptureState API
// =========================================================================

fn example_capture_api() {
    println!("\n--- Example 5: CaptureState API ---\n");

    println!("  impl CaptureState {{");
    println!("    /// Get all capture names");
    println!("    pub fn names(&self) -> impl Iterator<Item = &String>;");
    println!();
    println!("    /// Get a capture by name");
    println!("    pub fn get(&self, name: &str) -> Option<CaptureValue>;");
    println!();
    println!("    /// Check if capture exists");
    println!("    pub fn contains(&self, name: &str) -> bool;");
    println!("  }}");
    println!();
    println!("  impl CaptureValue {{");
    println!("    /// Get text from original input (zero-copy)");
    println!("    pub fn get_text<'a>(&self, input: &'a str) -> &'a str;");
    println!();
    println!("    /// Get offset in input");
    println!("    pub fn offset(&self) -> usize;");
    println!();
    println!("    /// Get length of captured text");
    println!("    pub fn len(&self) -> usize;");
    println!("  }}");
}

// =========================================================================
// Main
// =========================================================================

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Streaming Parser with Captures Example");
    println!("======================================\n");

    example_basic_streaming()?;
    example_chunk_config();
    example_chunk_selection();
    example_result_structure();
    example_capture_api();

    // =========================================================================
    // Summary
    // =========================================================================
    println!("\n--- Benefits of Streaming with Captures ---");
    println!("* Process files larger than available RAM");
    println!("* Captures persist across streaming parse operations");
    println!("* Memory bounded by chunk_size * window_size");
    println!("* Single pass through data");
    println!("* Extract specific fields without loading entire file");

    println!("\n--- Performance Notes ---");
    println!("* Memory: O(chunk_size * window_size)");
    println!("* Captures: Accumulate during parse, available at end");
    println!("* For very large captures: process incrementally with reset()");

    println!("\n--- API Summary ---");
    println!("  let config = ChunkConfig {{ chunk_size: 65536, window_size: 2 }};");
    println!("  let mut parser = StreamingParser::new(&grammar, config);");
    println!("  let result = parser.parse_from_reader(&mut reader, &mut arena)?;");
    println!("  if let Some(captures) = result.capture_state {{");
    println!("    captures.get(\"name\").map(|v| v.get_text(input))");
    println!("  }}");

    Ok(())
}
