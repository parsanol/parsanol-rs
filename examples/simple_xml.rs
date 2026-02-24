//! Simple XML Parser Example
//!
//! This example demonstrates parsing simple XML-like structures.
//! Shows handling of nested tags, text content, and tag matching.
//! Based on the Parslet simple_xml.rb example.
//!
//! Note: This is a simplified XML parser for educational purposes.
//! It does not handle all XML complexities (namespaces, attributes, CDATA, etc.)
//!
//! Run with: cargo run --example simple_xml --no-default-features

#![allow(clippy::print_literal)]
#![allow(clippy::get_first)]

use parsanol::portable::{
    parser_dsl::{choice, dynamic, re, seq, str, GrammarBuilder},
    AstArena, Grammar, PortableParser,
};

/// Build a simple XML grammar
fn build_xml_grammar() -> Grammar {
    GrammarBuilder::new()
        // Text content: anything except angle brackets
        .rule("text", re("[^<>]*"))
        // Tag name: alphabetic characters
        .rule("tag_name", re("[a-zA-Z][a-zA-Z0-9_-]*"))
        // Opening tag: <name>
        .rule(
            "open_tag",
            seq(vec![
                dynamic(str("<")),
                dynamic(re("[a-zA-Z][a-zA-Z0-9_-]*")),
                dynamic(str(">")),
            ]),
        )
        // Closing tag: </name>
        .rule(
            "close_tag",
            seq(vec![
                dynamic(str("</")),
                dynamic(re("[a-zA-Z][a-zA-Z0-9_-]*")),
                dynamic(str(">")),
            ]),
        )
        // Self-closing tag: <name />
        .rule(
            "self_closing_tag",
            seq(vec![
                dynamic(str("<")),
                dynamic(re("[a-zA-Z][a-zA-Z0-9_-]*")),
                dynamic(re("[ \\t]*/[ \\t]*")),
                dynamic(str(">")),
            ]),
        )
        // Element: open tag + content + close tag OR self-closing
        .rule(
            "element",
            choice(vec![
                dynamic(seq(vec![
                    dynamic(str("<")),
                    dynamic(re("[a-zA-Z][a-zA-Z0-9_-]*")),
                    dynamic(str(">")),
                    dynamic(re("[^<>]*")), // content
                    dynamic(str("</")),
                    dynamic(re("[a-zA-Z][a-zA-Z0-9_-]*")),
                    dynamic(str(">")),
                ])),
                dynamic(seq(vec![
                    dynamic(str("<")),
                    dynamic(re("[a-zA-Z][a-zA-Z0-9_-]*")),
                    dynamic(re("[ \\t]*/[ \\t]*")),
                    dynamic(str(">")),
                ])),
            ]),
        )
        // Document: sequence of elements and text
        .rule("document", re("[^<>]*(?:<[^>]+>[^<>]*)*"))
        .build()
}

/// XML node types
#[derive(Debug, Clone)]
pub enum XmlNode {
    /// Element with tag name, children, and text content
    Element {
        name: String,
        children: Vec<XmlNode>,
        text: Option<String>,
    },
    /// Text content
    Text(String),
    /// Self-closing element
    EmptyElement { name: String },
}

impl std::fmt::Display for XmlNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            XmlNode::Element {
                name,
                children,
                text,
            } => {
                write!(f, "<{}>", name)?;
                if let Some(t) = text {
                    write!(f, "{}", t)?;
                }
                for child in children {
                    write!(f, "{}", child)?;
                }
                write!(f, "</{}>", name)
            }
            XmlNode::Text(t) => write!(f, "{}", t),
            XmlNode::EmptyElement { name } => write!(f, "<{} />", name),
        }
    }
}

/// Parse result
#[derive(Debug, Clone)]
pub struct XmlParseResult {
    pub valid: bool,
    pub message: String,
}

