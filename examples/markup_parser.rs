//! Simple Markup Parser Example
//!
//! This example demonstrates parsing a simple markup language (like Markdown subset).
//! Shows handling of inline formatting, headers, and lists.
//!
//! Run with: cargo run --example markup_parser --no-default-features

#![allow(clippy::get_first)]

use parsanol::portable::{
    parser_dsl::{choice, dynamic, re, seq, str, GrammarBuilder},
    AstArena, Grammar, PortableParser,
};

/// Build a markup grammar
fn build_markup_grammar() -> Grammar {
    GrammarBuilder::new()
        // Inline: bold **text**, italic *text*, code `text`
        .rule(
            "bold",
            seq(vec![
                dynamic(str("**")),
                dynamic(re("[^*]+")),
                dynamic(str("**")),
            ]),
        )
        .rule(
            "italic",
            seq(vec![
                dynamic(str("*")),
                dynamic(re("[^*]+")),
                dynamic(str("*")),
            ]),
        )
        .rule(
            "code",
            seq(vec![
                dynamic(str("`")),
                dynamic(re("[^`]+")),
                dynamic(str("`")),
            ]),
        )
        // Header: # to ######
        .rule("h1", seq(vec![dynamic(str("# ")), dynamic(re("[^\\n]+"))]))
        .rule("h2", seq(vec![dynamic(str("## ")), dynamic(re("[^\\n]+"))]))
        .rule(
            "h3",
            seq(vec![dynamic(str("### ")), dynamic(re("[^\\n]+"))]),
        )
        // List item: - or * followed by text
        .rule(
            "list_item",
            seq(vec![
                dynamic(re("[ \t]*[-*][ \t]+")),
                dynamic(re("[^\\n]+")),
            ]),
        )
        // Paragraph: text until blank line or EOF
        .rule("paragraph", re("[^\\n]+(\\n[^\\n]+)*"))
        // Document: any of the above
        .rule(
            "block",
            choice(vec![
                dynamic(seq(vec![dynamic(str("# ")), dynamic(re("[^\\n]+"))])),
                dynamic(seq(vec![dynamic(str("## ")), dynamic(re("[^\\n]+"))])),
                dynamic(seq(vec![dynamic(str("### ")), dynamic(re("[^\\n]+"))])),
                dynamic(seq(vec![
                    dynamic(re("[ \t]*[-*][ \t]+")),
                    dynamic(re("[^\\n]+")),
                ])),
                dynamic(re("[^\\n]+")),
            ]),
        )
        .build()
}

/// Markup element types
#[derive(Debug, Clone)]
pub enum MarkupElement {
    /// Header with level and text
    Header { level: u8, text: String },
    /// Paragraph with raw text
    Paragraph(String),
    /// List item with text
    ListItem(String),
    /// Bold text
    Bold(String),
    /// Italic text
    Italic(String),
    /// Code inline
    Code(String),
}

impl std::fmt::Display for MarkupElement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MarkupElement::Header { level, text } => {
                write!(f, "H{}: {}", level, text)
            }
            MarkupElement::Paragraph(text) => write!(f, "P: {}", text),
            MarkupElement::ListItem(text) => write!(f, "LI: {}", text),
            MarkupElement::Bold(text) => write!(f, "**{}**", text),
            MarkupElement::Italic(text) => write!(f, "*{}*", text),
            MarkupElement::Code(text) => write!(f, "`{}`", text),
        }
    }
}

/// Parsed markup document
#[derive(Debug, Clone, Default)]
pub struct MarkupDocument {
    pub elements: Vec<MarkupElement>,
}

/// Parse a markup string into structured elements
pub fn parse_markup(input: &str) -> Result<MarkupDocument, String> {
    let grammar = build_markup_grammar();
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    let _ast = parser
        .parse()
        .map_err(|e| format!("Parse error: {:?}", e))?;

    // Manual parsing for clarity
    let mut doc = MarkupDocument::default();

    for line in input.lines() {
        let trimmed = line.trim();

        // Skip empty lines
        if trimmed.is_empty() {
            continue;
        }

        // Headers
        if let Some(stripped) = trimmed.strip_prefix("### ") {
            doc.elements.push(MarkupElement::Header {
                level: 3,
                text: stripped.to_string(),
            });
        } else if let Some(stripped) = trimmed.strip_prefix("## ") {
            doc.elements.push(MarkupElement::Header {
                level: 2,
                text: stripped.to_string(),
            });
        } else if let Some(stripped) = trimmed.strip_prefix("# ") {
            doc.elements.push(MarkupElement::Header {
                level: 1,
                text: stripped.to_string(),
            });
        }
        // List items
        else if let Some(stripped) = trimmed.strip_prefix("- ") {
            doc.elements
                .push(MarkupElement::ListItem(stripped.to_string()));
        } else if let Some(stripped) = trimmed.strip_prefix("* ") {
            doc.elements
                .push(MarkupElement::ListItem(stripped.to_string()));
        }
        // Regular paragraph
        else {
            doc.elements
                .push(MarkupElement::Paragraph(trimmed.to_string()));
        }
    }

    Ok(doc)
}

/// Parse inline formatting (bold, italic, code)
pub fn parse_inline(text: &str) -> Vec<MarkupElement> {
    let mut elements = Vec::new();
    let mut chars = text.chars().peekable();
    let mut current = String::new();

    while let Some(c) = chars.next() {
        match c {
            '*' if chars.peek() == Some(&'*') => {
                chars.next(); // consume second *
                if !current.is_empty() {
                    elements.push(MarkupElement::Paragraph(current.clone()));
                    current.clear();
                }
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
            '*' => {
                if !current.is_empty() {
                    elements.push(MarkupElement::Paragraph(current.clone()));
                    current.clear();
                }
                // Read until closing *
                let mut italic_text = String::new();
                while let Some(&next) = chars.peek() {
                    if next == '*' {
                        chars.next();
                        break;
                    }
                    italic_text.push(chars.next().unwrap());
                }
                elements.push(MarkupElement::Italic(italic_text));
            }
            '`' => {
                if !current.is_empty() {
                    elements.push(MarkupElement::Paragraph(current.clone()));
                    current.clear();
                }
                // Read until closing `
                let mut code_text = String::new();
                while let Some(&next) = chars.peek() {
                    if next == '`' {
                        chars.next();
                        break;
                    }
                    code_text.push(chars.next().unwrap());
                }
                elements.push(MarkupElement::Code(code_text));
            }
            _ => {
                current.push(c);
            }
        }
    }

    if !current.is_empty() {
        elements.push(MarkupElement::Paragraph(current));
    }

    elements
}

fn main() {
    println!("Simple Markup Parser Example");
    println!("============================\n");

    let markup = r#"
# Main Title

This is a paragraph with **bold** and *italic* text.

## Features

- Fast parsing
- Memory efficient
- Easy to use API

### Code Example

Here is some `inline code` for demonstration.

- Supports **nested** formatting
- Can parse *multiple* styles
"#;

    println!("Input Markup:");
    println!("-------------");
    println!("{}\n", markup);

    match parse_markup(markup) {
        Ok(doc) => {
            println!("Parsed Elements:");
            println!("----------------");
            for (i, element) in doc.elements.iter().enumerate() {
                println!("{}. {}", i + 1, element);
            }

            println!("\nInline Formatting Demo:");
            println!("-----------------------");
            let inline_text = "This has **bold**, *italic*, and `code` formatting";
            let inline_elements = parse_inline(inline_text);
            for element in inline_elements {
                println!("  {}", element);
            }
        }
        Err(e) => println!("Error: {}", e),
    }
}
