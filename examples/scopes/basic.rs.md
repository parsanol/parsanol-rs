# Scope Atoms - Rust Implementation

## How to Run

```bash
cargo run --example scopes/basic
```

## Code Walkthrough

### Why Scopes?

Without scopes, captures accumulate across the entire parse:

```rust
let grammar = GrammarBuilder::new()
    .rule("items", sequence([
        capture("temp", str("a")),
        capture("temp", str("b")),
        capture("temp", str("c")),
    ]))
    .build();
// Result: captures = ["temp" = "c"]  (last wins)
```

With scopes, inner captures are automatically discarded:

```rust
let grammar = GrammarBuilder::new()
    .rule("outer", sequence([
        capture("outer_name", str("prefix")),
        scope(sequence([
            capture("inner_name", re(r"[a-z]+")),  // Discarded
        ])),
    ]))
    .build();
// Result: captures = ["outer_name" = "prefix"]
```

### Scope Isolation

```rust
scope(sequence([
    capture("key", re(r"[a-z]+")),
    str("="),
    capture("value", re(r"[^\n]+")),
]))
```

Captures inside `scope()` are:
- Available during parsing for inner rules
- Automatically discarded when scope exits
- Never visible to outer capture inspection

### Nested Scopes

Scopes can be nested. Only the outermost captures persist:

```rust
let grammar = GrammarBuilder::new()
    .rule("nested", sequence([
        capture("level", str("L1")),      // Persisted
        scope(sequence([
            capture("level", str("L2")),  // Discarded
            scope(sequence([
                capture("level", str("L3")), // Discarded
            ])),
        ])),
    ]))
    .build();
// Result: captures = ["level" = "L1"]
```

### INI Configuration Example

Parse sections with isolated key-value pairs:

```rust
let grammar = GrammarBuilder::new()
    .rule("section", sequence([
        str("["),
        capture("section", re(r"[a-zA-Z_]+")),  // Persists
        str("]\n"),
        scope(repeat("kv_pair", 0, None)),      // Key-values isolated
    ]))
    .build();
```

Each section's `key` and `value` captures are isolated.

### Recursive Parsing

Scopes are essential for recursive structures:

```rust
let grammar = GrammarBuilder::new()
    .rule("paren", sequence([
        str("("),
        scope(choice([
            capture("content", re(r"[^()]+")),
            recursive("paren"),  // Each level has own scope
        ])),
        str(")"),
    ]))
    .build();
```

Each nesting level has isolated captures.

## Output Types

Scopes affect what `ParseResult` contains:

```rust
let result = parser.parse(input)?;

// Only captures from outside scopes are visible
for name in result.capture_names() {
    println!("{} = {:?}", name, result.get_capture(name, input));
}
```

## Design Decisions

### Generational Implementation

Scopes use "generational" tracking for O(1) push:

```rust
pub struct CaptureState {
    captures: HashMap<String, CaptureValue>,
    scope_depth: usize,
    scope_markers: Vec<usize>,  // Capture count at each scope entry
}
```

- `push_scope()`: O(1) - just records current capture count
- `pop_scope()`: O(c_scope) - removes captures added in scope

### When to Use Scopes

Use scopes when:
- Parsing nested structures with per-level context
- Processing repeated structures that should be independent
- Preventing capture name collisions
- Memory management for many captures

### When NOT to Use Scopes

Don't use scopes when:
- You need all captures accumulated
- There's no nesting or repetition
- Capture names are unique across the grammar

## Performance Notes

| Operation | Complexity |
|-----------|------------|
| push_scope | O(1) |
| pop_scope | O(c_scope) |
| Nesting overhead | ~2% per level |

Scopes are cheap - use them liberally to keep captures clean.
