# JSON Parser (Pattern-Based Approach) - Rust Implementation

## How to Run

```bash
cargo run --example json/pattern --no-default-features
```

## Code Walkthrough

### Grammar Definition

The JSON grammar is defined using the pattern DSL with a choice of primitive types:

```rust
GrammarBuilder::new()
    .rule("json", choice(vec![
        dynamic(str("true")),
        dynamic(str("false")),
        dynamic(str("null")),
        dynamic(re(r#"-?[0-9]+(\.[0-9]+)?"#)),
        dynamic(re(r#""[^"]*""#)),
    ]))
    .build()
```

The grammar uses `choice` to try each alternative in order. The regex for numbers handles optional decimal portions, while strings match anything between quotes.

### AST to JsonValue Conversion

After parsing, the generic AST is converted to a typed JsonValue enum:

```rust
fn ast_to_json(node: &AstNode, arena: &AstArena, input: &str) -> Result<JsonValue, String> {
    match node {
        AstNode::InputRef { offset, length } => {
            let s = &input[*offset as usize..(*offset + *length) as usize];
            match s {
                "true" => Ok(JsonValue::Bool(true)),
                "false" => Ok(JsonValue::Bool(false)),
                "null" => Ok(JsonValue::Null),
                // ... handle strings and numbers
            }
        }
        // ... other node types
    }
}
```

The `InputRef` type references the original input string by offset and length, avoiding string copies until necessary.

### Serialized Output

The result is serialized to JSON for FFI transfer:

```rust
pub fn parse_to_json_string(input: &str) -> Result<String, String> {
    let value = ast_to_json(&ast, &arena, input)?;
    serde_json::to_string(&value).map_err(|e| e.to_string())
}
```

This produces a JSON string that can be passed across FFI boundaries to any host language.

## Output Types

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<JsonValue>),
    Object(HashMap<String, JsonValue>),
}
```

The `Serialize` and `Deserialize` derives enable JSON serialization for FFI.

## Design Decisions

### Why Serialize for FFI?

This approach is ideal for cross-language scenarios where Rust parses and another language (Ruby, Python, JavaScript) consumes the result. The serialization overhead is offset by Rust's parsing speed.

### Same Grammar as Transform Mode

This uses the SAME grammar as `transform.rs` for fair performance comparison between serialized and native output modes.
