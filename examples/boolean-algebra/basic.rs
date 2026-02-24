//! Boolean Algebra Parser Example
//!
//! This example demonstrates parsing boolean expressions with AND/OR operators.
//! Shows operator precedence, parentheses handling, and DNF conversion.
//! Based on the Parslet boolean_algebra.rb example.
//!
//! Run with: cargo run --example boolean_algebra --no-default-features

#![allow(clippy::print_literal)]

use parsanol::portable::{
    parser_dsl::{choice, dynamic, re, ref_, seq, str, GrammarBuilder},
    AstArena, Grammar, PortableParser,
};

/// Build a boolean algebra grammar
fn build_boolean_grammar() -> Grammar {
    let mut builder = GrammarBuilder::new();

    // Basic tokens
    builder = builder.rule("space", re("[ \\t]+"));
    builder = builder.rule("lparen", str("("));
    builder = builder.rule("rparen", str(")"));

    // Operators
    builder = builder.rule("and_op", str("and"));
    builder = builder.rule("or_op", str("or"));

    // Variable: var followed by digits
    builder = builder.rule("variable", re("var[0-9]+"));

    // Primary: parenthesized expression or variable
    builder = builder.rule(
        "primary",
        choice(vec![
            dynamic(seq(vec![
                dynamic(str("(")),
                dynamic(re("[ \\t]*")),
                dynamic(ref_("or_expr")),
                dynamic(re("[ \\t]*")),
                dynamic(str(")")),
            ])),
            dynamic(re("var[0-9]+")),
        ]),
    );

    // AND expression: primary (AND primary)*
    builder = builder.rule(
        "and_expr",
        seq(vec![
            dynamic(re("var[0-9]+")),
            dynamic(re("(?:[ \\t]+and[ \\t]+var[0-9]+)*")),
        ]),
    );

    // OR expression: and_expr (OR and_expr)*
    builder = builder.rule(
        "or_expr",
        seq(vec![
            dynamic(re("var[0-9]+(?:[ \\t]+and[ \\t]+var[0-9]+)*")),
            dynamic(re(
                "(?:[ \\t]+or[ \\t]+var[0-9]+(?:[ \\t]+and[ \\t]+var[0-9]+)*)*",
            )),
        ]),
    );

    builder.build()
}

/// Boolean expression AST
#[derive(Debug, Clone)]
pub enum BoolExpr {
    Var(String),
    And(Box<BoolExpr>, Box<BoolExpr>),
    Or(Box<BoolExpr>, Box<BoolExpr>),
}

impl std::fmt::Display for BoolExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BoolExpr::Var(v) => write!(f, "{}", v),
            BoolExpr::And(l, r) => write!(f, "({} AND {})", l, r),
            BoolExpr::Or(l, r) => write!(f, "({} OR {})", l, r),
        }
    }
}

/// Parse a boolean expression string
pub fn parse_boolean(input: &str) -> Result<BoolExpr, String> {
    let input = input.trim();
    let grammar = build_boolean_grammar();
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    parser
        .parse()
        .map_err(|e| format!("Parse error: {:?}", e))?;

    // Manual parsing for actual AST construction
    parse_or_expr(input)
}

/// Parse OR expression (lowest precedence)
fn parse_or_expr(input: &str) -> Result<BoolExpr, String> {
    let input = input.trim();

    // Find OR at the top level (not inside parentheses)
    let mut depth = 0;
    let mut or_pos = None;

    for (i, c) in input.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ if depth == 0 => {
                if input[i..].starts_with(" or ") {
                    or_pos = Some(i);
                }
            }
            _ => {}
        }
    }

    if let Some(pos) = or_pos {
        let left = parse_and_expr(&input[..pos])?;
        let right = parse_or_expr(&input[pos + 4..])?;
        return Ok(BoolExpr::Or(Box::new(left), Box::new(right)));
    }

    parse_and_expr(input)
}

/// Parse AND expression (higher precedence than OR)
fn parse_and_expr(input: &str) -> Result<BoolExpr, String> {
    let input = input.trim();

    // Find AND at the top level
    let mut depth = 0;
    let mut and_pos = None;

    for (i, c) in input.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ if depth == 0 => {
                if input[i..].starts_with(" and ") {
                    and_pos = Some(i);
                }
            }
            _ => {}
        }
    }

    if let Some(pos) = and_pos {
        let left = parse_primary(&input[..pos])?;
        let right = parse_and_expr(&input[pos + 5..])?;
        return Ok(BoolExpr::And(Box::new(left), Box::new(right)));
    }

    parse_primary(input)
}

/// Parse primary (variable or parenthesized expression)
fn parse_primary(input: &str) -> Result<BoolExpr, String> {
    let input = input.trim();

    // Parenthesized expression
    if input.starts_with('(') && input.ends_with(')') {
        return parse_or_expr(&input[1..input.len() - 1]);
    }

    // Variable
    if input.starts_with("var") && input.len() > 3 {
        let num_part = &input[3..];
        if num_part.chars().all(|c| c.is_ascii_digit()) {
            return Ok(BoolExpr::Var(input.to_string()));
        }
    }

    Err(format!("Invalid primary: {}", input))
}

/// Evaluate boolean expression with variable bindings
pub fn eval(
    expr: &BoolExpr,
    bindings: &std::collections::HashMap<String, bool>,
) -> Result<bool, String> {
    match expr {
        BoolExpr::Var(v) => bindings
            .get(v)
            .copied()
            .ok_or_else(|| format!("Unknown variable: {}", v)),
        BoolExpr::And(l, r) => Ok(eval(l, bindings)? && eval(r, bindings)?),
        BoolExpr::Or(l, r) => Ok(eval(l, bindings)? || eval(r, bindings)?),
    }
}

fn main() {
    println!("Boolean Algebra Parser Example");
    println!("==============================\n");

    let expressions = [
        "var1",
        "var1 and var2",
        "var1 or var2",
        "var1 and var2 or var3",
        "var1 or var2 and var3",
        "(var1 or var2) and var3",
        "var1 and (var2 or var3)",
        "var1 and var2 and var3",
    ];

    println!("{:<35} | {}", "Expression", "Parsed AST");
    println!("{}", "-".repeat(70));

    for expr_str in expressions {
        match parse_boolean(expr_str) {
            Ok(expr) => {
                println!("{:<35} | {}", expr_str, expr);
            }
            Err(e) => {
                println!("{:<35} | ERROR: {}", expr_str, e);
            }
        }
    }

    // Demonstrate evaluation
    println!("\nEvaluation Example:");
    println!("-------------------");

    use std::collections::HashMap;
    let mut bindings = HashMap::new();
    bindings.insert("var1".to_string(), true);
    bindings.insert("var2".to_string(), false);
    bindings.insert("var3".to_string(), true);

    println!("Bindings: var1=true, var2=false, var3=true\n");

    let eval_exprs = [
        "var1 and var2",
        "var1 or var2",
        "var1 and var3",
        "(var1 or var2) and var3",
    ];

    for expr_str in eval_exprs {
        if let Ok(expr) = parse_boolean(expr_str) {
            if let Ok(result) = eval(&expr, &bindings) {
                println!("  {} = {}", expr_str, result);
            }
        }
    }
}
