# Markdown Parser - Rust Implementation

## How to Run

```bash
cargo run --example markdown/basic --no-default-features
```

## Code Walkthrough

### ATX Header Parsing

Headers use 1-6 `#` characters:

```rust
.rule(
    "atx_header",
    seq(vec![
        dynamic(re("[ \t]*")),
        dynamic(re("#{1,6}")),     // 1-6 hashes
        dynamic(re("[ \t]+")),     // required space
        dynamic(re("[^\\n\\r]+")), // header text
    ]),
)
```

The space after `#` is required by CommonMark spec.

### Code Block Parsing

Fenced code blocks use triple backticks:

```rust
.rule(
    "code_block",
    seq(vec![
        dynamic(str("```")),
        dynamic(re("[a-zA-Z0-9+-]*")), // optional language
        dynamic(re("[\r]?[\n]")),
        dynamic(re(".*?")), // code content (non-greedy)
        dynamic(re("[\r]?[\n]")),
        dynamic(str("```")),
    ]),
)
```

Language specification is optional.

### Emphasis Parsing

Inline emphasis uses asterisks:

```rust
.rule("italic", re("\\*[^*]+\\*"))
.rule("bold", re("\\*\\*[^*]+\\*\\*"))
.rule("bold_italic", re("\\*\\*\\*[^*]+\\*\\*\\*"))
```

Bold-italic (`***`) must be checked before bold (`**`) and italic (`*`).

### Link and Image Parsing

Links and images use bracket syntax:

```rust
.rule(
    "link",
    seq(vec![
        dynamic(str("[")),
        dynamic(re("[^\\]]+")), // text
        dynamic(str("]")),
        dynamic(str("(")),
        dynamic(re("[^)]+")), // url
        dynamic(str(")")),
    ]),
)
.rule(
    "image",
    seq(vec![
        dynamic(str("![")),
        dynamic(re("[^\\]]+")), // alt text
        dynamic(str("]")),
        dynamic(str("(")),
        dynamic(re("[^)]+")), // url
        dynamic(str(")")),
    ]),
)
```

Images are distinguished by the leading `!`.

### List Parsing

Lists are accumulated until a non-list line:

```rust
// Unordered list
if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
    let item = trimmed[2..].trim().to_string();
    if let Some((_, ref mut items)) = current_list {
        items.push(item);
    } else {
        current_list = Some((false, vec![item]));
    }
}

// Ordered list
if let Some(pos) = trimmed.find(". ") {
    if pos > 0 && trimmed[..pos].chars().all(|c| c.is_ascii_digit()) {
        let item = trimmed[pos + 2..].trim().to_string();
        // ...
    }
}
```

The `flush_list` helper emits completed lists.

## Output Types

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MdBlock {
    #[serde(rename = "heading")]
    Heading { level: u8, text: String },

    #[serde(rename = "paragraph")]
    Paragraph { text: String },

    #[serde(rename = "code_block")]
    CodeBlock { language: Option<String>, code: String },

    #[serde(rename = "blockquote")]
    Blockquote { text: String },

    #[serde(rename = "list")]
    List { ordered: bool, items: Vec<String> },

    #[serde(rename = "hr")]
    HorizontalRule,

    #[serde(rename = "blank")]
    Blank,
}

pub struct MarkdownDocument {
    pub blocks: Vec<MdBlock>,
}
```

The serde representation serializes cleanly to JSON.

## Design Decisions

### Why Not Full CommonMark?

Full CommonMark has many edge cases (setext headers, link references, HTML blocks). This subset covers the 95% use case.

### Why Line-Based Parsing?

Markdown's block structure is fundamentally line-based. Processing line-by-line is simpler than character-by-character for blocks.
