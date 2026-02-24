# Simple XML Parser - Rust Implementation

## How to Run

```bash
cargo run --example simple-xml/basic --no-default-features
```

## Code Walkthrough

### Tag Name Parsing

Tag names follow standard XML identifier rules:

```rust
.rule("tag_name", re("[a-zA-Z][a-zA-Z0-9_-]*"))
```

Names start with a letter, then allow letters, digits, underscores, and hyphens.

### Opening and Closing Tags

Tags come in three varieties:

```rust
.rule(
    "open_tag",
    seq(vec![dynamic(str("<")), dynamic(re("[a-zA-Z][a-zA-Z0-9_-]*")), dynamic(str(">"))]),
)
.rule(
    "close_tag",
    seq(vec![dynamic(str("</")), dynamic(re("[a-zA-Z][a-zA-Z0-9_-]*")), dynamic(str(">"))]),
)
.rule(
    "self_closing_tag",
    seq(vec![dynamic(str("<")), dynamic(re("[a-zA-Z][a-zA-Z0-9_-]*")), dynamic(re("[ \\t]*/[ \\t]*")), dynamic(str(">"))]),
)
```

Self-closing tags (`<br />`) are handled separately from paired tags.

### Tag Matching Validation

XML requires matching open/close tags:

```rust
pub fn validate_xml(input: &str) -> XmlParseResult {
    let mut tag_stack: Vec<String> = Vec::new();

    while pos < chars.len() {
        if chars[pos] == '<' {
            if tag_content.starts_with('/') {
                // Closing tag
                if let Some(expected) = tag_stack.pop() {
                    if expected != tag_name {
                        return XmlParseResult {
                            valid: false,
                            message: format!("Tag mismatch: expected </{}>, found </{}>", expected, tag_name),
                        };
                    }
                }
            } else if !tag_content.ends_with('/') {
                // Opening tag (not self-closing)
                tag_stack.push(tag_name.to_string());
            }
        }
    }

    if !tag_stack.is_empty() {
        return XmlParseResult {
            valid: false,
            message: format!("Unclosed tags: {}", tag_stack.join(", ")),
        };
    }
}
```

A stack tracks open tags; mismatched or unclosed tags produce errors.

### Element Parsing

Elements are parsed into a tree structure:

```rust
pub fn parse_xml(input: &str) -> Result<Vec<XmlNode>, String> {
    while pos < chars.len() {
        // Collect text before tag
        while pos < chars.len() && chars[pos] != '<' {
            pos += 1;
        }
        if pos > text_start {
            nodes.push(XmlNode::Text(text));
        }

        // Parse tag
        if chars[pos] == '<' {
            // Extract tag content
        }
    }
}
```

## Output Types

```rust
pub enum XmlNode {
    Element {
        name: String,
        children: Vec<XmlNode>,
        text: Option<String>,
    },
    Text(String),
    EmptyElement { name: String },
}

pub struct XmlParseResult {
    pub valid: bool,
    pub message: String,
}
```

The tree structure preserves nesting, while `XmlParseResult` provides validation summary.

## Design Decisions

### Why Stack-Based Validation?

XML's nested structure naturally maps to a stack. Each opening tag pushes, each closing tag pops—mismatches are immediately detected.

### Why Not Full XML?

Full XML supports namespaces, CDATA, processing instructions, DOCTYPE, and entities. This parser focuses on the common subset for educational purposes.
