# CSV Parser (Pattern-Based Approach) - Rust Implementation

## How to Run

```bash
cargo run --example csv/pattern --no-default-features
```

## Code Walkthrough

### Grammar Structure

The CSV grammar handles fields separated by commas, with support for quoted fields:

```rust
GrammarBuilder::new()
    .rule("field", choice(vec![
        dynamic(re(r#""(?:[^"]|"")*""#)),  // Quoted field
        dynamic(re(r#"[^,\n\r]+"#)),         // Unquoted field
    ]))
    .rule("row", seq(vec![
        dynamic(choice(vec![...])),  // field
        dynamic(seq(vec![dynamic(str(",")), dynamic(choice(vec![...]))]).many()),  // , field*
    ]))
    .build()
```

The grammar distinguishes between quoted and unquoted fields. Quoted fields use double-quotes with `""` for escaped quotes.

### Quoted Field Handling

Quoted fields can contain commas and newlines:

```rust
.rule("quoted_field", re(r#""(?:[^"]|"")*""#))
```

The regex `""` within a quoted field represents an escaped double-quote character (RFC 4180 standard).

### Escape Sequence Processing

After parsing, escaped quotes are converted:

```rust
fn process_escaped_quotes(s: &str) -> String {
    s.replace(r#""""#, r#"""#)
}
```

The standard CSV escape mechanism doubles quotes: `"hello ""world"""` becomes `hello "world"`.

### Serialized Output

The result is serialized to JSON for FFI transfer:

```rust
pub fn parse_to_json_string(input: &str) -> Result<String, String> {
    let rows = parse_csv(input)?;
    serde_json::to_string(&rows).map_err(|e| e.to_string())
}
```

This produces a JSON array of arrays, suitable for any language to consume.

## Output Types

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CsvRow {
    pub fields: Vec<String>,
}

pub type CsvData = Vec<CsvRow>;
```

The serialized form is a JSON array of row objects, each containing a `fields` array.

## Design Decisions

### Why Regex for Quoted Fields?

Using regex for quoted fields is more efficient than a recursive grammar for this simple pattern. The RFC 4180 standard is well-defined and maps cleanly to regex.

### Row-by-Row Processing

The parser processes row by row, making it suitable for streaming large files in the future.
