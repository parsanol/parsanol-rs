# Comments Parser - Rust Implementation

## How to Run

```bash
cargo run --example comments/basic --no-default-features
```

## Code Walkthrough

### Line Comment Parsing

Line comments start with `//` and continue to end of line:

```rust
.rule("line_comment", seq(vec![
    dynamic(str("//")),
    dynamic(re("[^\n\r]*")),
]))
```

The regex `[^\n\r]*` matches any character except newlines.

### Block Comment Parsing

Block comments use `/* */` delimiters:

```rust
.rule("block_comment", seq(vec![
    dynamic(str("/*")),
    dynamic(re("(?:[^*]|\\*(?![/))*")),  // Not */ or * followed by /
    dynamic(str("*/")),
]))
```

The pattern `(?:[^*]|\*(?![/])*` matches anything that isn't `*/` using negative lookahead.

### Comment Extraction

Comments are extracted by scanning character by character:

```rust
// Line comment
if pos + 1 < chars.len() && chars[pos] == '/' && chars[pos + 1] == '/' {
    pos += 2;
    let start = pos;
    while pos < chars.len() && chars[pos] != '\n' && chars[pos] != '\r' {
        pos += 1;
    }
    let comment: String = chars[start..pos].iter().collect();
    elements.push(CodeElement::Comment(Comment::Line(comment)));
}

// Block comment
if pos + 1 < chars.len() && chars[pos] == '/' && chars[pos + 1] == '*' {
    pos += 2;
    let start = pos;
    while pos + 1 < chars.len() && !(chars[pos] == '*' && chars[pos + 1] == '/') {
        pos += 1;
    }
    // ...
}
```

### Comment Stripping

Comments can be removed from code:

```rust
pub fn strip_comments(input: &str) -> String {
    let elements = parse_with_comments(input).unwrap_or_default();
    let mut result = String::new();

    for element in elements {
        match element {
            CodeElement::Identifier(ident) => result.push_str(&ident),
            CodeElement::Newline => result.push('\n'),
            CodeElement::Comment(_) => { /* Skip comments */ }
        }
    }
    result.trim().to_string()
}
```

## Output Types

```rust
pub enum Comment {
    Line(String),
    Block(String),
}

pub enum CodeElement {
    Identifier(String),
    Comment(Comment),
    Newline,
}
```

Comments are distinguished from identifiers, preserving the code structure.

## Design Decisions

### Why Not Nested Block Comments?

Standard C-style block comments don't nest. For nested comments (like Pascal), a counter-based approach is needed instead of simple pattern matching.

### Why Manual Parsing After Grammar Check?

The grammar validates structure, but extracting comment content requires tracking positions. Manual parsing after validation provides precise content extraction.
