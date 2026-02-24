//! Deepest Error Reporting Example
//!
//! Demonstrates finding the deepest parse failure point for better error messages.
//!
//! Run with: cargo run --example deepest-errors --no-default-features

#![allow(clippy::print_literal)]

use parsanol::portable::{
    parser_dsl::{choice, dynamic, re, ref_, seq, str, GrammarBuilder},
    AstArena, Grammar, PortableParser,
};

/// Build a simple grammar for error demonstration
fn build_grammar() -> Grammar {
    GrammarBuilder::new()
        // Basic tokens
        .rule("identifier", re(r"[a-zA-Z_][a-zA-Z0-9_]*"))
        .rule("number", re(r"[0-9]+"))
        // Expression
        .rule(
            "expr",
            choice(vec![
                dynamic(re(r"[a-zA-Z_][a-zA-Z0-9_]*")),
                dynamic(re(r"[0-9]+")),
            ]),
        )
        // Statement: identifier = expr
        .rule(
            "statement",
            seq(vec![
                dynamic(ref_("identifier")),
                dynamic(str("=")),
                dynamic(ref_("expr")),
            ]),
        )
        .build()
}

/// Find the deepest error position
fn find_deepest_error(input: &str, grammar: &Grammar) -> (usize, String) {
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(grammar, input, &mut arena);

    match parser.parse() {
        Ok(_) => (0, "Success".to_string()),
        Err(e) => {
            // Find deepest position from error
            let error_str = e.to_string();
            let pos = error_str
                .find("at position")
                .map(|p| {
                    let rest = &error_str[p..];
                    rest.split_whitespace()
                        .nth(2)
                        .and_then(|s| s.trim_end_matches(':').parse::<usize>().ok())
                        .unwrap_or(0)
                })
                .unwrap_or(0);

            (pos, error_str)
        }
    }
}

/// Format input with position marker
fn format_error(input: &str, position: usize) -> String {
    let lines: Vec<&str> = input.lines().collect();
    let mut current_pos = 0;
    let mut result = String::new();

    for (i, line) in lines.iter().enumerate() {
        result.push_str(&format!("{:02} {}\n", i + 1, line));

        let line_end = current_pos + line.len();
        if position >= current_pos && position <= line_end {
            let col = position - current_pos;
            result.push_str(&format!("   {}^\n", " ".repeat(col)));
        }
        current_pos = line_end + 1; // +1 for newline
    }

    result
}

fn main() {
    println!("Deepest Error Reporting Example");
    println!("================================");
    println!();

    let grammar = build_grammar();

    // Test cases
    let test_cases = [
        // Valid
        ("x = 42", true),
        ("name = hello", true),
        // Invalid
        ("= 42", false),
        ("x =", false),
        ("x = ", false),
    ];

    for (input, should_succeed) in test_cases {
        println!("---");
        println!("Input: {:?}", input);

        let (pos, error) = find_deepest_error(input, &grammar);

        if should_succeed {
            if pos == 0 && error == "Success" {
                println!("OK Parsed successfully");
            } else {
                println!("FAIL Unexpected error: {}", error);
            }
        } else {
            println!("Error at position {}", pos);
            println!("{}", format_error(input, pos));
        }
        println!();
    }

    println!("---");
    println!("Deepest error reporting finds the point where parsing");
    println!("progressed furthest before failing, providing more");
    println!("useful error messages than just 'expected something'.");
}
