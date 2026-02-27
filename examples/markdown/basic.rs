//! Markdown Subset Parser Example
//!
//! This example demonstrates parsing a subset of Markdown, a lightweight markup
//! language for creating formatted text. Markdown is widely used for documentation,
//! blogs, and README files.
//!
//! Supported features:
//! - Headers: # H1, ## H2, ### H3
//! - Paragraphs: plain text separated by blank lines
//! - Emphasis: *italic*, **bold**, ***bold italic***
//! - Inline code: `code`
//! - Links: [text](url)
//! - Images: ![alt](url)
//! - Lists: - item, 1. item
//! - Blockquotes: > quote
//! - Code blocks: ```language ... ```
//! - Horizontal rules: --- or ***
//!
//! Run with: cargo run --example markdown/basic --no-default-features

use parsanol::portable::{
    parser_dsl::{choice, dynamic, re, seq, str, GrammarBuilder, ParsletExt},
    AstArena, Grammar, PortableParser,
};
use serde::{Deserialize, Serialize};

/// Build Markdown grammar (subset)
fn build_markdown_grammar() -> Grammar {
    GrammarBuilder::new()
        // Headers: # H1, ## H2, ### H3, etc.
        .rule(
            "atx_header",
            seq(vec![
                dynamic(re("[ \t]*")),
                dynamic(re("#{1,6}")),     // 1-6 hashes
                dynamic(re("[ \t]+")),     // required space
                dynamic(re("[^\\n\\r]+")), // header text
            ]),
        )
        // Setext headers: H1 underline, H2 underline
        .rule(
            "setext_h1",
            seq(vec![
                dynamic(re("[^\\n\\r]+")), // text
                dynamic(re("[\r]?[\n]")),
                dynamic(re("={3,}")), // === underline
            ]),
        )
        .rule(
            "setext_h2",
            seq(vec![
                dynamic(re("[^\\n\\r]+")), // text
                dynamic(re("[\r]?[\n]")),
                dynamic(re("-{3,}")), // --- underline
            ]),
        )
        // Horizontal rule: --- or *** or ___
        .rule(
            "hr",
            seq(vec![
                dynamic(re("[ \t]*")),
                dynamic(re("(-[ \t]*){3,}|(\\*[ \t]*){3,}|(_[ \t]*){3,}")),
                dynamic(re("[ \t]*")),
            ]),
        )
        // Code block: ```
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
        // Inline code: `code`
        .rule(
            "inline_code",
            seq(vec![
                dynamic(str("`")),
                dynamic(re("[^`]+")),
                dynamic(str("`")),
            ]),
        )
        // Emphasis: *italic*, **bold**, ***both***
        .rule("italic", re("\\*[^*]+\\*"))
        .rule("bold", re("\\*\\*[^*]+\\*\\*"))
        .rule("bold_italic", re("\\*\\*\\*[^*]+\\*\\*\\*"))
        // Link: [text](url)
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
        // Image: ![alt](url)
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
        // Blockquote: > quote
        .rule(
            "blockquote",
            seq(vec![
                dynamic(re("[ \t]*")),
                dynamic(str(">")),
                dynamic(re("[ \t]*")),
                dynamic(re("[^\\n\\r]+")),
            ]),
        )
        // Unordered list: - item or * item
        .rule(
            "ul_item",
            seq(vec![
                dynamic(re("[ \t]*")),
                dynamic(str("-").or(str("*"))),
                dynamic(re("[ \t]+")),
                dynamic(re("[^\\n\\r]+")),
            ]),
        )
        // Ordered list: 1. item
        .rule(
            "ol_item",
            seq(vec![
                dynamic(re("[ \t]*")),
                dynamic(re("[0-9]+")),
                dynamic(str(".")),
                dynamic(re("[ \t]+")),
                dynamic(re("[^\\n\\r]+")),
            ]),
        )
        // Paragraph: text separated by blank lines
        .rule("paragraph", re("[^#\\-*_>`\\n\\r][^\\n\\r]+"))
        // Blank line
        .rule("blank", re("[ \t]*[\r]?[\n]"))
        // Inline elements (for parsing within paragraphs)
        .rule(
            "inline",
            choice(vec![
                dynamic(re("!\\[[^\\]]+\\]\\([^)]+\\)")), // image
                dynamic(re("\\[[^\\]]+\\]\\([^)]+\\)")),  // link
                dynamic(re("`[^`]+`")),                   // inline code
                dynamic(re("\\*\\*\\*[^*]+\\*\\*\\*")),   // bold italic
                dynamic(re("\\*\\*[^*]+\\*\\*")),         // bold
                dynamic(re("\\*[^*]+\\*")),               // italic
                dynamic(re("[^\\[*_`\n\\r]+")),           // plain text
            ]),
        )
        // Block elements
        .rule(
            "block",
            choice(vec![
                dynamic(re("```[a-zA-Z0-9+-]*[\r]?[\n].*?[\r]?[\n]```")), // code block
                dynamic(re("#{1,6}[ \t]+[^\\n\\r]+")),                    // atx header
                dynamic(re(">[ \t]*[^\\n\\r]+")),                         // blockquote
                dynamic(re("(-[ \t]*){3,}|(\\*[ \t]*){3,}")),             // hr
                dynamic(re("[0-9]+\\.[ \t]+[^\\n\\r]+")),                 // ordered list
                dynamic(re("[-*][ \t]+[^\\n\\r]+")),                      // unordered list
                dynamic(re("[^#\\-*_>`\\n\\r][^\\n\\r]*")),               // paragraph
                dynamic(re("[ \t]*")),                                    // blank
            ]),
        )
        // Document: sequence of blocks
        .rule(
            "document",
            seq(vec![
                dynamic(
                    seq(vec![
                        dynamic(choice(vec![
                            dynamic(re("```[a-zA-Z0-9+-]*[\r]?[\n].*?[\r]?[\n]```")),
                            dynamic(re("#{1,6}[ \t]+[^\\n\\r]+")),
                            dynamic(re(">[ \t]*[^\\n\\r]+")),
                            dynamic(re("(-[ \t]*){3,}|(\\*[ \t]*){3,}")),
                            dynamic(re("[0-9]+\\.[ \t]+[^\\n\\r]+")),
                            dynamic(re("[-*][ \t]+[^\\n\\r]+")),
                            dynamic(re("[^#\\-*_>`\\n\\r][^\\n\\r]*")),
                            dynamic(re("[ \t]*")),
                        ])),
                        dynamic(re("[\r]?[\n]")),
                    ])
                    .many(),
                ),
                dynamic(
                    choice(vec![
                        dynamic(re("#{1,6}[ \t]+[^\\n\\r]+")),
                        dynamic(re(">[ \t]*[^\\n\\r]+")),
                        dynamic(re("[-*][ \t]+[^\\n\\r]+")),
                        dynamic(re("[^#\\-*_>`\\n\\r][^\\n\\r]*")),
                        dynamic(re("[ \t]*")),
                    ])
                    .optional(),
                ),
            ]),
        )
        .build()
}

