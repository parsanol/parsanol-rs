# ERB Template Parser - Rust Implementation

## How to Run

```bash
cargo run --example erb/basic --no-default-features
```

## Code Walkthrough

### ERB Tag Types

ERB has three tag types with different purposes:

```rust
.rule(
    "expression",  // <%= ... %>  - Output expression value
    seq(vec![dynamic(str("<%=")), dynamic(re("[^%]*")), dynamic(str("%>"))]),
)
.rule(
    "comment",     // <%# ... %>  - Comment (not rendered)
    seq(vec![dynamic(str("<%#")), dynamic(re("[^%]*")), dynamic(str("%>"))]),
)
.rule(
    "code",        // <% ... %>   - Execute code
    seq(vec![dynamic(str("<%")), dynamic(re("[^%]*")), dynamic(str("%>"))]),
)
```

### Template Parsing

Templates are parsed by scanning for `<%` markers:

```rust
while pos < chars.len() {
    if pos + 1 < chars.len() && chars[pos] == '<' && chars[pos + 1] == '%' {
        pos += 2;  // Skip <%

        // Determine tag type
        let tag_type = if chars[pos] == '=' {
            pos += 1;
            ErbTagType::Expression
        } else if chars[pos] == '#' {
            pos += 1;
            ErbTagType::Comment
        } else {
            ErbTagType::Code
        };

        // Find closing %>
        while pos + 1 < chars.len() && !(chars[pos] == '%' && chars[pos + 1] == '>') {
            pos += 1;
        }
        // ...
    } else {
        // Collect text until next ERB tag
    }
}
```

### Template Rendering

Expressions are substituted with variable values:

```rust
pub fn render_template(
    elements: &[ErbElement],
    vars: &std::collections::HashMap<&str, &str>,
) -> String {
    let mut result = String::new();

    for element in elements {
        match element {
            ErbElement::Text(t) => result.push_str(t),
            ErbElement::Expression(e) => {
                if let Some(value) = vars.get(e.as_str()) {
                    result.push_str(value);
                } else {
                    result.push_str(&format!("[{}]", e));
                }
            }
            ErbElement::Code(_) => { /* Not implemented */ }
            ErbElement::Comment(_) => { /* Not rendered */ }
        }
    }
    result
}
```

## Output Types

```rust
pub enum ErbElement {
    Text(String),        // Plain text
    Expression(String),  // <%= ... %>
    Code(String),        // <% ... %>
    Comment(String),     // <%# ... %>
}
```

Elements are tagged with their type for appropriate rendering.

## Design Decisions

### Why Not Execute Code?

Code execution (`<% ... %>`) requires an embedded Ruby interpreter. This parser focuses on template structure analysis, not execution.

### Why Simplified Content Matching?

The regex `[^%]*` is simplified and doesn't handle `%>` inside strings. Production parsers need more sophisticated matching for edge cases.
