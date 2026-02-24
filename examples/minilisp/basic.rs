//! Mini Lisp Parser Example
//!
//! A simple S-expression parser demonstrating list parsing, atoms, and transforms.
//!
//! Run with: cargo run --example minilisp --no-default-features

#![allow(clippy::print_literal)]

/// Lisp expression AST
#[derive(Debug, Clone)]
pub enum LispExpr {
    Identifier(String),
    Integer(i64),
    Float(f64),
    String(String),
    List(Vec<LispExpr>),
}

impl std::fmt::Display for LispExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LispExpr::Identifier(s) => write!(f, "{}", s),
            LispExpr::Integer(n) => write!(f, "{}", n),
            LispExpr::Float(n) => write!(f, "{}", n),
            LispExpr::String(s) => write!(f, "\"{}\"", s),
            LispExpr::List(items) => {
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

/// Parse a lisp expression (simplified manual parsing for demo)
fn parse_lisp(input: &str) -> Result<LispExpr, String> {
    let input = input.trim();

    // List
    if input.starts_with('(') && input.ends_with(')') {
        let inner = &input[1..input.len() - 1];
        let items = parse_items(inner)?;
        return Ok(LispExpr::List(items));
    }

    // String
    if input.starts_with('"') && input.ends_with('"') {
        return Ok(LispExpr::String(input[1..input.len() - 1].to_string()));
    }

    // Float (has decimal point or exponent)
    if input.contains('.') || input.contains('e') || input.contains('E') {
        if let Ok(n) = input.parse::<f64>() {
            return Ok(LispExpr::Float(n));
        }
    }

    // Integer
    if let Ok(n) = input.parse::<i64>() {
        return Ok(LispExpr::Integer(n));
    }

    // Identifier
    if input
        .chars()
        .all(|c| c.is_alphanumeric() || c == '=' || c == '*' || c == '_' || c == '+' || c == '-')
    {
        return Ok(LispExpr::Identifier(input.to_string()));
    }

    Err(format!("Could not parse: {}", input))
}

/// Parse space-separated items
fn parse_items(input: &str) -> Result<Vec<LispExpr>, String> {
    let mut items = Vec::new();
    let mut current = String::new();
    let mut depth = 0;
    let mut in_string = false;
    let mut escape = false;

    for c in input.chars() {
        if escape {
            current.push(c);
            escape = false;
            continue;
        }

        match c {
            '\\' if in_string => {
                current.push(c);
                escape = true;
            }
            '"' => {
                current.push(c);
                in_string = !in_string;
            }
            '(' if !in_string => {
                current.push(c);
                depth += 1;
            }
            ')' if !in_string => {
                current.push(c);
                depth -= 1;
            }
            ' ' | '\t' | '\n' | '\r' if depth == 0 && !in_string => {
                if !current.trim().is_empty() {
                    items.push(parse_lisp(&current)?);
                }
                current.clear();
            }
            _ => {
                current.push(c);
            }
        }
    }

    if !current.trim().is_empty() {
        items.push(parse_lisp(&current)?);
    }

    Ok(items)
}

fn main() {
    println!("Mini Lisp Parser Example");
    println!("========================");
    println!();

    let inputs = [
        "(+ 1 2)",
        "(define x 42)",
        "(lambda (x) (* x x))",
        r#"(display "Hello, World!")"#,
        "(begin (define a 1) (define b 2) (+ a b))",
        r#"
(define test (lambda ()
  (begin
    (display "something")
    (display 1)
    (display 3.08))))
"#,
    ];

    for input in inputs {
        println!("Input: {}", input.trim().replace('\n', " "));
        match parse_lisp(input) {
            Ok(expr) => {
                println!("  Parsed: {}", expr);
                println!("  Debug: {:?}", expr);
            }
            Err(e) => {
                println!("  Error: {}", e);
            }
        }
        println!();
    }

    println!("---");
    println!("This example demonstrates parsing S-expressions with:");
    println!("* Nested lists: (f (g x) y)");
    println!("* Atoms: identifiers, integers, floats, strings");
    println!("* Recursive structure: lists can contain lists");
}
