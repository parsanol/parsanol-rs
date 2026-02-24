# S-Expression Parser - Rust Implementation

## How to Run

```bash
cargo run --example sexp/basic --no-default-features
```

## Code Walkthrough

### Atom Parsing

Atoms are the primitive values in S-expressions:

```rust
.rule("symbol", re("[a-zA-Z_+\\-*/=<>!][a-zA-Z0-9_+\\-*/=<>!]*"))
.rule("number", re("-?[0-9]+(\\.[0-9]+)?"))
.rule("string", re("\"[^\"]*\""))

.rule(
    "atom",
    choice(vec![
        dynamic(re("[a-zA-Z_+\\-*/=<>!][a-zA-Z0-9_+\\-*/=<>!]*")),  // symbol
        dynamic(re("-?[0-9]+(\\.[0-9]+)?")),                        // number
        dynamic(re("\"[^\"]*\"")),                                  // string
    ]),
)
```

Symbols can include operator characters (`+`, `-`, `*`, etc.) for Lisp-style operators.

### List Parsing

Lists are parenthesized sequences:

```rust
.rule(
    "list",
    seq(vec![
        dynamic(str("(")),
        dynamic(re("[ \t\n\r]*")),  // optional whitespace
        dynamic(re("\\)*")),         // items (simplified)
        dynamic(str(")")),
    ]),
)
```

### Recursive Parsing

S-expressions are parsed recursively by tracking nesting depth:

```rust
fn parse_sexp_list(input: &str) -> Result<Vec<SExp>, String> {
    let mut items = Vec::new();
    let mut depth = 0;
    let mut current = String::new();

    for c in input.chars() {
        match c {
            '(' => {
                depth += 1;
                current.push(c);
            }
            ')' => {
                depth -= 1;
                current.push(c);
            }
            ' ' | '\t' | '\n' | '\r' if depth == 0 => {
                if !current.is_empty() {
                    items.push(parse_sexp(&current)?);
                    current.clear();
                }
            }
            _ => current.push(c),
        }
    }
    Ok(items)
}
```

When depth is 0 and whitespace is found, the current item is complete.

### Value Detection

Values are detected by their format:

```rust
pub fn parse_sexp(input: &str) -> Result<SExp, String> {
    if input.starts_with('(') && input.ends_with(')') {
        return Ok(SExp::List(parse_sexp_list(inner)?));
    }
    if input.starts_with('"') && input.ends_with('"') {
        return Ok(SExp::String(input[1..len-1].to_string()));
    }
    if let Ok(n) = input.parse::<i64>() {
        return Ok(SExp::Int(n));
    }
    if let Ok(n) = input.parse::<f64>() {
        return Ok(SExp::Float(n));
    }
    Ok(SExp::Symbol(input.to_string()))
}
```

## Output Types

```rust
pub enum SExp {
    Symbol(String),
    Int(i64),
    Float(f64),
    String(String),
    List(Vec<SExp>),
}
```

The recursive enum supports arbitrarily nested S-expressions.

## Design Decisions

### Why Depth Tracking?

Nested lists require tracking opening/closing parentheses. Depth tracking handles arbitrary nesting without separate tokenization.

### Why Separate Int and Float?

Type distinction enables precise evaluation. In Lisp, `1` and `1.0` are different types with different behaviors.
