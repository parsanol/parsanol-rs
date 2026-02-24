//! String Literal Parser Example
//!
//! This example demonstrates parsing string literals with escape sequences.
//! Shows handling of quotes, escapes, and constructing useful ASTs.
//! Based on the Parslet string_parser.rb example.
//!
//! Run with: cargo run --example string_literal_parser --no-default-features

#![allow(clippy::print_literal)]

use parsanol::portable::{
    parser_dsl::{choice, dynamic, re, seq, str, GrammarBuilder},
    AstArena, Grammar, PortableParser,
};

/// Build a string/integer literal grammar
fn build_literals_grammar() -> Grammar {
    GrammarBuilder::new()
        // Whitespace
        .rule("space", re("[ \\t]+"))
        // Integer: one or more digits
        .rule("integer", re("[0-9]+"))
        // Escape sequence: \ followed by any character
        .rule("escape", seq(vec![dynamic(str("\\")), dynamic(re("."))]))
        // String content: escape sequence or non-quote character
        .rule(
            "string_content",
            choice(vec![
                dynamic(seq(vec![dynamic(str("\\")), dynamic(re("."))])),
                dynamic(re("[^\"]")),
            ]),
        )
        // String: " content "
        .rule(
            "string",
            seq(vec![
                dynamic(str("\"")),
                dynamic(re("(?:\\\\.|[^\"])*")),
                dynamic(str("\"")),
            ]),
        )
        // Literal: integer or string
        .rule(
            "literal",
            choice(vec![
                dynamic(re("[0-9]+")),
                dynamic(seq(vec![
                    dynamic(str("\"")),
                    dynamic(re("(?:\\\\.|[^\"])*")),
                    dynamic(str("\"")),
                ])),
            ]),
        )
        // Line: literal followed by optional whitespace and newline
        .rule(
            "line",
            seq(vec![
                dynamic(choice(vec![
                    dynamic(re("[0-9]+")),
                    dynamic(seq(vec![
                        dynamic(str("\"")),
                        dynamic(re("(?:\\\\.|[^\"])*")),
                        dynamic(str("\"")),
                    ])),
                ])),
                dynamic(re("[ \\t]*")),
                dynamic(re("\\n?")),
            ]),
        )
        .build()
}

/// Parsed literal types
#[derive(Debug, Clone)]
pub enum Literal {
    Integer(i64),
    String(String),
}

impl std::fmt::Display for Literal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Literal::Integer(n) => write!(f, "Int({})", n),
            Literal::String(s) => write!(f, "Str({:?})", s),
        }
    }
}

/// Parse a string literal with escape sequence handling
pub fn parse_string_literal(input: &str) -> Result<String, String> {
    let input = input.trim();

    if !input.starts_with('"') || !input.ends_with('"') {
        return Err("String must be quoted".to_string());
    }

    let content = &input[1..input.len() - 1];
    let mut result = String::new();
    let chars: Vec<char> = content.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            // Handle escape sequence
            let next = chars[i + 1];
            match next {
                'n' => result.push('\n'),
                't' => result.push('\t'),
                'r' => result.push('\r'),
                '\\' => result.push('\\'),
                '"' => result.push('"'),
                '0' => result.push('\0'),
                c => result.push(c), // Unknown escape, just include the character
            }
            i += 2;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    Ok(result)
}

/// Parse an integer literal
pub fn parse_integer_literal(input: &str) -> Result<i64, String> {
    input.trim().parse::<i64>().map_err(|e| e.to_string())
}

/// Parse any literal
pub fn parse_literal(input: &str) -> Result<Literal, String> {
    let input = input.trim();

    if input.starts_with('"') && input.ends_with('"') {
        let s = parse_string_literal(input)?;
        Ok(Literal::String(s))
    } else if input.chars().all(|c| c.is_ascii_digit() || c == '-') {
        let n = parse_integer_literal(input)?;
        Ok(Literal::Integer(n))
    } else {
        Err(format!("Unknown literal type: {}", input))
    }
}

/// Parse multiple literals from input (one per line)
pub fn parse_literals(input: &str) -> Vec<Result<Literal, String>> {
    input
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(parse_literal)
        .collect()
}

fn main() {
    println!("String Literal Parser Example");
    println!("=============================\n");

    // Test integer parsing
    println!("=== Integer Literals ===");
    let integers = vec!["42", "0", "123456", "-10"];
    for input in integers {
        match parse_integer_literal(input) {
            Ok(n) => println!("  {} => {}", input, n),
            Err(e) => println!("  {} => ERROR: {}", input, e),
        }
    }
    println!();

    // Test string parsing
    println!("=== String Literals ===");
    let strings = vec![
        r#""hello""#,
        r#""hello world""#,
        r#""line1\nline2""#,
        r#""tab\there""#,
        r#""quote: \"test\"""#,
        r#""backslash: \\""#,
        r#""""#,
    ];
    for input in strings {
        match parse_string_literal(input) {
            Ok(s) => println!("  {} => {:?}", input, s),
            Err(e) => println!("  {} => ERROR: {}", input, e),
        }
    }
    println!();

    // Test mixed literals
    println!("=== Mixed Literals ===");
    let literals = vec![
        r#""hello""#,
        "42",
        r#""escape\nsequence""#,
        "100",
        r#""quoted \"inner\" string""#,
    ];
    for input in literals {
        match parse_literal(input) {
            Ok(lit) => println!("  {} => {}", input, lit),
            Err(e) => println!("  {} => ERROR: {}", input, e),
        }
    }
    println!();

    // Parse multiple lines
    println!("=== Multi-line Input ===");
    let input = r#"42
"hello world"
123
"line1\nline2"
"test"
"#;
    println!("Input:");
    println!("{}", input);
    println!("\nParsed:");
    for result in parse_literals(input) {
        match result {
            Ok(lit) => println!("  {}", lit),
            Err(e) => println!("  ERROR: {}", e),
        }
    }

    // Grammar parsing
    println!("\n=== Grammar Parsing ===");
    let grammar = build_literals_grammar();

    let test_cases = vec![
        ("42", "integer"),
        (r#""hello""#, "string"),
        (r#""escape\nseq""#, "string"),
    ];

    for (input, rule) in test_cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        let result = parser.parse();
        println!("  {} ({}) => {:?}", input, rule, result.is_ok());
    }
}
