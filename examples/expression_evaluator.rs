//! Expression Evaluator Example
//!
//! This example demonstrates building a complete expression parser and evaluator.
//! Shows operator precedence, variables, and function calls.
//!
//! Run with: cargo run --example expression_evaluator --no-default-features

#![allow(clippy::print_literal)]
#![allow(clippy::get_first)]

use parsanol::portable::{
    infix::{Assoc, InfixBuilder},
    parser_dsl::{choice, dynamic, re, ref_, seq, str, GrammarBuilder},
    Grammar,
};
use std::collections::HashMap;

/// Build an expression grammar with full precedence
#[allow(dead_code)]
fn build_expression_grammar() -> Grammar {
    let mut builder = GrammarBuilder::new();

    // Basic atoms
    builder = builder.rule("number", re(r"[0-9]+(\.[0-9]+)?"));
    builder = builder.rule("identifier", re(r"[a-zA-Z_][a-zA-Z0-9_]*"));

    // Function call: name(args)
    builder = builder.rule(
        "funcall",
        seq(vec![
            dynamic(re(r"[a-zA-Z_][a-zA-Z0-9_]*")),
            dynamic(str("(")),
            dynamic(re(r"[^)]*")), // simplified args
            dynamic(str(")")),
        ]),
    );

    // Primary: number, function call, identifier, or parenthesized expression
    builder = builder.rule(
        "primary",
        choice(vec![
            dynamic(seq(vec![
                dynamic(str("(")),
                dynamic(ref_("expr")),
                dynamic(str(")")),
            ])),
            dynamic(seq(vec![
                dynamic(re(r"[a-zA-Z_][a-zA-Z0-9_]*")),
                dynamic(str("(")),
                dynamic(re(r"[^)]*")),
                dynamic(str(")")),
            ])),
            dynamic(re(r"[0-9]+(\.[0-9]+)?")),
            dynamic(re(r"[a-zA-Z_][a-zA-Z0-9_]*")),
        ]),
    );

    // Build expression with operator precedence
    // Higher precedence = binds tighter
    let expr_atom = InfixBuilder::new()
        .primary(ref_("primary"))
        // Comparison (lowest precedence)
        .op("==", 1, Assoc::NonAssoc)
        .op("!=", 1, Assoc::NonAssoc)
        .op("<", 1, Assoc::NonAssoc)
        .op("<=", 1, Assoc::NonAssoc)
        .op(">", 1, Assoc::NonAssoc)
        .op(">=", 1, Assoc::NonAssoc)
        // Addition/subtraction
        .op("+", 2, Assoc::Left)
        .op("-", 2, Assoc::Left)
        // Multiplication/division/modulo
        .op("*", 3, Assoc::Left)
        .op("/", 3, Assoc::Left)
        .op("%", 3, Assoc::Left)
        // Exponentiation (right associative)
        .op("^", 4, Assoc::Right)
        // Unary not (highest)
        .build(&mut builder);

    builder.update_rule("expr", expr_atom);
    builder.build()
}

/// Expression AST
#[derive(Debug, Clone)]
pub enum Expr {
    Number(f64),
    Variable(String),
    BinaryOp {
        left: Box<Expr>,
        op: String,
        right: Box<Expr>,
    },
    UnaryOp {
        op: String,
        operand: Box<Expr>,
    },
    FunctionCall {
        name: String,
        args: Vec<Expr>,
    },
}

/// Evaluation context with variables and functions
#[derive(Debug, Clone, Default)]
pub struct EvalContext {
    pub variables: HashMap<String, f64>,
    pub functions: HashMap<String, fn(&[f64]) -> f64>,
}

impl EvalContext {
    /// Create a new context with standard library
    pub fn new() -> Self {
        let mut ctx = Self::default();

        // Standard functions
        ctx.functions.insert("sin".to_string(), |args| {
            args.get(0).copied().unwrap_or(0.0).sin()
        });
        ctx.functions.insert("cos".to_string(), |args| {
            args.get(0).copied().unwrap_or(0.0).cos()
        });
        ctx.functions.insert("tan".to_string(), |args| {
            args.get(0).copied().unwrap_or(0.0).tan()
        });
        ctx.functions.insert("sqrt".to_string(), |args| {
            args.get(0).copied().unwrap_or(0.0).sqrt()
        });
        ctx.functions.insert("abs".to_string(), |args| {
            args.get(0).copied().unwrap_or(0.0).abs()
        });
        ctx.functions.insert("floor".to_string(), |args| {
            args.get(0).copied().unwrap_or(0.0).floor()
        });
        ctx.functions.insert("ceil".to_string(), |args| {
            args.get(0).copied().unwrap_or(0.0).ceil()
        });
        ctx.functions.insert("round".to_string(), |args| {
            args.get(0).copied().unwrap_or(0.0).round()
        });
        ctx.functions.insert("min".to_string(), |args| {
            args.get(0)
                .copied()
                .unwrap_or(0.0)
                .min(args.get(1).copied().unwrap_or(0.0))
        });
        ctx.functions.insert("max".to_string(), |args| {
            args.get(0)
                .copied()
                .unwrap_or(0.0)
                .max(args.get(1).copied().unwrap_or(0.0))
        });

        // Constants
        ctx.variables.insert("PI".to_string(), std::f64::consts::PI);
        ctx.variables.insert("E".to_string(), std::f64::consts::E);

        ctx
    }

    /// Set a variable
    pub fn set(&mut self, name: &str, value: f64) {
        self.variables.insert(name.to_string(), value);
    }

