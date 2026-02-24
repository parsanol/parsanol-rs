# CSV Parser (Transform-Based Approach) - Rust Implementation

## How to Run

```bash
cargo run --example csv/transform --no-default-features
```

## Code Walkthrough

### Native Rust Types

This approach produces native Rust types without serialization:

```rust
#[derive(Debug, Clone)]
pub struct CsvRow {
    pub fields: Vec<String>,
}

pub struct CsvData {
    pub headers: Option<CsvRow>,
    pub rows: Vec<CsvRow>,
}
```

The types separate headers from data rows, enabling type-safe processing.

### Direct Field Extraction

Fields are extracted directly into String values:

```rust
fn extract_fields(input: &str) -> Result<Vec<String>, String> {
    let mut fields = Vec::new();
    let mut in_quotes = false;
    let mut current = String::new();
    // ... character-by-character parsing
}
```

This avoids intermediate AST nodes and directly produces usable data.

### Header Detection

The first row can be treated as headers:

```rust
pub fn parse_with_headers(input: &str) -> Result<CsvData, String> {
    let mut lines = parse_rows(input)?.into_iter();
    let headers = lines.next().map(|row| CsvRow { fields: row.fields });
    Ok(CsvData { headers, rows: lines.collect() })
}
```

This enables column-name-based access to data.

### Zero-Copy Considerations

The parser uses string slices where possible:

```rust
pub struct FieldRef<'a> {
    pub value: &'a str,
    pub quoted: bool,
}
```

For large files, zero-copy parsing significantly reduces memory allocation.

## Output Types

```rust
#[derive(Debug, Clone)]
pub struct CsvRow {
    pub fields: Vec<String>,
}

pub struct CsvData {
    pub headers: Option<CsvRow>,
    pub rows: Vec<CsvRow>,
}
```

Native Rust types with no serialization overhead.

## Design Decisions

### Why Native Types?

For pure Rust applications, native types provide:
- Zero serialization overhead
- Type-safe pattern matching
- Compile-time guarantees
- Maximum performance

### Comparison with Pattern Mode

| Aspect | Pattern Mode | Transform Mode |
|--------|-------------|----------------|
| Output | JSON string | Native Rust |
| Memory | Higher (serialized) | Lower (direct) |
| Use case | FFI/Cross-language | Pure Rust |

Use transform mode for Rust-only applications where performance matters.
