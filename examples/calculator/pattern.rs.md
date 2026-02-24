# Calculator (Pattern-Based Approach) - Rust Implementation

## How to Run

```bash
cargo run --example calculator/pattern --no-default-features
```

## Code Walkthrough

### Expression Grammar

The calculator grammar handles arithmetic with proper operator precedence:

```rust
GrammarBuilder::new()
    .rule("number", re(r#"[0-9]+"#))
    .rule("add_op", str("+").or(str("-")))
    .rule("mul_op", str("*").or(str("/")))
    .rule("expr", ...)  // Handles precedence
    .build()
```

The grammar is defined declaratively using the pattern DSL, making it easy to understand and modify.

### Operator Precedence

Multiplication and division bind tighter than addition and subtraction:

```rust
// expr = term (('+' | '-') term)*
// term = factor (('*' | '/') factor)*
// factor = number | '(' expr ')'
```

This ensures `1+2*3` parses as `1+(2*3)`, not `(1+2)*3`.

### Parenthesized Expressions

Parentheses override precedence:

```rust
.rule("factor", choice(vec![
    dynamic(re(r#"[0-9]+"#)),
    dynamic(seq(vec![dynamic(str("(")), dynamic(expr), dynamic(str(")"))])),
]))
```

The recursive nature allows arbitrary nesting: `((1+2)*(3+4))`.

### Serialized Output

The AST is serialized to JSON for FFI transfer:

```rust
pub fn parse_to_json(input: &str) -> Result<String, String> {
    let result = parser.parse()?;
    serde_json::to_string(&result).map_err(|e| e.to_string())
}
```

Output example: `{"BinOp":{"left":{"Number":1},"op":"+","right":{"Number":2}}}`

## Output Types

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Expr {
    Number(i64),
    BinOp {
        left: Box<Expr>,
        op: String,
        right: Box<Expr>,
    },
}
```

The JSON structure mirrors this enum, with type discriminators for each variant.

## Design Decisions

### Why Serialized for FFI?

This approach is ideal when:
- Rust parses, another language consumes (Ruby, Python, JavaScript)
- Output needs to cross process/network boundaries
- You want language-agnostic result format

### Same Grammar as Transform Mode

This uses the SAME grammar as `transform.rs` for fair performance comparison between serialized and native output modes.
