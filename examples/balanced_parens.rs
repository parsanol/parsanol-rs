//! Balanced Parentheses Parser Example
//!
//! This example demonstrates parsing nested balanced structures.
//! Shows handling of recursion and tree pattern matching.
//! Based on the Parslet parens.rb example.
//!
//! Run with: cargo run --example balanced_parens --no-default-features

#![allow(clippy::print_literal)]

use parsanol::portable::{
    parser_dsl::{choice, dynamic, re, seq, str, GrammarBuilder},
    AstArena, Grammar, PortableParser,
};

/// Build a balanced parentheses grammar
fn build_parens_grammar() -> Grammar {
    GrammarBuilder::new()
        // Balanced: ( balanced? )
        .rule(
            "balanced",
            choice(vec![
                dynamic(seq(vec![
                    dynamic(str("(")),
                    dynamic(re("[^()]*")), // Simple content (no nesting for now)
                    dynamic(str(")")),
                ])),
                dynamic(re("[^()]+")), // Non-paren text
            ]),
        )
        // Multiple balanced
        .rule("balanced_list", re("[^()]*(?:\\([^()]*\\)[^()]*)*"))
        .build()
}

/// Parentheses node
#[derive(Debug, Clone)]
pub enum ParenNode {
    /// Balanced pair with content
    Balanced {
        content: Box<ParenNode>,
        depth: usize,
    },
    /// Text content
    Text(String),
    /// Empty balanced pair
    Empty(usize),
}

impl std::fmt::Display for ParenNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParenNode::Balanced { content, depth } => {
                write!(f, "{}({})", "  ".repeat(*depth), content)
            }
            ParenNode::Text(t) => write!(f, "{}", t),
            ParenNode::Empty(depth) => write!(f, "{}()", "  ".repeat(*depth)),
        }
    }
}

/// Parse balanced parentheses
pub fn parse_parens(input: &str) -> Result<ParenNode, String> {
    let grammar = build_parens_grammar();
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    parser
        .parse()
        .map_err(|e| format!("Parse error: {:?}", e))?;

    // Manual parsing with depth tracking
    parse_with_depth(input, 0)
}

fn parse_with_depth(input: &str, depth: usize) -> Result<ParenNode, String> {
    let input = input.trim();

    if input.is_empty() {
        return Ok(ParenNode::Text(String::new()));
    }

    // Check for balanced pair
    if input.starts_with('(') && input.ends_with(')') {
        let inner = &input[1..input.len() - 1];
        if inner.is_empty() {
            return Ok(ParenNode::Empty(depth));
        }
        let content = parse_with_depth(inner, depth + 1)?;
        return Ok(ParenNode::Balanced {
            content: Box::new(content),
            depth,
        });
    }

    // Text content
    Ok(ParenNode::Text(input.to_string()))
}

/// Calculate the maximum nesting depth
pub fn max_depth(input: &str) -> usize {
    let mut max: usize = 0;
    let mut current: usize = 0;

    for c in input.chars() {
        match c {
            '(' => {
                current += 1;
                max = max.max(current);
            }
            ')' => {
                current = current.saturating_sub(1);
            }
            _ => {}
        }
    }

    max
}

/// Check if parentheses are balanced
pub fn is_balanced(input: &str) -> bool {
    let mut count = 0;

    for c in input.chars() {
        match c {
            '(' => count += 1,
            ')' => {
                if count == 0 {
                    return false;
                }
                count -= 1;
            }
            _ => {}
        }
    }

    count == 0
}

fn main() {
    println!("Balanced Parentheses Parser Example");
    println!("===================================\n");

    let examples = [
        ("()", true, 1),
        ("(())", true, 2),
        ("((()))", true, 3),
        ("(()())", true, 2),
        ("()()()", true, 1),
        ("(())(())", true, 2),
        ("(()", false, 0),   // Unbalanced
        ("())", false, 0),   // Unbalanced
        ("((())", false, 0), // Unbalanced
        ("text", true, 0),
        ("(text)", true, 1),
        ("((text))", true, 2),
    ];

    println!(
        "{:<15} | {:<10} | {:<10} | {}",
        "Input", "Balanced?", "Max Depth", "Status"
    );
    println!("{}", "-".repeat(60));

    for (input, expected_balanced, expected_depth) in examples {
        let balanced = is_balanced(input);
        let depth = max_depth(input);
        let status = if balanced == expected_balanced && depth == expected_depth {
            "✓"
        } else {
            "✗"
        };
        println!(
            "{:<15} | {:<10} | {:<10} | {}",
            input, balanced, depth, status
        );
    }

    // Parse tree visualization
    println!("\nParse Tree Examples:");
    println!("--------------------");

    let tree_examples = ["()", "(())", "((()))", "(()())"];

    for input in tree_examples {
        println!("\n{}:", input);
        match parse_parens(input) {
            Ok(node) => print_node(&node, 0),
            Err(e) => println!("  Error: {}", e),
        }
    }
}

fn print_node(node: &ParenNode, indent: usize) {
    let pad = "  ".repeat(indent);
    match node {
        ParenNode::Balanced { content, depth } => {
            println!("{}Balanced (depth={}):", pad, depth);
            print_node(content, indent + 1);
        }
        ParenNode::Text(t) => {
            println!("{}Text: {:?}", pad, t);
        }
        ParenNode::Empty(depth) => {
            println!("{}Empty (depth={})", pad, depth);
        }
    }
}
