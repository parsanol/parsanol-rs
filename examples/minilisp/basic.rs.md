# Mini Lisp Parser - Rust Implementation

## How to Run

```bash
cargo run --example minilisp/basic --no-default-features
```

## Code Walkthrough

### Atom Types

Lisp atoms are the leaf nodes of expressions:

```rust
builder = builder.rule("identifier", re(r"[a-zA-Z=*][a-zA-Z=*_]*"));
builder = builder.rule("integer", re(r"[+-]?[0-9]+"));
builder = builder.rule("float", re(r"[+-]?[0-9]+(\.[0-9]+)?([eE][+-]?[0-9]+)?"));
builder = builder.rule("string", seq(vec![
    dynamic(str("\"")),
    dynamic(re(r"([^\"\\]|\\.)")),
    dynamic(str("\"")),
]));
```

Identifiers allow special chars like `=` and `*` for operators. Floats support scientific notation.

### List Rule

Lists are parenthesized sequences:

```rust
builder = builder.rule("list", seq(vec![
    dynamic(re(r"[ \t\n\r]*")),  // leading space
    dynamic(str("(")),
    dynamic(re(r"[ \t\n\r]*")),  // space after open
    dynamic(ref_("items")),
    dynamic(str(")")),
    dynamic(re(r"[ \t\n\r]*")),  // trailing space
]));
```

Whitespace handling around delimiters keeps the grammar clean.

### Items Rule

Items are zero or more expressions:

```rust
builder = builder.rule("items", dynamic(ref_("expr")).repeat(0));
```

Empty lists `()` are valid; the repeat(0) allows this.

### Expression Choice

Expressions are either lists or atoms:

```rust
builder = builder.rule("expr", choice(vec![
    dynamic(ref_("list")),
    dynamic(ref_("atom")),
]));
```

Order matters: list first because it starts with `(`, distinguishing from identifiers.

### AST Enum

The Rust enum represents all expression types:

```rust
pub enum LispExpr {
    Identifier(String),
    Integer(i64),
    Float(f64),
    String(String),
    List(Vec<LispExpr>),
}
```

Recursive structure via `Vec<LispExpr>` enables arbitrary nesting.

### Item Parsing

Space-separated items require careful handling:

```rust
fn parse_items(input: &str) -> Result<Vec<LispExpr>, String> {
    let mut depth = 0;
    let mut in_string = false;
    // Track nesting depth and string context
    // to split only at top-level spaces
}
```

Depth tracking handles nested lists; string tracking prevents splitting inside quotes.

## Output Types

```rust
// Simple list
LispExpr::List(vec![
    LispExpr::Identifier("+".to_string()),
    LispExpr::Integer(1),
    LispExpr::Integer(2),
])

// Nested structure
LispExpr::List(vec![
    LispExpr::Identifier("lambda".to_string()),
    LispExpr::List(vec![LispExpr::Identifier("x".to_string())]),
    LispExpr::List(vec![
        LispExpr::Identifier("*".to_string()),
        LispExpr::Identifier("x".to_string()),
        LispExpr::Identifier("x".to_string()),
    ]),
])
```

## Design Decisions

### Why Float Before Integer in Atom Choice?

Floats contain digits and might match integer pattern. Trying float first ensures `3.14` doesn't partially match as `3`.

### Why String Tracking in Parser?

Strings can contain spaces and parentheses. Without tracking, `"hello world"` would split incorrectly.

### Why Vec for Lists?

Rust's Vec is the standard heap-allocated sequence. It provides efficient push operations and iteration.

### Why Display Implementation?

Pretty-printing helps debugging and demonstrates AST structure. The Display trait integrates with Rust's formatting system.
