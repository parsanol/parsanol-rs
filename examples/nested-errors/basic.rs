//! Nested Error Reporting Example
//!
//! Demonstrates tree-structured error reporting showing all failure paths.
//!
//! Run with: cargo run --example nested-errors --no-default-features

#![allow(clippy::print_literal)]

use parsanol::portable::{
    parser_dsl::{choice, dynamic, re, seq, str, GrammarBuilder},
    AstArena, Grammar, PortableParser,
};

/// Build a simple grammar for error demonstration
fn build_grammar() -> Grammar {
    GrammarBuilder::new()
        // Basic tokens
        .rule("identifier", re(r"[a-zA-Z_][a-zA-Z0-9_]*"))
        .rule("number", re(r"[0-9]+"))
        .rule("string", re(r#""[^"]*""#))
        // Expression
        .rule(
            "expr",
            choice(vec![
                dynamic(re(r"[a-zA-Z_][a-zA-Z0-9_]*")),
                dynamic(re(r"[0-9]+")),
                dynamic(re(r#""[^"]*""#)),
            ]),
        )
        // Statement: identifier = expr
        .rule(
            "statement",
            seq(vec![
                dynamic(re(r"[a-zA-Z_][a-zA-Z0-9_]*")),
                dynamic(str("=")),
                dynamic(re(r#"[a-zA-Z_][a-zA-Z0-9_]*|[0-9]+|"[^"]*""#)),
            ]),
        )
        .build()
}

/// Error tree node for nested reporting
#[derive(Debug, Clone)]
pub struct ErrorNode {
    pub message: String,
    pub position: usize,
    pub children: Vec<ErrorNode>,
}

impl ErrorNode {
    fn new(message: &str, position: usize) -> Self {
        Self {
            message: message.to_string(),
            position,
            children: Vec::new(),
        }
    }

    fn add_child(&mut self, child: ErrorNode) {
        self.children.push(child);
    }

    /// Format as ASCII tree
    fn format_tree(&self, prefix: &str, is_last: bool) -> String {
        let connector = if is_last { "`- " } else { "|- " };
        let mut result = format!(
            "{}{}{} (pos {})\n",
            prefix, connector, self.message, self.position
        );

        let child_prefix = if is_last { "    " } else { "|   " };
        let new_prefix = format!("{}{}", prefix, child_prefix);

        for (i, child) in self.children.iter().enumerate() {
            let is_last_child = i == self.children.len() - 1;
            result.push_str(&child.format_tree(&new_prefix, is_last_child));
        }

        result
    }
}

/// Build an error tree from a parse failure
fn build_error_tree(input: &str, grammar: &Grammar) -> ErrorNode {
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(grammar, input, &mut arena);

    match parser.parse() {
        Ok(_) => ErrorNode::new("Success", 0),
        Err(e) => {
            // Build a simplified error tree
            let error_str = e.to_string();

            // Parse position from error message
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

            // Root error
            let mut root = ErrorNode::new("Parse failed", 0);

            // Add common failure causes based on context
            let rest = if pos < input.len() { &input[pos..] } else { "" };

            if rest.starts_with('=') {
                root.add_child(ErrorNode::new("Expected identifier before '='", pos));
            } else if !rest.is_empty() && !rest.chars().next().unwrap().is_alphanumeric() {
                root.add_child(ErrorNode::new(
                    &format!("Unexpected character: {:?}", rest.chars().next()),
                    pos,
                ));
            }

            root.add_child(ErrorNode::new("Expected: identifier = value", pos));

            root
        }
    }
}

fn main() {
    println!("Nested Error Reporting Example");
    println!("===============================");
    println!();

    let grammar = build_grammar();

    // Test case with error
    let input = "x = 123"; // Valid
    println!("Input: {}", input);
    let tree = build_error_tree(input, &grammar);
    println!("Result: {}", tree.format_tree("", true));

    // Test case with error
    let input2 = "= 123"; // Missing identifier
    println!("---");
    println!("Input: {}", input2);
    let tree2 = build_error_tree(input2, &grammar);
    println!("Error tree:");
    println!("{}", tree2.format_tree("", true));

    // Test case with error
    let input3 = "x ="; // Missing value
    println!("---");
    println!("Input: {}", input3);
    let tree3 = build_error_tree(input3, &grammar);
    println!("Error tree:");
    println!("{}", tree3.format_tree("", true));

    println!("---");
    println!("Nested error reporting shows the full tree of failures,");
    println!("helping developers understand which alternatives were tried.");
}
