# TOML Parser - Rust Implementation

## How to Run

```bash
cargo run --example toml/basic --no-default-features
```

## Code Walkthrough

### String Parsing

TOML supports basic and literal strings:

```rust
.rule(
    "basic_string",
    seq(vec![
        dynamic(str("\"")),
        dynamic(choice(vec![
            dynamic(re(r#"\\."#)),     // any escape
            dynamic(re(r#"[^"\\]+"#)), // normal chars
        ]).many()),
        dynamic(str("\"")),
    ]),
)
.rule(
    "literal_string",
    seq(vec![
        dynamic(str("'")),
        dynamic(re("[^']*").or(str("''"))),
        dynamic(str("'")),
    ]),
)
```

Basic strings support escapes (`\n`, `\t`); literal strings are verbatim.

### Number Parsing

Multiple number formats are supported:

```rust
.rule("integer", re("-?[0-9]+"))
.rule("float", re("-?[0-9]+\\.[0-9]+([eE][+-]?[0-9]+)?"))
.rule("hex_int", re("0x[0-9a-fA-F]+"))
.rule("oct_int", re("0o[0-7]+"))
.rule("bin_int", re("0b[01]+"))
```

Hex, octal, and binary use `0x`, `0o`, `0b` prefixes.

### Array Parsing

Arrays can contain mixed types:

```rust
.rule(
    "array",
    seq(vec![
        dynamic(str("[")),
        dynamic(re("[ \t\r\n]*")),
        dynamic(/* items separated by commas */),
        dynamic(re("[ \t\r\n]*")),
        dynamic(str("]")),
    ]),
)
```

Arrays can span multiple lines.

### Table Parsing

Tables define sections:

```rust
.rule(
    "table",
    seq(vec![
        dynamic(str("[")),
        dynamic(re("[a-zA-Z_][a-zA-Z0-9_.-]*")), // table name
        dynamic(str("]")),
    ]),
)
.rule(
    "array_table",
    seq(vec![
        dynamic(str("[[")),
        dynamic(re("[a-zA-Z_][a-zA-Z0-9_.-]*")),
        dynamic(str("]]")),
    ]),
)
```

Array tables (`[[items]]`) create arrays of tables.

### Document Parsing

Documents are parsed line by line:

```rust
pub fn parse_toml(input: &str) -> Result<String, String> {
    let mut doc = TomlDocument {
        root: HashMap::new(),
        tables: HashMap::new(),
    };
    let mut current_table = String::new();

    for line in input.lines() {
        // Table header
        if line.starts_with('[') && line.ends_with(']') {
            current_table = line[1..len-1].to_string();
            continue;
        }

        // Key-value pair
        if let Some((key, value)) = line.split_once('=') {
            if current_table.is_empty() {
                doc.root.insert(key, parse_toml_value(value));
            } else {
                doc.tables.entry(current_table.clone()).or_default().insert(key, value);
            }
        }
    }
}
```

## Output Types

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TomlValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Array(Vec<TomlValue>),
    Table(HashMap<String, TomlValue>),
}

pub struct TomlDocument {
    pub root: HashMap<String, TomlValue>,
    pub tables: HashMap<String, HashMap<String, TomlValue>>,
}
```

The `#[serde(untagged)]` allows flexible JSON serialization.

## Design Decisions

### Why Untagged Enum for Values?

TOML values are dynamically typed. Untagged enums serialize cleanly without wrapper objects.

### Why Separate Root and Tables?

Root-level key-values are separate from tabled values, matching TOML's semantic structure.
