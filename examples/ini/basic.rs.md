# INI Parser - Rust Implementation

## How to Run

```bash
cargo run --example ini/basic --no-default-features
```

## Code Walkthrough

### Key-Value Pair Parsing

INI entries are `key = value` format:

```rust
.rule("key", re("[a-zA-Z_][a-zA-Z0-9_]*"))
.rule("value", re("[^\\n]+"))
.rule(
    "pair",
    seq(vec![
        dynamic(re("[a-zA-Z_][a-zA-Z0-9_]*")), // key
        dynamic(re("[ \t]*=[ \t]*")),          // =
        dynamic(re("[^\\n]+")),                // value
    ]),
)
```

Whitespace around `=` is optional.

### Section Header Parsing

Sections group key-value pairs:

```rust
.rule("section_name", re("[a-zA-Z_][a-zA-Z0-9_.]*"))
.rule(
    "section",
    seq(vec![
        dynamic(str("[")),
        dynamic(re("[a-zA-Z_][a-zA-Z0-9_.]*")),
        dynamic(str("]")),
    ]),
)
```

Dots allow dotted section names like `[database.mysql]`.

### Comment Handling

Comments start with `;` or `#`:

```rust
.rule("comment", re("[;#][^\\n]*"))
```

Comments extend to end of line and are ignored.

### Line Processing

INI files are processed line by line:

```rust
pub fn parse_ini(input: &str) -> Result<IniConfig, String> {
    let mut config = IniConfig::default();
    let mut current_section: Option<String> = None;

    for line in input.lines() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }

        // Section header
        if line.starts_with('[') && line.ends_with(']') {
            current_section = Some(line[1..line.len() - 1].to_string());
            continue;
        }

        // Key-value pair
        if let Some((key, value)) = line.split_once('=') {
            match &current_section {
                Some(section) => config.sections.entry(section.clone()).or_default().insert(key, value),
                None => config.global.insert(key, value),
            };
        }
    }
    Ok(config)
}
```

## Output Types

```rust
pub struct IniConfig {
    /// Global key-value pairs (before any section)
    pub global: HashMap<String, String>,
    /// Sections with their key-value pairs
    pub sections: HashMap<String, HashMap<String, String>>,
}
```

Global pairs appear before any section header; sectioned pairs are grouped.

## Design Decisions

### Why Not Use Grammar for Full Parsing?

The grammar validates structure, but INI's line-based nature makes manual parsing simpler. Splitting on `=` and `[` is more straightforward than complex grammar rules.

### Why Preserve Section Order?

Standard HashMap doesn't preserve order. For ordered sections, use `IndexMap` instead.