    /// Evaluate an expression
    pub fn eval(&self, expr: &Expr) -> Result<f64, String> {
        match expr {
            Expr::Number(n) => Ok(*n),
            Expr::Variable(name) => self
                .variables
                .get(name)
                .copied()
                .ok_or_else(|| format!("Unknown variable: {}", name)),
            Expr::BinaryOp { left, op, right } => {
                let l = self.eval(left)?;
                let r = self.eval(right)?;
                match op.as_str() {
                    "+" => Ok(l + r),
                    "-" => Ok(l - r),
                    "*" => Ok(l * r),
                    "/" => Ok(l / r),
                    "%" => Ok(l % r),
                    "^" => Ok(l.powf(r)),
                    "==" => Ok(if l == r { 1.0 } else { 0.0 }),
                    "!=" => Ok(if l != r { 1.0 } else { 0.0 }),
                    "<" => Ok(if l < r { 1.0 } else { 0.0 }),
                    "<=" => Ok(if l <= r { 1.0 } else { 0.0 }),
                    ">" => Ok(if l > r { 1.0 } else { 0.0 }),
                    ">=" => Ok(if l >= r { 1.0 } else { 0.0 }),
                    _ => Err(format!("Unknown operator: {}", op)),
                }
            }
            Expr::UnaryOp { op, operand } => {
                let v = self.eval(operand)?;
                match op.as_str() {
                    "-" => Ok(-v),
                    "!" => Ok(if v == 0.0 { 1.0 } else { 0.0 }),
                    _ => Err(format!("Unknown unary operator: {}", op)),
                }
            }
            Expr::FunctionCall { name, args } => {
                let arg_values: Result<Vec<f64>, _> = args.iter().map(|a| self.eval(a)).collect();
                let arg_values = arg_values?;

                if let Some(func) = self.functions.get(name) {
                    Ok(func(&arg_values))
                } else {
                    Err(format!("Unknown function: {}", name))
                }
            }
        }
    }
}

/// Parse an expression string (simplified manual parsing)
pub fn parse_expression(input: &str) -> Result<Expr, String> {
    let input = input.trim();

    // Number
    if let Ok(n) = input.parse::<f64>() {
        return Ok(Expr::Number(n));
    }

    // Parenthesized expression
    if input.starts_with('(') && input.ends_with(')') {
        return parse_expression(&input[1..input.len() - 1]);
    }

    // Function call
    if let Some(idx) = input.find('(') {
        if input.ends_with(')') {
            let name = &input[..idx];
            let args_str = &input[idx + 1..input.len() - 1];
            let args = parse_args(args_str)?;
            return Ok(Expr::FunctionCall {
                name: name.to_string(),
                args,
            });
        }
    }

    // Binary operators (low to high precedence)
    for op in [
        "==", "!=", "<=", ">=", "<", ">", "+", "-", "*", "/", "%", "^",
    ] {
        // Find rightmost occurrence for left-associative, leftmost for right-associative
        let idx = if op == "^" {
            input.find(op)
        } else {
            input.rfind(op)
        };

        if let Some(idx) = idx {
            // Skip if inside parentheses
            let before = &input[..idx];
            let after = &input[idx + op.len()..];

            // Check parentheses balance
            let open = before.matches('(').count();
            let close = before.matches(')').count();
            if open != close {
                continue;
            }

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

    // Variable
    if input.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Ok(Expr::Variable(input.to_string()));
    }

    Err(format!("Could not parse: {}", input))
}

/// Parse comma-separated arguments
fn parse_args(input: &str) -> Result<Vec<Expr>, String> {
    if input.trim().is_empty() {
        return Ok(Vec::new());
    }

    let mut args = Vec::new();
    let mut current = String::new();
    let mut depth = 0;

    for c in input.chars() {
        match c {
            '(' => {
                depth += 1;
                current.push(c);
            }
            ')' => {
                depth -= 1;
                current.push(c);
            }
            ',' if depth == 0 => {
                if !current.trim().is_empty() {
                    args.push(parse_expression(&current)?);
                }
                current.clear();
            }
            _ => current.push(c),
        }
    }

    if !current.trim().is_empty() {
        args.push(parse_expression(&current)?);
    }

    Ok(args)
}

fn main() {
    println!("Expression Evaluator Example");
    println!("============================\n");

    let mut ctx = EvalContext::new();
    ctx.set("x", 10.0);
    ctx.set("y", 5.0);

    let expressions = [
        "1 + 2 * 3",
        "(1 + 2) * 3",
        "2 ^ 3 ^ 2", // Right associative: 2^(3^2) = 2^9 = 512
        "x + y",
        "x * y - 5",
        "sin(PI / 2)",
        "sqrt(16)",
        "max(x, y)",
        "x > y",
        "min(sin(0), cos(0))",
    ];

    println!(
        "Variables: x = {}, y = {}",
        ctx.variables.get("x").unwrap(),
        ctx.variables.get("y").unwrap()
    );
    println!(
        "Constants: PI = {}, E = {}\n",
        std::f64::consts::PI,
        std::f64::consts::E
    );

    println!("{:<25} | {:<15} | {}", "Expression", "Result", "AST");
    println!("{}", "-".repeat(70));

    for expr_str in expressions {
        match parse_expression(expr_str) {
            Ok(expr) => match ctx.eval(&expr) {
                Ok(result) => {
                    println!("{:<25} | {:<15.4} | {:?}", expr_str, result, expr);
                }
                Err(e) => {
                    println!("{:<25} | Error: {}", expr_str, e);
                }
            },
            Err(e) => {
                println!("{:<25} | Parse Error: {}", expr_str, e);
            }
        }
    }
}
