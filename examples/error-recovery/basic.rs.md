# Error Recovery - Rust Implementation

## How to Run

```bash
cargo run --example error-recovery --no-default-features
```

## Code Walkthrough

### Parse Result Structure

Track both successful parses and errors:

```rust
struct ParseResult {
    value: Option<String>,
    errors: Vec<ParseError>,
    recovered: bool,
}

struct ParseError {
    position: usize,
    message: String,
    context: String,
}
```

Each result knows if it succeeded, what errors occurred, and whether recovery happened.

### Recovery Strategy: Synchronization Points

Find safe points to continue parsing:

```rust
fn find_sync_point(&self, input: &str, pos: usize) -> usize {
    // Find next delimiter or whitespace as sync point
    let slice = &input[pos..];
    for (i, c) in slice.char_indices() {
        if c == ',' || c == ';' || c == '\n' || c.is_whitespace() {
            return pos + i + 1;
        }
    }
    input.len()
}
```

Skip to known delimiters rather than arbitrary positions.

### Error Collection Mode

Gather all errors before reporting:

```rust
fn collect_errors(input: &str, grammar: &Grammar) -> Vec<ParseError> {
    let mut errors = Vec::new();
    let mut pos = 0;

    while pos < input.len() {
        if parse_fails_at(input, pos) {
            errors.push(record_error(input, pos));
            pos += 1;  // Advance by one character
        } else {
            pos += consumed_length;
        }
    }
    errors
}
```

Report all issues at once instead of one-at-a-time.

### Context Preservation

Show surrounding text for debugging:

```rust
fn get_context(&self, input: &str, pos: usize) -> String {
    let start = pos.saturating_sub(10);
    let end = (pos + 10).min(input.len());
    format!("...{}[HERE]{}...", &input[start..pos], &input[pos..end])
}
```

Users see where the error occurred with surrounding context.

## Output Types

```
Input: 1+2, 3*, 5+6

Segment 1:
  Parsed: 1+2

Segment 2:
  Error at 5: Failed to parse at this position
  Context: ...1+2, [HERE]3*, 5+6...
  (Recovered and continued)

Segment 3:
  Parsed: 5+6
```

## Design Decisions

### Why Synchronization Points?

Arbitrary resumption can cause cascading errors. Synchronization at delimiters ensures clean restart.

### Why Collect All Errors?

Users prefer fixing multiple issues at once. One-error-at-a-time is frustrating for large files.

### When to Use

Use error recovery when:
- Building IDE/editor integrations
- Processing user-generated content
- Validating configuration files
- Creating linting tools

### Trade-offs

- More complex parser implementation
- May report false positives after recovery
- Context preservation adds overhead
- Not all grammars support easy recovery
