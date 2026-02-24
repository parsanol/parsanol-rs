//! Comment Parser Example
//!
//! This example demonstrates parsing code with line and block comments.
//! Shows handling of nested structures with comment skipping.
//! Based on the Parslet comments.rb example.
//!
//! Run with: cargo run --example comments_parser --no-default-features

#![allow(clippy::print_literal)]

use parsanol::portable::{
    parser_dsl::{choice, dynamic, re, seq, str, GrammarBuilder},
    AstArena, Grammar, PortableParser,
};

/// Build a grammar that handles comments
fn build_comment_grammar() -> Grammar {
    GrammarBuilder::new()
        // Line comment: // until end of line
        .rule("line_comment", seq(vec![
            dynamic(str("//")),
            dynamic(re("[^\n\r]*")),
        ]))
        // Block comment: /* ... */
        .rule("block_comment", seq(vec![
            dynamic(str("/*")),
            dynamic(re("(?:[^*]|\\*(?![/]))*")),  // Not */ or * followed by /
            dynamic(str("*/")),
        ]))
        // Whitespace
        .rule("space", re("[ \\t]+"))
        // Identifier
        .rule("identifier", re("[a-zA-Z_][a-zA-Z0-9_]*"))
        // Statement: identifier or comment
        .rule("statement", choice(vec![
            dynamic(re("[a-zA-Z_][a-zA-Z0-9_]*")),
            dynamic(seq(vec![
                dynamic(str("//")),
                dynamic(re("[^\n\r]*")),
            ])),
            dynamic(seq(vec![
                dynamic(str("/*")),
                dynamic(re("(?:[^*]|\\*(?![/]))*")),
                dynamic(str("*/")),
            ])),
        ]))
        // Line: optional statement
        .rule("line", re("[ \\t]*(?:(?:[a-zA-Z_][a-zA-Z0-9_]*)|(?://[^\n\r]*)|(?:/\\*(?:[^*]|\\*(?![/]))*\\*/))?[ \\t]*"))
        // Document: multiple lines
        .rule("document", re(".*"))
        .build()
}

/// Comment types
#[derive(Debug, Clone)]
pub enum Comment {
    Line(String),
    Block(String),
}

/// Parsed code element
#[derive(Debug, Clone)]
pub enum CodeElement {
    Identifier(String),
    Comment(Comment),
    Newline,
}

/// Parse code with comments
pub fn parse_with_comments(input: &str) -> Result<Vec<CodeElement>, String> {
    let grammar = build_comment_grammar();
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
        // Skip whitespace (except newlines)
        while pos < chars.len() && (chars[pos] == ' ' || chars[pos] == '\t') {
            pos += 1;
        }

        if pos >= chars.len() {
            break;
        }

        // Newline
        if chars[pos] == '\n' || chars[pos] == '\r' {
            elements.push(CodeElement::Newline);
            pos += 1;
            if chars[pos - 1] == '\r' && pos < chars.len() && chars[pos] == '\n' {
                pos += 1;
            }
            continue;
        }

        // Line comment
        if pos + 1 < chars.len() && chars[pos] == '/' && chars[pos + 1] == '/' {
            pos += 2;
            let start = pos;
            while pos < chars.len() && chars[pos] != '\n' && chars[pos] != '\r' {
                pos += 1;
            }
            let comment: String = chars[start..pos].iter().collect();
            elements.push(CodeElement::Comment(Comment::Line(comment)));
            continue;
        }

        // Block comment
        if pos + 1 < chars.len() && chars[pos] == '/' && chars[pos + 1] == '*' {
            pos += 2;
            let start = pos;
            while pos + 1 < chars.len() && !(chars[pos] == '*' && chars[pos + 1] == '/') {
                pos += 1;
            }
            let comment: String = chars[start..pos].iter().collect();
            pos += 2; // Skip */
            elements.push(CodeElement::Comment(Comment::Block(comment)));
            continue;
        }

        // Identifier
        if chars[pos].is_alphabetic() || chars[pos] == '_' {
            let start = pos;
            while pos < chars.len() && (chars[pos].is_alphanumeric() || chars[pos] == '_') {
                pos += 1;
            }
            let ident: String = chars[start..pos].iter().collect();
            elements.push(CodeElement::Identifier(ident));
            continue;
        }

        // Skip unknown character
        pos += 1;
    }

    Ok(elements)
}

/// Strip comments from code
pub fn strip_comments(input: &str) -> String {
    let elements = parse_with_comments(input).unwrap_or_default();
    let mut result = String::new();

    for element in elements {
        match element {
            CodeElement::Identifier(ident) => {
                if !result.is_empty() && !result.ends_with('\n') && !result.ends_with(' ') {
                    result.push(' ');
                }
                result.push_str(&ident);
            }
            CodeElement::Newline => {
                result.push('\n');
            }
            CodeElement::Comment(_) => {
                // Skip comments
            }
        }
    }

    result.trim().to_string()
}

fn main() {
    println!("Comment Parser Example");
    println!("======================\n");

    let code = r#"
// This is a line comment
variable1
variable2 // inline comment
/* This is a
   block comment */
variable3 /* inline block */ variable4
"#;

    println!("Input Code:");
    println!("-----------");
    println!("{}", code);

    println!("\nParsed Elements:");
    println!("----------------");
    match parse_with_comments(code) {
        Ok(elements) => {
            for element in elements {
                match element {
                    CodeElement::Identifier(ident) => println!("  IDENT: {}", ident),
                    CodeElement::Comment(Comment::Line(c)) => {
                        println!("  LINE_COMMENT: {}", c.trim())
                    }
                    CodeElement::Comment(Comment::Block(c)) => {
                        println!("  BLOCK_COMMENT: {}", c.trim().replace('\n', "\\n"))
                    }
                    CodeElement::Newline => println!("  NEWLINE"),
                }
            }
        }
        Err(e) => println!("Error: {}", e),
    }

    println!("\nStripped Code (no comments):");
    println!("----------------------------");
    println!("{}", strip_comments(code));
}
