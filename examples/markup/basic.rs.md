# Markup Parser - Rust Implementation

## How to Run

```bash
cargo run --example markup/basic --no-default-features
```

## Code Walkthrough

### Inline Formatting

Inline elements use delimiter characters:

```rust
.rule(
    "bold",
    seq(vec![dynamic(str("**")), dynamic(re("[^*]+")), dynamic(str("**"))]),
)
.rule(
    "italic",
    seq(vec![dynamic(str("*")), dynamic(re("[^*]+")), dynamic(str("*"))]),
)
.rule(
    "code",
    seq(vec![dynamic(str("`")), dynamic(re("[^`]+")), dynamic(str("`"))]),
)
```

Each format has opening and closing delimiters with content between.

### Header Parsing

Headers use `#` prefix for levels 1-3:

```rust
.rule("h1", seq(vec![dynamic(str("# ")), dynamic(re("[^\\n]+"))]))
.rule("h2", seq(vec![dynamic(str("## ")), dynamic(re("[^\\n]+"))]))
.rule("h3", seq(vec![dynamic(str("### ")), dynamic(re("[^\\n]+"))]))
```

The space after `#` is required; header text continues to end of line.

### List Item Parsing

List items use `-` or `*` prefixes:

```rust
.rule(
    "list_item",
    seq(vec![
        dynamic(re("[ \t]*[-*][ \t]+")),  // marker with required space
        dynamic(re("[^\\n]+")),           // item text
    ]),
)
```

Whitespace before the marker allows nested lists (visually).

### Block Element Selection

Blocks are parsed in priority order:

```rust
.rule(
    "block",
    choice(vec![
        dynamic(seq(vec![dynamic(str("# ")), dynamic(re("[^\\n]+"))])),     // h1
        dynamic(seq(vec![dynamic(str("## ")), dynamic(re("[^\\n]+"))])),    // h2
        dynamic(seq(vec![dynamic(str("### ")), dynamic(re("[^\\n]+"))])),   // h3
        dynamic(seq(vec![dynamic(re("[ \t]*[-*][ \t]+")), dynamic(re("[^\\n]+"))])),  // list
        dynamic(re("[^\\n]+")),  // paragraph
    ]),
)
```

Headers are tried first; if none match, the line is a paragraph.

### Inline Formatting Processing

Inline elements are parsed character by character:

```rust
match c {
    '*' if chars.peek() == Some(&'*') => {
        chars.next();  // consume second *
        // Read until closing **
        let mut bold_text = String::new();
        while let Some(&next) = chars.peek() {
            if next == '*' {
                chars.next();
                if chars.peek() == Some(&'*') {
                    chars.next();
                    break;
                }
                bold_text.push('*');
            } else {
                bold_text.push(chars.next().unwrap());
            }
        }
        elements.push(MarkupElement::Bold(bold_text));
    }
    // ... italic and code similar
}
```

## Output Types

```rust
pub enum MarkupElement {
    Header { level: u8, text: String },
    Paragraph(String),
    ListItem(String),
    Bold(String),
    Italic(String),
    Code(String),
}

pub struct MarkupDocument {
    pub elements: Vec<MarkupElement>,
}
```

Elements are flat; nesting is implied by context.

## Design Decisions

### Why Flat Element Structure?

Full AST nesting (paragraphs containing bold/italic) is complex. A flat structure is simpler for basic processing and transformation.

### Why Separate Block and Inline Parsing?

Block structure is line-based; inline formatting is character-based. Separating the passes simplifies both.
