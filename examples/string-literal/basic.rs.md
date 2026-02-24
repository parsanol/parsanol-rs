# String Literal Parser - Rust Implementation

## How to Run

```bash
cargo run --example string-literal/basic --no-default-features
```

## Code Walkthrough

### String Grammar Definition

Strings are quoted with escape sequence support:

```rust
.rule("escape", seq(vec![dynamic(str("\\")), dynamic(re("."))]))
.rule(
    "string",
    seq(vec![
        dynamic(str("\"")),
        dynamic(re("(?:\\\\.|[^\"])*")),  // Escape or non-quote
        dynamic(str("\"")),
    ]),
)
```

The regex `(?:\\\\.|[^\"])*` matches either an escape sequence or any non-quote character.

### Escape Sequence Processing

Escape sequences are processed character by character:

```rust
pub fn parse_string_literal(input: &str) -> Result<String, String> {
    let content = &input[1..input.len() - 1];  // Strip quotes
    let mut result = String::new();
    let chars: Vec<char> = content.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            let next = chars[i + 1];
            match next {
                'n' => result.push('\n'),
                't' => result.push('\t'),
                'r' => result.push('\r'),
                '\\' => result.push('\\'),
                '"' => result.push('"'),
                '0' => result.push('\0'),
                c => result.push(c),  // Unknown escape, include as-is
            }
            i += 2;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    Ok(result)
}
```

### Literal Type Detection

The parser distinguishes integers from strings:

```rust
pub fn parse_literal(input: &str) -> Result<Literal, String> {
    if input.starts_with('"') && input.ends_with('"') {
        let s = parse_string_literal(input)?;
        Ok(Literal::String(s))
    } else if input.chars().all(|c| c.is_ascii_digit() || c == '-') {
        let n = parse_integer_literal(input)?;
        Ok(Literal::Integer(n))
    } else {
        Err(format!("Unknown literal type: {}", input))
    }
}
```

## Output Types

```rust
pub enum Literal {
    Integer(i64),
    String(String),
}
```

The typed enum allows direct use of parsed values without runtime type checking.

## Design Decisions

### Why Not Use Regex for Escape Processing?

Escape sequences like `\n` require semantic interpretation (newline, not literal backslash-n). Regex matches text patterns but doesn't interpret them.

### Why Handle Unknown Escapes?

Unknown escape sequences (like `\x`) are included as-is for robustness. This matches most programming language behavior.