/// Markdown block types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MdBlock {
    #[serde(rename = "heading")]
    Heading { level: u8, text: String },

    #[serde(rename = "paragraph")]
    Paragraph { text: String },

    #[serde(rename = "code_block")]
    CodeBlock {
        language: Option<String>,
        code: String,
    },

    #[serde(rename = "blockquote")]
    Blockquote { text: String },

    #[serde(rename = "list")]
    List { ordered: bool, items: Vec<String> },

    #[serde(rename = "hr")]
    HorizontalRule,

    #[serde(rename = "blank")]
    Blank,
}

/// Parsed Markdown document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkdownDocument {
    pub blocks: Vec<MdBlock>,
}

/// Parse Markdown string
pub fn parse_markdown(input: &str) -> Result<String, String> {
    let grammar = build_markdown_grammar();
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    let _ast = parser
        .parse()
        .map_err(|e| format!("Parse error: {:?}", e))?;

    // Manual parsing for structured output
    let doc = parse_markdown_document(input)?;
    serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())
}

/// Parse a Markdown document
fn parse_markdown_document(input: &str) -> Result<MarkdownDocument, String> {
    let mut blocks = Vec::new();
    let mut in_code_block = false;
    let mut code_content = String::new();
    let mut code_lang: Option<String> = None;
    let mut current_list: Option<(bool, Vec<String>)> = None; // (ordered, items)

    for line in input.lines() {
        // Handle code blocks
        if line.trim().starts_with("```") {
            if in_code_block {
                // End code block
                blocks.push(MdBlock::CodeBlock {
                    language: code_lang.take(),
                    code: code_content.trim_end().to_string(),
                });
                code_content.clear();
                in_code_block = false;
            } else {
                // Start code block
                in_code_block = true;
                code_lang = if line.trim().len() > 3 {
                    Some(line.trim()[3..].to_string())
                } else {
                    None
                };
            }
            continue;
        }

        if in_code_block {
            code_content.push_str(line);
            code_content.push('\n');
            continue;
        }

        // Finish any pending list
        let trimmed = line.trim();

        // ATX headers: # H1
        if trimmed.starts_with('#') {
            flush_list(&mut current_list, &mut blocks);
            let hash_count = trimmed.chars().take_while(|&c| c == '#').count() as u8;
            let text = trimmed[hash_count as usize..].trim().to_string();
            blocks.push(MdBlock::Heading {
                level: hash_count,
                text,
            });
            continue;
        }

        // Horizontal rule
        if is_horizontal_rule(trimmed) {
            flush_list(&mut current_list, &mut blocks);
            blocks.push(MdBlock::HorizontalRule);
            continue;
        }

        // Blockquote
        if trimmed.starts_with('>') {
            flush_list(&mut current_list, &mut blocks);
            let text = trimmed.strip_prefix('>').unwrap_or("").trim().to_string();
            blocks.push(MdBlock::Blockquote { text });
            continue;
        }

        // Unordered list
        if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            let item = trimmed[2..].trim().to_string();
            if let Some((_, ref mut items)) = current_list {
                items.push(item);
            } else {
                current_list = Some((false, vec![item]));
            }
            continue;
        }

        // Ordered list
        if let Some(pos) = trimmed.find(". ") {
            if pos > 0 && trimmed[..pos].chars().all(|c| c.is_ascii_digit()) {
                let item = trimmed[pos + 2..].trim().to_string();
                if let Some((_, ref mut items)) = current_list {
                    items.push(item);
                } else {
                    current_list = Some((true, vec![item]));
                }
                continue;
            }
        }

        // Blank line
        if trimmed.is_empty() {
            flush_list(&mut current_list, &mut blocks);
            continue;
        }

        // Paragraph
        flush_list(&mut current_list, &mut blocks);
        blocks.push(MdBlock::Paragraph {
            text: trimmed.to_string(),
        });
    }

    // Handle any remaining content
    if in_code_block {
        blocks.push(MdBlock::CodeBlock {
            language: code_lang,
            code: code_content.trim_end().to_string(),
        });
    }
    flush_list(&mut current_list, &mut blocks);

    Ok(MarkdownDocument { blocks })
}

