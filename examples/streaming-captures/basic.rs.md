# Streaming Parser with Captures - Rust Implementation

## How to Run

```bash
cargo run --example streaming-captures/basic
```

## Code Walkthrough

### Basic Setup

Create a grammar with captures, then use the streaming parser:

```rust
use parsanol::portable::streaming::{StreamingParser, ChunkConfig};

let grammar = GrammarBuilder::new()
    .rule("email", capture("email", re(r"[^@\s]+@[^@\s]+")))
    .build();

let config = ChunkConfig {
    chunk_size: 64 * 1024,  // 64KB chunks
    window_size: 2,
};

let mut parser = StreamingParser::new(&grammar, config);
let mut arena = AstArena::for_input(64 * 1024);

let result = parser.parse_from_reader(&mut file, &mut arena)?;
```

### Accessing Captures

After parsing, access captures from the result:

```rust
if let Some(captures) = &result.capture_state {
    for name in captures.names() {
        if let Some(value) = captures.get(&name) {
            let text = value.get_text(input);
            println!("{} = {:?}", name, text);
        }
    }
}
```

### Log File Analysis

Extract fields from Apache-style logs:

```rust
let grammar = GrammarBuilder::new()
    .rule("log_line", sequence([
        capture("ip", re(r"\d+\.\d+\.\d+\.\d+")),
        str(" - - ["),
        capture("timestamp", re(r"[^\]]+")),
        str("] \""),
        capture("method", re(r"[A-Z]+")),
        str(" "),
        capture("path", re(r"[^\s]+")),
        re(r" [^\"]+\" "),
        capture("status", re(r"\d+")),
    ]))
    .build();
```

Process 10GB log files with ~2MB memory.

### CSV Processing

Extract specific columns:

```rust
let grammar = GrammarBuilder::new()
    .rule("row", sequence([
        re(r"[^,]*"),  // id
        str(","),
        capture("name", re(r"[^,]*")),
        str(","),
        capture("email", re(r"[^,@]+@[^,]+")),
        str("\n"),
    ]))
    .build();
```

### Chunk Configuration

```rust
let config = ChunkConfig {
    chunk_size: 1024 * 1024,  // 1MB chunks
    window_size: 2,           // 2-chunk sliding window
};
```

- `chunk_size`: Bytes per chunk (larger = fewer syscalls)
- `window_size`: Chunks kept in memory (larger = more backtracking)

### Memory Management

For very large captures:

```rust
// Option 1: Process incrementally
if let Some(captures) = &result.capture_state {
    if captures.names().len() > 100_000 {
        process_captures(captures);
        parser.reset();  // Clear captures
    }
}

// Option 2: Use scopes to limit capture accumulation
let grammar = GrammarBuilder::new()
    .rule("section", sequence([
        capture("section_name", re(r"[a-z]+")),
        scope(repeat("item", 0, None)),  // Items don't accumulate
    ]))
    .build();
```

### StreamingResult Fields

```rust
pub struct StreamingResult {
    pub ast: AstNode,               // Parse tree
    pub bytes_processed: usize,     // Total bytes read
    pub chunks_processed: usize,    // Number of chunks used
    pub peak_memory: usize,         // Maximum memory used
    pub cache_stats: (u64, u64, f64), // (hits, misses, hit_rate)
    pub capture_state: Option<CaptureState>,  // Extracted captures
}
```

## Chunk Size Selection

| Use Case | Recommended Size | Reason |
|----------|------------------|--------|
| Real-time feeds | 4-16 KB | Low latency priority |
| Log files | 256 KB - 1 MB | Throughput priority |
| Network streams | 8-64 KB | Balance latency/throughput |
| Large files | 1-4 MB | Reduce system calls |

## Window Size Selection

| Use Case | Window | Reason |
|----------|--------|--------|
| Sequential parsing | 1-2 | Minimal backtracking |
| Moderate backtracking | 2-3 | Default |
| Heavy backtracking | 4-5 | Complex grammars |

## Output Types

### CaptureState

```rust
impl CaptureState {
    /// Get all capture names
    pub fn names(&self) -> Vec<String>;

    /// Get a capture by name
    pub fn get(&self, name: &str) -> Option<CaptureValue>;

    /// Check if capture exists
    pub fn contains(&self, name: &str) -> bool;
}
```

### CaptureValue

```rust
impl CaptureValue {
    /// Get text from original input (zero-copy)
    pub fn get_text<'a>(&self, input: &'a str) -> &'a str;

    /// Get offset in input
    pub fn offset(&self) -> usize;

    /// Get length of captured text
    pub fn len(&self) -> usize;
}
```

## Design Decisions

### Capture Persistence

Captures accumulate during the streaming parse and are available at the end:

```rust
// Captures from all chunks are merged
let result = parser.parse_from_reader(&mut file, &mut arena)?;
let all_captures = result.capture_state;  // All captures from entire file
```

### Memory Bounds

Memory is bounded by:
```
memory = chunk_size * window_size + capture_state_size
```

For a 1GB file with 64KB chunks and window=2:
- Buffer memory: 64KB * 2 = 128KB
- Plus capture state (depends on number of captures)

### Scope Integration

Scopes work in streaming mode - captures inside scopes are discarded:

```rust
.rule("section", sequence([
    capture("name", re(r"[a-z]+")),  // Persists
    scope(repeat("item", 0, None)),  // Items isolated
]))
```

## Performance Notes

| Metric | Value |
|--------|-------|
| Memory overhead | chunk_size * window_size |
| Capture lookup | O(n) where n = number of captures |
| Streaming overhead | ~10% vs non-streaming |

**Optimization Tips**:
1. Use appropriate chunk size for your use case
2. Minimize window size when possible
3. Use scopes to limit capture accumulation
4. Process captures incrementally for very large files

## Error Handling

```rust
match parser.parse_from_reader(&mut reader, &mut arena) {
    Ok(result) => {
        // Access captures
        if let Some(captures) = result.capture_state {
            // Process captures
        }
    }
    Err(e) => {
        // Handle parse error
        println!("Parse error: {:?}", e);
    }
}
```
