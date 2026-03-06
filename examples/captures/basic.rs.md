# Capture Atoms - Rust Implementation

## How to Run

```bash
cargo run --example captures/basic
```

## Code Walkthrough

### Basic Capture

Wrap any parslet with `capture()` to name the matched text:

```rust
let grammar = GrammarBuilder::new()
    .rule("greeting", capture("greeting", str("hello")))
    .build();

let result = parser.parse("hello world")?;
if let Some(text) = result.get_capture("greeting", input) {
    println!("Captured: {}", text);  // "hello"
}
```

The captured text is extracted without copying the input string.

### Nested Captures

Captures can be nested to capture both the whole and parts:

```rust
let grammar = GrammarBuilder::new()
    .rule("email", capture("email",
        sequence([
            capture("local", re(r"[a-zA-Z0-9._%+-]+")),
            str("@"),
            capture("domain", re(r"[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}")),
        ])
    ))
    .build();
```

This captures:
- `email`: the full email address
- `local`: the part before @
- `domain`: the part after @

### Log Line Parsing

Real-world example parsing Apache log format:

```rust
let grammar = GrammarBuilder::new()
    .rule("log_line", sequence([
        capture("ip", re(r"\d+\.\d+\.\d+\.\d+")),
        str(" - - ["),
        capture("timestamp", re(r"[^\]]+")),
        str("] \""),
        capture("method", re(r"[A-Z]+")),
        str(" "),
        capture("path", re(r"[^\s]+")),
        str("\" "),
        capture("status", re(r"\d+")),
    ]))
    .build();
```

Each field is captured by name for easy access.

### Inspection API

```rust
// Get all capture names
let names: Vec<String> = result.capture_names();

// Get specific capture
let value: Option<&str> = result.get_capture("name", input);

// Get all captures as HashMap
let all: HashMap<&str, &str> = result.captures(input);
```

### Raw Grammar API

For more control, use the raw `Grammar` API:

```rust
let mut grammar = Grammar::new();
let name = grammar.add_atom(Atom::Regex { pattern: r"[a-zA-Z]+".into() });
let name_capture = grammar.add_atom(Atom::Capture {
    name: "name".into(),
    atom: name,
});
```

## Output Types

The `ParseResult` provides capture inspection methods:

```rust
impl ParseResult {
    /// Returns names of all captures
    pub fn capture_names(&self) -> Vec<String>;

    /// Get text of a named capture (zero-copy)
    pub fn get_capture<'a>(&self, name: &str, input: &'a str) -> Option<&'a str>;

    /// Get all captures as HashMap
    pub fn captures<'a>(&'a self, input: &'a str) -> HashMap<&'a str, &'a str>;
}
```

## Design Decisions

### Zero-Copy Extraction

Captures store `(offset, length)` pairs, not string copies. The text is extracted
from the original input string when requested. This means:

- Minimal memory overhead
- Fast capture creation during parsing
- No string allocations until extraction

### Last-Name Wins

When multiple captures have the same name, the last one wins:

```rust
capture("item", str("apple")),
capture("item", str("banana")),  // This overwrites "apple"
```

For capturing multiple values, use repetition with inner captures.

### Cross-Backend Support

Captures work identically across all backends:
- **Packrat**: Native support
- **Bytecode**: Native VM instructions
- **Streaming**: Captures persist across chunks

## Performance Notes

| Operation | Complexity |
|-----------|------------|
| Create capture | O(1) |
| Get capture | O(n) where n = number of captures |
| Get all captures | O(n) |

For heavy capture usage, consider:
- Using scope atoms for nested contexts (automatic cleanup)
- Processing captures incrementally in streaming mode
