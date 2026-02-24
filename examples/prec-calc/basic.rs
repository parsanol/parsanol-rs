//! Precedence Calculator Example
//!
//! Demonstrates precedence climbing for infix expression parsing.
//! Shows how to parse variable assignments with proper operator precedence.
//!
//! Run with: cargo run --example prec-calc --no-default-features

#![allow(clippy::print_literal)]

use std::collections::HashMap;

/// Expression AST
#[derive(Debug, Clone)]
pub enum Expr {
    Integer(i64),
    BinaryOp {
        left: Box<Expr>,
        op: String,
        right: Box<Expr>,
    },
}

/// Variable bindings
#[derive(Debug, Clone, Default)]
pub struct Bindings {
    vars: HashMap<String, i64>,
}

impl Bindings {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, name: &str, value: i64) {
        self.vars.insert(name.to_string(), value);
    }

    pub fn get(&self, name: &str) -> Option<i64> {
        self.vars.get(name).copied()
    }
}

/// Evaluate an expression
fn eval(expr: &Expr) -> i64 {
    match expr {
        Expr::Integer(n) => *n,
        Expr::BinaryOp { left, op, right } => {
            let l = eval(left);
            let r = eval(right);
            match op.as_str() {
                "+" => l + r,
                "-" => l - r,
                "*" => l * r,
                "/" => l / r,
                _ => panic!("Unknown operator: {}", op),
            }
        }
    }
}

/// Parse and evaluate a simple expression (manual parsing for demo)
fn parse_expression(input: &str) -> Result<Expr, String> {
    let input = input.trim();

    // Try to parse as integer first
    if let Ok(n) = input.parse::<i64>() {
        return Ok(Expr::Integer(n));
    }

    // Binary operators (low to high precedence)
    for op in ["+", "-", "*", "/"] {
        // Find rightmost for left-assoc, leftmost for right-assoc
        let idx = if op == "+" {
            input.rfind(op)
        } else {
            input.find(op).or_else(|| input.rfind(op))
        };

        if let Some(idx) = idx {
            let before = &input[..idx];
            let after = &input[idx + 1..];

            if !before.is_empty() && !after.is_empty() {
                let left = parse_expression(before)?;
                let right = parse_expression(after)?;
                return Ok(Expr::BinaryOp {
                    left: Box::new(left),
                    op: op.to_string(),
                    right: Box::new(right),
                });
            }
        }
    }

    Err(format!("Could not parse: {}", input))
}

/// Parse and evaluate assignments
fn parse_assignments(input: &str) -> Bindings {
    let mut bindings = Bindings::new();

    for line in input.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(eq_pos) = line.find('=') {
            let name = line[..eq_pos].trim();
            let expr_str = line[eq_pos + 1..].trim();

            match parse_expression(expr_str) {
                Ok(expr) => {
                    let value = eval(&expr);
                    bindings.set(name, value);
                    println!("  {} = {} (from {})", name, value, expr_str);
                }
                Err(e) => {
                    println!("  Error parsing '{}': {}", expr_str, e);
                }
            }
        }
    }

    bindings
}

fn main() {
    println!("Precedence Calculator Example");
    println!("==============================");
    println!();

    let input = r#"
a = 1
b = 2
c = 3 * 25
d = 100 + 3*4
e = 10 - 3 - 2
"#;

    println!("Input:");
    println!("{}", input);
    println!();

    println!("Parsing and evaluating:");
    let bindings = parse_assignments(input);

    println!();
    println!("Final bindings:");
    for (name, value) in bindings.vars.iter() {
        println!("  {} = {}", name, value);
    }

    // Demonstrate precedence
    println!();
    println!("Precedence demonstration:");
    let test_exprs = [
        ("1 + 2 * 3", 7),     // 1 + (2 * 3) = 7
        ("2 * 3 + 4", 10),    // (2 * 3) + 4 = 10
        ("10 - 3 - 2", 5),    // (10 - 3) - 2 = 5 (left assoc)
        ("100 + 3 * 4", 112), // 100 + (3 * 4) = 112
    ];

    for (expr_str, expected) in test_exprs {
        match parse_expression(expr_str) {
            Ok(expr) => {
                let result = eval(&expr);
                let status = if result == expected { "OK" } else { "FAIL" };
                println!(
                    "  {} = {} (expected {}) {}",
                    expr_str, result, expected, status
                );
            }
            Err(e) => {
                println!("  {} -> Error: {}", expr_str, e);
            }
        }
    }
}
