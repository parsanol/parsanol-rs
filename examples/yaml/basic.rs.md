# YAML Parser - Rust Implementation

## How to Run

```bash
cargo run --example yaml/basic --no-default-features
```

## Code Walkthrough

### Scalar Type Parsing

YAML supports multiple scalar types:

```rust
.rule("null_val", str("null").or(str("~")).or(str("")))
.rule(
    "bool_val",
    str("true").or(str("false")).or(str("yes")).or(str("no")),
)
.rule(
    "number",
    choice(vec![
        dynamic(re("-?[0-9]+\\.[0-9]+")), // float
        dynamic(re("-?[0-9]+")),          // integer
    ]),
)
```

Null can be `null`, `~`, or empty; booleans accept `yes`/`no`.

### String Parsing

Strings can be quoted or plain:

```rust
.rule(
    "double_quoted",
    seq(vec![
        dynamic(str("\"")),
        dynamic(re("[^\"\\\\]*(\\\\.[^\"\\\\]*)*")),
        dynamic(str("\"")),
    ]),
)
.rule(
    "single_quoted",
    seq(vec![
        dynamic(str("'")),
        dynamic(re("[^']*")),
        dynamic(str("'")),
    ]),
)
.rule("plain_string", re("[^:#\\[\\]{}\\n\\r][^#\\n\\r]*"))
```

Double-quoted strings support escapes; single-quoted are literal.

### Inline Collections

YAML supports inline arrays and objects:

```rust
.rule(
    "flow_seq",
    seq(vec![
        dynamic(str("[")),
        dynamic(/* items separated by commas */),
        dynamic(str("]")),
    ]),
)
.rule(
    "flow_map",
    seq(vec![
        dynamic(str("{")),
        dynamic(/* key: value pairs */),
        dynamic(str("}")),
    ]),
)
```

These are JSON-like inline syntax.

### Key-Value Parsing

YAML uses `:` for key-value separation:

```rust
.rule(
    "keyval",
    seq(vec![
        dynamic(re("[a-zA-Z_][a-zA-Z0-9_-]*|\"[^\"]+\"|'[^']+'")), // key
        dynamic(re(":[ \t]*")),                                    // colon
        dynamic(choice(vec![
            dynamic(re("[^#\\n\\r]+")), // inline value
            dynamic(re("")),            // empty (nested)
        ])),
    ]),
)
```

Empty values indicate nested content on following lines.

### List Item Parsing

Lists use `-` prefix:

```rust
.rule(
    "list_item",
    seq(vec![
        dynamic(re("[ \t]*")),
        dynamic(str("-")),
        dynamic(re("[ \t]+")),
        dynamic(/* value */),
    ]),
)
```

Indentation determines nesting level.

### Value Parsing

Values are parsed by type detection:

```rust
fn parse_yaml_value(s: &str) -> YamlValue {
    // Null
    if s.is_empty() || s == "null" || s == "~" {
        return YamlValue::Null;
    }

    // Boolean
    if s == "true" || s == "yes" {
        return YamlValue::Boolean(true);
    }

    // Inline array: [a, b, c]
    if s.starts_with('[') && s.ends_with(']') {
        let items: Vec<YamlValue> = inner.split(',')
            .map(|item| parse_yaml_value(item.trim()))
            .collect();
        return YamlValue::Array(items);
    }

    // ... more type detection
}
```

## Output Types

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum YamlValue {
    Null,
    Boolean(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Array(Vec<YamlValue>),
    Object(HashMap<String, YamlValue>),
}
```

The recursive enum supports arbitrary nesting.

## Design Decisions

### Why Untagged Enum?

Like TOML, YAML values are dynamically typed. Untagged enums serialize to natural JSON.

### Why Not Full Indentation Parsing?

Full YAML indentation parsing is complex. This subset uses simplified line-by-line processing for common cases.
