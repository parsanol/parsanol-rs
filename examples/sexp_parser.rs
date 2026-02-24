//! S-Expression Parser Example
//!
//! This example demonstrates parsing S-expressions (Lisp-style syntax).
//! Shows recursive grammar definition and nested structure handling.
//!
//! Run with: cargo run --example sexp_parser --no-default-features

#![allow(clippy::get_first)]
#![allow(clippy::single_match)]

use parsanol::portable::{
    parser_dsl::{choice, dynamic, re, seq, str, GrammarBuilder},
    Grammar,
};

/// Build an S-expression grammar
#[allow(dead_code)]
fn build_sexp_grammar() -> Grammar {
    GrammarBuilder::new()
        // Whitespace (spaces, tabs, newlines)
        .rule("whitespace", re("[ \t\n\r]+"))
        // Atom: symbol or number
        .rule("symbol", re("[a-zA-Z_+\\-*/=<>!][a-zA-Z0-9_+\\-*/=<>!]*"))
        .rule("number", re("-?[0-9]+(\\.[0-9]+)?"))
        .rule("string", re("\"[^\"]*\""))
        // Atom: any of the above
        .rule(
            "atom",
            choice(vec![
                dynamic(re("[a-zA-Z_+\\-*/=<>!][a-zA-Z0-9_+\\-*/=<>!]*")),
                dynamic(re("-?[0-9]+(\\.[0-9]+)?")),
                dynamic(re("\"[^\"]*\"")),
            ]),
        )
        // List: (items...)
        .rule(
            "list",
            seq(vec![
                dynamic(str("(")),
                dynamic(re("[ \t\n\r]*")), // optional whitespace
                dynamic(re("\\)*")),       // items (simplified)
                dynamic(str(")")),
            ]),
        )
        // S-expression: atom or list
        .rule(
            "sexp",
            choice(vec![
                dynamic(re("[a-zA-Z_+\\-*/=<>!][a-zA-Z0-9_+\\-*/=<>!]*")),
                dynamic(re("-?[0-9]+(\\.[0-9]+)?")),
                dynamic(re("\"[^\"]*\"")),
                dynamic(seq(vec![
                    dynamic(str("(")),
                    dynamic(re("[^)]*")), // content
                    dynamic(str(")")),
                ])),
            ]),
        )
        .build()
}

/// S-expression value
#[derive(Debug, Clone)]
pub enum SExp {
    /// Symbol (identifier)
    Symbol(String),
    /// Integer number
    Int(i64),
    /// Floating point number
    Float(f64),
    /// String literal
    String(String),
    /// List of S-expressions
    List(Vec<SExp>),
}

impl std::fmt::Display for SExp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SExp::Symbol(s) => write!(f, "{}", s),
            SExp::Int(n) => write!(f, "{}", n),
            SExp::Float(n) => write!(f, "{}", n),
            SExp::String(s) => write!(f, "\"{}\"", s),
            SExp::List(items) => {
                write!(f, "(")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, ")")
            }
        }
    }
}

/// Parse an S-expression string
pub fn parse_sexp(input: &str) -> Result<SExp, String> {
    let input = input.trim();

    // Empty
    if input.is_empty() {
        return Err("Empty input".to_string());
    }

    // List
    if input.starts_with('(') && input.ends_with(')') {
        let inner = &input[1..input.len() - 1];
        let items = parse_sexp_list(inner)?;
        return Ok(SExp::List(items));
    }

    // String
    if input.starts_with('"') && input.ends_with('"') {
        return Ok(SExp::String(input[1..input.len() - 1].to_string()));
    }

    // Number
    if let Ok(n) = input.parse::<i64>() {
        return Ok(SExp::Int(n));
    }
    if let Ok(n) = input.parse::<f64>() {
        return Ok(SExp::Float(n));
    }

    // Symbol
    if input
        .chars()
        .all(|c| c.is_alphanumeric() || "+-*/_=<>!".contains(c))
    {
        return Ok(SExp::Symbol(input.to_string()));
    }

    Err(format!("Invalid S-expression: {}", input))
}

/// Parse a list of S-expressions (inner content of parentheses)
fn parse_sexp_list(input: &str) -> Result<Vec<SExp>, String> {
    let mut items = Vec::new();
    let mut chars = input.chars().peekable();
    let mut current = String::new();
    let mut depth = 0;

    while let Some(c) = chars.next() {
        match c {
            '(' => {
                depth += 1;
                current.push(c);
            }
            ')' => {
                depth -= 1;
                current.push(c);
            }
            '"' => {
                current.push(c);
                // Read until closing quote
                while let Some(&next) = chars.peek() {
                    current.push(chars.next().unwrap());
                    if next == '"' {
                        break;
                    }
                }
            }
            ' ' | '\t' | '\n' | '\r' if depth == 0 => {
                if !current.is_empty() {
                    items.push(parse_sexp(&current)?);
                    current.clear();
                }
            }
            _ => {
                current.push(c);
            }
        }
    }

    if !current.is_empty() {
        items.push(parse_sexp(&current)?);
    }

    Ok(items)
}

fn main() {
    println!("S-Expression Parser Example");
    println!("===========================\n");

    let expressions = [
        "42",
        "-3.14",
        "hello",
        "\"hello world\"",
        "(+ 1 2)",
        "(+ (* 2 3) (- 10 5))",
        "(defun factorial (n) (if (<= n 1) 1 (* n (factorial (- n 1)))))",
        "(list 1 2 3 \"four\" (quote five))",
    ];

    for expr in expressions {
        println!("Input: {}", expr);
        match parse_sexp(expr) {
            Ok(sexp) => {
                println!("Parsed: {}", sexp);
                println!("Debug: {:?}", sexp);
            }
            Err(e) => println!("Error: {}", e),
        }
        println!();
    }

    // Demonstrate evaluation
    println!("Simple Evaluation Demo:");
    println!("-----------------------");

    let expr = "(+ (* 2 3) (- 10 5))";
    match parse_sexp(expr) {
        Ok(SExp::List(items)) => {
            println!("Expression: {}", expr);
            println!("Operator: {:?}", items.first());
            println!("Operands: {:?}", &items[1..]);
            if let Some(SExp::Symbol(op)) = items.first() {
                if op == "+" {
                    let sum: i64 = items[1..]
                        .iter()
                        .filter_map(|x| if let SExp::Int(n) = x { Some(*n) } else { None })
                        .sum();
                    println!("Sum of integers: {}", sum);
                }
            }
        }
        _ => {}
    }
}