/// Validate XML by checking tag matching
pub fn validate_xml(input: &str) -> XmlParseResult {
    let grammar = build_xml_grammar();
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    if parser.parse().is_err() {
        return XmlParseResult {
            valid: false,
            message: "Failed to parse XML structure".to_string(),
        };
    }

    // Manual tag matching validation
    let mut tag_stack: Vec<String> = Vec::new();
    let mut pos = 0;
    let chars: Vec<char> = input.chars().collect();

    while pos < chars.len() {
        if chars[pos] == '<' {
            // Find closing >
            let end = chars[pos..].iter().position(|&c| c == '>');
            if let Some(end) = end {
                let tag_content: String = chars[pos + 1..pos + end].iter().collect();
                let tag_content = tag_content.trim();

                if let Some(stripped) = tag_content.strip_prefix('/') {
                    // Closing tag
                    let tag_name = stripped.trim();
                    if let Some(expected) = tag_stack.pop() {
                        if expected != tag_name {
                            return XmlParseResult {
                                valid: false,
                                message: format!(
                                    "Tag mismatch: expected </{}>, found </{}>",
                                    expected, tag_name
                                ),
                            };
                        }
                    } else {
                        return XmlParseResult {
                            valid: false,
                            message: format!("Unexpected closing tag </{}>", tag_name),
                        };
                    }
                } else if tag_content.ends_with('/') {
                    // Self-closing tag - skip
                } else {
                    // Opening tag
                    let tag_name = tag_content.split_whitespace().next().unwrap_or(tag_content);
                    tag_stack.push(tag_name.to_string());
                }
                pos += end + 1;
            } else {
                pos += 1;
            }
        } else {
            pos += 1;
        }
    }

    if !tag_stack.is_empty() {
        return XmlParseResult {
            valid: false,
            message: format!("Unclosed tags: {}", tag_stack.join(", ")),
        };
    }

    XmlParseResult {
        valid: true,
        message: "XML is valid".to_string(),
    }
}

/// Parse XML into a simple structure
pub fn parse_xml(input: &str) -> Result<Vec<XmlNode>, String> {
    let grammar = build_xml_grammar();
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    parser
        .parse()
        .map_err(|e| format!("Parse error: {:?}", e))?;

    // Manual parsing into XmlNode structure
    let mut nodes = Vec::new();
    let mut pos = 0;
    let chars: Vec<char> = input.chars().collect();

    while pos < chars.len() {
        // Collect text before tag
        let text_start = pos;
        while pos < chars.len() && chars[pos] != '<' {
            pos += 1;
        }
        if pos > text_start {
            let text: String = chars[text_start..pos].iter().collect();
            let text = text.trim();
            if !text.is_empty() {
                nodes.push(XmlNode::Text(text.to_string()));
            }
        }

        // Parse tag
        if pos < chars.len() && chars[pos] == '<' {
            let tag_start = pos;
            // Find closing >
            while pos < chars.len() && chars[pos] != '>' {
                pos += 1;
            }
            if pos < chars.len() {
                pos += 1; // Include >
                let tag: String = chars[tag_start..pos].iter().collect();
                nodes.push(XmlNode::Text(tag)); // Simplified - just store as text
            }
        }
    }

    Ok(nodes)
}

fn main() {
    println!("Simple XML Parser Example");
    println!("=========================\n");

    let examples = [
        // Valid XML
        ("<a>text</a>", true),
        ("<a><b>content</b></a>", true),
        ("<a><b>nested</b><c>elements</c></a>", true),
        ("<root>some text in the tags</root>", true),
        ("<br />", true),
        ("<div><p>Hello</p><br /><p>World</p></div>", true),
        // Invalid XML
        ("<a><b>mismatch</a></b>", false),
        ("<a>unclosed", false),
        ("<a></b>", false),
    ];

    println!("{:<50} | {:<8} | {}", "XML", "Valid?", "Message");
    println!("{}", "-".repeat(80));

    for (xml, expected_valid) in examples {
        let result = validate_xml(xml);
        let status = if result.valid == expected_valid {
            "✓"
        } else {
            "✗"
        };
        println!(
            "{:<50} | {:<8} | {} {}",
            xml, result.valid, status, result.message
        );
    }

    // Detailed example
    println!("\nDetailed Parsing:");
    println!("-----------------");

    let xml = "<book><title>XML Parsing</title><author>John Doe</author></book>";
    println!("Input: {}", xml);

    let result = validate_xml(xml);
    println!("Valid: {}", result.valid);

    if let Ok(nodes) = parse_xml(xml) {
        println!("Nodes: {:?}", nodes.len());
        for node in &nodes {
            if let XmlNode::Text(t) = node {
                println!("  - {}", t);
            }
        }
    }

    // Tag nesting visualization
    println!("\nTag Nesting Example:");
    println!("--------------------");
    let nested = "<root><level1><level2><level3>deep</level3></level2></level1></root>";
    println!("Input: {}", nested);
    let result = validate_xml(nested);
    println!("Result: {} - {}", result.valid, result.message);
}
