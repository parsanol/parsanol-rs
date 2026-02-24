//! ERB Template Parser Example
//!
//! This example demonstrates parsing ERB-style templates.
//! Shows handling of mixed text and embedded code sections.
//! Based on the Parslet erb.rb example.
//!
//! Run with: cargo run --example erb_parser --no-default-features

#![allow(clippy::print_literal)]

use parsanol::portable::{
    parser_dsl::{choice, dynamic, re, seq, str, GrammarBuilder},
    AstArena, Grammar, PortableParser,
};

/// Build an ERB grammar
fn build_erb_grammar() -> Grammar {
    GrammarBuilder::new()
        // ERB expression: <%= ... %>
        .rule(
            "expression",
            seq(vec![
                dynamic(str("<%=")),
                dynamic(re("[^%]*")), // Content (simplified)
                dynamic(str("%>")),
            ]),
        )
        // ERB comment: <%# ... %>
        .rule(
            "comment",
            seq(vec![
                dynamic(str("<%#")),
                dynamic(re("[^%]*")),
                dynamic(str("%>")),
            ]),
        )
        // ERB code: <% ... %>
        .rule(
            "code",
            seq(vec![
                dynamic(str("<%")),
                dynamic(re("[^%]*")),
                dynamic(str("%>")),
            ]),
        )
        // Any ERB tag
        .rule(
            "erb_tag",
            choice(vec![
                dynamic(seq(vec![
                    dynamic(str("<%=")),
                    dynamic(re("[^%]*")),
                    dynamic(str("%>")),
                ])),
                dynamic(seq(vec![
                    dynamic(str("<%#")),
                    dynamic(re("[^%]*")),
                    dynamic(str("%>")),
                ])),
                dynamic(seq(vec![
                    dynamic(str("<%")),
                    dynamic(re("[^%]*")),
                    dynamic(str("%>")),
                ])),
            ]),
        )
        // Text: anything until ERB tag
        .rule("text", re("[^<]+"))
        // Template: text and tags mixed
        .rule("template", re(".*"))
        .build()
}

/// ERB element types
#[derive(Debug, Clone)]
pub enum ErbElement {
    /// Plain text
    Text(String),
    /// Expression: <%= ... %>
    Expression(String),
    /// Code: <% ... %>
    Code(String),
    /// Comment: <%# ... %>
    Comment(String),
}

impl std::fmt::Display for ErbElement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErbElement::Text(t) => write!(f, "TEXT: {}", t),
            ErbElement::Expression(e) => write!(f, "EXPR: {}", e),
            ErbElement::Code(c) => write!(f, "CODE: {}", c),
            ErbElement::Comment(c) => write!(f, "COMMENT: {}", c),
        }
    }
}

/// Parse an ERB template
pub fn parse_erb(input: &str) -> Result<Vec<ErbElement>, String> {
    let grammar = build_erb_grammar();
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    parser
        .parse()
        .map_err(|e| format!("Parse error: {:?}", e))?;

    // Manual parsing for element extraction
    let mut elements = Vec::new();
    let mut pos = 0;
    let chars: Vec<char> = input.chars().collect();

    while pos < chars.len() {
        // Check for ERB tag
        if pos + 1 < chars.len() && chars[pos] == '<' && chars[pos + 1] == '%' {
            // Determine tag type
            pos += 2; // Skip <%

            let tag_type = if pos < chars.len() && chars[pos] == '=' {
                pos += 1;
                ErbTagType::Expression
            } else if pos < chars.len() && chars[pos] == '#' {
                pos += 1;
                ErbTagType::Comment
            } else {
                ErbTagType::Code
            };

            // Find closing %>
            let content_start = pos;
            while pos + 1 < chars.len() && !(chars[pos] == '%' && chars[pos + 1] == '>') {
                pos += 1;
            }

            let content: String = chars[content_start..pos].iter().collect();
            pos += 2; // Skip %>

            match tag_type {
                ErbTagType::Expression => {
                    elements.push(ErbElement::Expression(content.trim().to_string()))
                }
                ErbTagType::Code => elements.push(ErbElement::Code(content.trim().to_string())),
                ErbTagType::Comment => {
                    elements.push(ErbElement::Comment(content.trim().to_string()))
                }
            }
        } else {
            // Collect text until next ERB tag
            let text_start = pos;
            while pos < chars.len()
                && !(pos + 1 < chars.len() && chars[pos] == '<' && chars[pos + 1] == '%')
            {
                pos += 1;
            }
            let text: String = chars[text_start..pos].iter().collect();
            if !text.is_empty() {
                elements.push(ErbElement::Text(text));
            }
        }
    }

    Ok(elements)
}

#[derive(Debug, Clone, Copy)]
enum ErbTagType {
    Expression,
    Code,
    Comment,
}

/// Render template with variable bindings (simplified)
pub fn render_template(
    elements: &[ErbElement],
    vars: &std::collections::HashMap<&str, &str>,
) -> String {
    let mut result = String::new();

    for element in elements {
        match element {
            ErbElement::Text(t) => result.push_str(t),
            ErbElement::Expression(e) => {
                // Simple variable substitution
                if let Some(value) = vars.get(e.as_str()) {
                    result.push_str(value);
                } else {
                    result.push_str(&format!("[{}]", e));
                }
            }
            ErbElement::Code(_) => {
                // Code execution not implemented (would need embedded interpreter)
            }
            ErbElement::Comment(_) => {
                // Comments are not rendered
            }
        }
    }

    result
}

fn main() {
    println!("ERB Template Parser Example");
    println!("===========================\n");

    let templates = [
        "The value of x is <%= x %>.",
        "<% 1 + 2 %>",
        "<%# commented %>",
        "Hello <%= name %>, you have <%= count %> messages.",
        "Text before <% code %> text after.",
    ];

    println!("=== Parsing Examples ===\n");

    for template in templates {
        println!("Template: {:?}", template);
        match parse_erb(template) {
            Ok(elements) => {
                println!("Elements:");
                for element in &elements {
                    println!("  {}", element);
                }
            }
            Err(e) => println!("Error: {}", e),
        }
        println!();
    }

    // Demonstrate rendering
    println!("=== Rendering Example ===\n");

    let template = "Hello <%= name %>, welcome to <%= place %>!";
    println!("Template: {:?}", template);

    use std::collections::HashMap;
    let mut vars = HashMap::new();
    vars.insert("name", "World");
    vars.insert("place", "Rust");

    match parse_erb(template) {
        Ok(elements) => {
            println!("Parsed:");
            for element in &elements {
                println!("  {}", element);
            }
            println!("\nRendered: {:?}", render_template(&elements, &vars));
        }
        Err(e) => println!("Error: {}", e),
    }

    // Complex example
    println!("\n=== Complex Template ===\n");

    let complex = r#"<!DOCTYPE html>
<html>
<head><title><%= title %></title></head>
<body>
  <%# This is a comment %>
  <h1><%= heading %></h1>
  <p>Welcome, <%= user %>!</p>
</body>
</html>"#;

    println!("Template:");
    println!("{}", complex);
    println!();

    let mut vars2 = HashMap::new();
    vars2.insert("title", "My Page");
    vars2.insert("heading", "Welcome");
    vars2.insert("user", "Alice");

    match parse_erb(complex) {
        Ok(elements) => {
            println!("Found {} elements:", elements.len());
            for (i, element) in elements.iter().enumerate() {
                println!("  {}: {}", i + 1, element);
            }
        }
        Err(e) => println!("Error: {}", e),
    }
}
