# Nested Error Reporting - Rust Implementation

## How to Run

```bash
cargo run --example nested-errors/basic --no-default-features
```

## Code Walkthrough

### Error Tree Structure

Errors form a tree with nested causes:

```rust
pub struct ErrorNode {
    pub message: String,
    pub position: usize,
    pub children: Vec<ErrorNode>,
}
```

The tree structure mirrors the grammar's choice points.

### Tree Construction

Build the tree from parse results:

```rust
fn build_error_tree(input: &str, grammar: &Grammar) -> ErrorNode {
    match parser.parse() {
        Ok(_) => ErrorNode::new("Success", 0),
        Err(e) => {
            let mut root = ErrorNode::new("Parse failed", 0);
            root.add_child(ErrorNode::new("Expected 'end'", pos));
            root
        }
    }
}
```

Root contains overall failure; children show specific causes.

### ASCII Tree Formatting

Display errors as a visual tree:

```rust
fn format_tree(&self, prefix: &str, is_last: bool) -> String {
    let connector = if is_last { "`- " } else { "|- " };
    let result = format!("{}{}{} (pos {})\n",
        prefix, connector, self.message, self.position);

    for (i, child) in self.children.iter().enumerate() {
        let is_last_child = i == self.children.len() - 1;
        result.push_str(&child.format_tree(&new_prefix, is_last_child));
    }
    result
}
```

Recursive formatting with branch connectors.

### Example Output

```
`- Parse failed (pos 0)
    |- Expected 'end' keyword (pos 25)
    |- In reference expression (pos 20)
    `- Expected identifier (pos 15)
```

Each indentation level shows deeper failure causes.

### Position Tracking

Positions help locate errors:

```rust
let pos = error_str.find("at position").map(|p| {
    let rest = &error_str[p..];
    rest.split_whitespace()
        .nth(2)
        .and_then(|s| s.trim_end_matches(':').parse::<usize>().ok())
        .unwrap_or(0)
}).unwrap_or(0);
```

Extract byte position from error message for display.

## Design Decisions

### Why Tree Structure?

PEG parsers have nested alternatives. A tree naturally represents which alternatives were tried at each point.

### Why Children Array?

Multiple causes can contribute to a single failure. The array captures all relevant information.

### Why ASCII Art?

ASCII trees work in any terminal. No special rendering needed for basic debugging.

### Why Position on Every Node?

Each failure has a specific location. Showing position at every level helps pinpoint issues.

## Comparison with Deepest Errors

| Feature | Deepest | Nested |
|---------|---------|--------|
| Single point | Yes | No (tree) |
| All alternatives | No | Yes |
| Output size | Small | Large |
| Use case | User errors | Grammar debugging |

Deepest for users; nested for developers.
