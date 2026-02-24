# Streaming Parser - Rust Implementation

## How to Run

```bash
cargo run --example streaming/basic
```

## Code Walkthrough

### Row Grammar

Define grammar for parsing individual rows:

```rust
GrammarBuilder::new()
    .rule("field", re(r"[^,\n\r]+"))
    .rule("quoted_field", seq(vec![
        dynamic(str("\"")),
        dynamic(re(r#"(?:[^"]|"")*"#)),
        dynamic(str("\"")),
    ]))
    .build()
```

Each row is parsed independently.

### Chunk Processing

Read input in fixed-size chunks:

```rust
let mut buffer = [0u8; 64 * 1024];  // 64KB chunks
loop {
    let bytes_read = reader.read(&mut buffer)?;
    if bytes_read == 0 { break; }
    process_chunk(&buffer[..bytes_read]);
}
```

Memory usage is constant regardless of file size.

### Row Boundary Handling

Buffer incomplete rows between chunks:

```rust
let mut pending = String::new();
for chunk in chunks {
    pending.push_str(chunk);
    while let Some(newline_pos) = pending.find('\n') {
        let line = pending.drain(..=newline_pos).collect::<String>();
        parse_and_emit(&line);
    }
}
```

Complete rows are processed; partial rows wait for more data.

### Immediate Processing

Process parsed rows immediately without storing:

```rust
fn process_row(row: ParsedRow) {
    // Process row (write to DB, transform, etc.)
    // Row is dropped after this function returns
}
```

No accumulation of results in memory.

## Output Types

```
Input: 1GB CSV file
Memory: ~64KB (buffer size)
Processing: Row-by-row streaming
```

## Design Decisions

### Why Streaming?

Traditional parsers load entire input into memory. For GB-scale files, this is impractical or impossible.

### Why Chunk-Based?

Fixed buffer size means predictable memory usage. The buffer can be tuned based on available memory.

### When to Use

Use streaming when:
- File size exceeds available memory
- Processing real-time data streams
- Memory usage must be bounded
- Processing can be done row-by-row

### Trade-offs

- Cannot reference earlier parts of input
- No random access to parsed content
- Requires line/chunk boundaries in format
