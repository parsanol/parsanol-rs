# Error Reporting - Rust Implementation

## How to Run

```bash
cargo run --example error-reporting/basic --no-default-features
```

## Code Walkthrough

### Error Grammar Definition

The grammar defines what valid input looks like:

```rust
.rule("identifier", re("[a-zA-Z_][a-zA-Z0-9_]*"))
.rule("number", re("[0-9]+(\\.[0-9]+)?"))
.rule(
    "expr",
    choice(vec![
        dynamic(re("[0-9]+(\\.[0-9]+)?")),
        dynamic(re("[a-zA-Z_][a-zA-Z0-9_]*")),
    ]),
)
```

When parsing fails, we need to explain why.

### Position to Line/Column Conversion

Byte positions are converted to human-readable locations:

```rust
pub fn position_to_line_col(input: &str, pos: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;

    for (i, c) in input.chars().enumerate() {
        if i >= pos {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}
```

This enables "Error at line 3, column 5" messages.

### Context Extraction

Surrounding lines provide context:

```rust
pub fn get_context(input: &str, pos: usize, radius: usize) -> String {
    let lines: Vec<&str> = input.lines().collect();
    let (line, _col) = position_to_line_col(input, pos);

    let start_line = line.saturating_sub(radius);
    let end_line = (line + radius).min(lines.len());

    let mut context = String::new();
    for i in start_line..end_line {
        let prefix = if i + 1 == line { ">>> " } else { "    " };
        context.push_str(&format!("{}{:3}: {}\n", prefix, i + 1, lines[i]));
    }
    context
}
```

The error line is marked with `>>>` for visibility.

### Failure Point Detection

Finding where parsing failed:

```rust
fn find_failure_point(input: &str) -> usize {
    // Try parsing progressively longer prefixes
    for i in 1..=input.len() {
        let prefix = &input[..i];
        let mut arena = AstArena::for_input(prefix.len());
        let mut parser = PortableParser::new(&grammar, prefix, &mut arena);

        if parser.parse().is_err() {
            // Check if adding more characters helps
            if i < input.len() {
                let next_prefix = &input[..i + 1];
                // If next prefix parses, failure is transient
                if parser2.parse().is_ok() {
                    continue;
                }
            }
            return i.saturating_sub(1);
        }
    }
    input.len()
}
```

This finds the deepest point where parsing could succeed before failing.

### Error Information Structure

```rust
pub struct ParseErrorInfo {
    pub message: String,
    pub position: usize,
    pub line: usize,
    pub column: usize,
    pub context: String,
}
```

## Output Types

```rust
pub struct ParseErrorInfo {
    pub message: String,
    pub position: usize,
    pub line: usize,
    pub column: usize,
    pub context: String,
}
```

Complete error information for display or logging.

## Design Decisions

### Why Prefix-Based Failure Detection?

PEG parsers don't naturally report failure position. By testing progressively longer prefixes, we find where input becomes invalid.

### Why Separate Context Extraction?

Showing just the error position isn't helpful. Surrounding lines give developers the context needed to understand and fix errors.