fn flush_list(list: &mut Option<(bool, Vec<String>)>, blocks: &mut Vec<MdBlock>) {
    if let Some((ordered, items)) = list.take() {
        if !items.is_empty() {
            blocks.push(MdBlock::List { ordered, items });
        }
    }
}

fn is_horizontal_rule(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() {
        return false;
    }
    let first_char = s.chars().next().unwrap();
    if first_char != '-' && first_char != '*' && first_char != '_' {
        return false;
    }
    let count = s.chars().filter(|&c| c == first_char).count();
    count >= 3 && s.chars().all(|c| c == first_char || c == ' ' || c == '\t')
}

fn main() {
    println!("Markdown Subset Parser Example");
    println!("==============================\n");

    let examples = [
        (
            r#"# Main Title

This is a paragraph with **bold** and *italic* text.

## Section 1

- Item one
- Item two
- Item three

### Subsection

1. First
2. Second
3. Third

> This is a blockquote

---

```rust
fn main() {
    println!("Hello, world!");
}
```

[Link text](https://example.com)
"#,
            "Full document",
        ),
        (
            r#"# Quick Start

1. Install dependencies
2. Run the server
3. Open browser

**Done!**
"#,
            "Simple document",
        ),
    ];

    for (input, description) in examples {
        println!("Input: ({})", description);
        println!("{}\n", input);
        match parse_markdown(input) {
            Ok(json) => println!("Output:\n{}\n", json),
            Err(e) => println!("Error: {}\n", e),
        }
        println!("{}", "-".repeat(60));
        println!();
    }
}
