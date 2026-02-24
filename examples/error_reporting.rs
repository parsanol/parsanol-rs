//! Error Reporting Example
//!
//! This example demonstrates rich error reporting for parsers.
//! Shows how to provide helpful error messages with position info.
//! Based on the Parslet deepest_errors.rb and nested_errors.rb examples.
//!
//! Run with: cargo run --example error_reporting --no-default-features

#![allow(clippy::print_literal)]
#![allow(clippy::get_first)]

use parsanol::portable::{
    parser_dsl::{choice, dynamic, re, seq, str, GrammarBuilder},
    AstArena, Grammar, PortableParser,
};

/// Build a grammar that demonstrates error reporting
fn build_grammar() -> Grammar {
    GrammarBuilder::new()
        // Whitespace
        .rule("space", re("[ \\t]+"))
        // Newline
        .rule("newline", re("[\\r\\n]"))
        // Comment: # to end of line
        .rule(
            "comment",
            seq(vec![dynamic(str("#")), dynamic(re("[^\\r\\n]*"))]),
        )
        // Identifier
        .rule("identifier", re("[a-zA-Z_][a-zA-Z0-9_]*"))
        // Number
        .rule("number", re("[0-9]+(\\.[0-9]+)?"))
        // Expression
        .rule(
            "expr",
            choice(vec![
                dynamic(re("[0-9]+(\\.[0-9]+)?")),
                dynamic(re("[a-zA-Z_][a-zA-Z0-9_]*")),
            ]),
        )
        .build()
}

/// Error information
#[derive(Debug, Clone)]
pub struct ParseErrorInfo {
    pub message: String,
    pub position: usize,
    pub line: usize,
    pub column: usize,
    pub context: String,
}

impl std::fmt::Display for ParseErrorInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Error at line {}, column {} (position {}): {}\nContext: {}",
            self.line, self.column, self.position, self.message, self.context
        )
    }
}

/// Convert position to line/column
pub fn position_to_line_col(input: &str, pos: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;

    for (i, c) in input.chars().enumerate() {
        if i >= pos {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }

    (line, col)
}

/// Get context around a position
pub fn get_context(input: &str, pos: usize, radius: usize) -> String {
    let lines: Vec<&str> = input.lines().collect();
    let (line, _col) = position_to_line_col(input, pos);

    let start_line = line.saturating_sub(radius);
    let end_line = (line + radius).min(lines.len());

    let mut context = String::new();
    for i in start_line..end_line {
        let prefix = if i + 1 == line { ">>> " } else { "    " };
        if let Some(l) = lines.get(i) {
            context.push_str(&format!("{}{:3}: {}\n", prefix, i + 1, l));
        }
    }
    context
}

/// Create detailed error info
pub fn create_error_info(input: &str, pos: usize, message: &str) -> ParseErrorInfo {
    let (line, col) = position_to_line_col(input, pos);
    let context = get_context(input, pos, 2);

    ParseErrorInfo {
        message: message.to_string(),
        position: pos,
        line,
        column: col,
        context,
    }
}

/// Prettify input with line numbers
pub fn prettify(input: &str) -> String {
    let mut result = String::new();
    for (i, line) in input.lines().enumerate() {
        result.push_str(&format!("{:02} {}\n", i + 1, line));
    }
    result
}

/// Try parsing with detailed error reporting
pub fn parse_with_errors(input: &str) -> Result<(), ParseErrorInfo> {
    let grammar = build_grammar();
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    match parser.parse() {
        Ok(_) => Ok(()),
        Err(_) => {
            // Find the deepest failure point
            let pos = find_failure_point(input);
            Err(create_error_info(input, pos, "Failed to parse input"))
        }
    }
}

/// Find the deepest failure point in the input
fn find_failure_point(input: &str) -> usize {
    let grammar = build_grammar();

    // Try parsing progressively longer prefixes
    for i in 1..=input.len() {
        let prefix = &input[..i];
        let mut arena = AstArena::for_input(prefix.len());
        let mut parser = PortableParser::new(&grammar, prefix, &mut arena);

        if parser.parse().is_err() {
            // Check if adding more characters helps
            if i < input.len() {
                let next_prefix = &input[..i + 1];
                let mut arena2 = AstArena::for_input(next_prefix.len());
                let mut parser2 = PortableParser::new(&grammar, next_prefix, &mut arena2);

                if parser2.parse().is_ok() {
                    continue;
                }
            }
            return i.saturating_sub(1);
        }
    }

    input.len()
}

fn main() {
    println!("Error Reporting Example");
    println!("=======================\n");

    // Test valid input
    println!("=== Valid Input ===");
    let valid = "hello";
    println!("Input: {:?}", valid);
    match parse_with_errors(valid) {
        Ok(_) => println!("Result: Parsed successfully"),
        Err(e) => println!("Error: {}", e),
    }
    println!();

    // Test invalid input with error
    println!("=== Invalid Input (Single Line) ===");
    let invalid = "hello world!";
    println!("Input: {:?}", invalid);
    match parse_with_errors(invalid) {
        Ok(_) => println!("Result: Parsed successfully"),
        Err(e) => {
            println!("Error:\n{}", e);
        }
    }
    println!();

    // Test multi-line input
    println!("=== Multi-line Input ===");
    let multiline = r#"# Comment
identifier
42
bad@symbol
another
"#;
    println!("Input:");
    println!("{}", prettify(multiline));
    println!("Parsing:");
    match parse_with_errors(multiline) {
        Ok(_) => println!("Result: Parsed successfully"),
        Err(e) => {
            println!("Error at position {}:", e.position);
            println!("{}", e.context);
        }
    }
    println!();

    // Demonstrate position conversion
    println!("=== Position to Line/Column Conversion ===");
    let test = "line1\nline2\nline3\n";
    println!("Input: {:?}", test);
    for pos in [0, 3, 5, 6, 10, 12, 17] {
        let (line, col) = position_to_line_col(test, pos);
        println!("  Position {} => Line {}, Column {}", pos, line, col);
    }
    println!();

    // Context extraction
    println!("=== Context Extraction ===");
    let long_input = r#"first line
second line
third line
fourth line
fifth line
sixth line
"#;
    println!("Input:");
    println!("{}", long_input);
    println!("Context around line 4:");
    println!("{}", get_context(long_input, 30, 2));

    // Summary
    println!("=== Summary ===");
    println!("Error reporting features:");
    println!("  - Position to line/column conversion");
    println!("  - Context extraction around errors");
    println!("  - Formatted output with line numbers");
    println!("  - Deeper failure point detection");
}
