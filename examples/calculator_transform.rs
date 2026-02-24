//! Calculator Example - Native Transform Mode
//!
//! This example demonstrates the "Rust way" - parsing AND transforming
//! all in Rust, with no serialization overhead.
//!
//! IMPORTANT: This uses the SAME grammar as calculator_pattern.rs
//! for fair comparison!
//!
//! Run with: cargo run --example calculator_transform --no-default-features

use parsanol::portable::{
    infix::{Assoc, InfixBuilder},
    parser_dsl::{choice, dynamic, re, ref_, seq, str, GrammarBuilder},
    AstArena, AstNode, Grammar, PortableParser,
};

/// Build a calculator grammar with proper precedence (SAME as calculator_pattern.rs)
fn build_calculator_grammar() -> Grammar {
    let mut builder = GrammarBuilder::new();

    // Add "expr" as first rule so it becomes the root
    builder = builder.rule("expr", re(r"[0-9]+")); // Placeholder, will be replaced

    builder = builder.rule("number", re(r"[0-9]+"));
    builder = builder.rule(
        "primary",
        choice(vec![
            dynamic(seq(vec![
                dynamic(str("(")),
                dynamic(ref_("expr")),
                dynamic(str(")")),
            ])),
            dynamic(ref_("number")),
        ]),
    );

    // Build infix with precedence - same as calculator_pattern.rs
    let expr_atom = InfixBuilder::new()
        .primary(ref_("primary"))
        .op("*", 2, Assoc::Left)
        .op("/", 2, Assoc::Left)
        .op("+", 1, Assoc::Left)
        .op("-", 1, Assoc::Left)
        .build(&mut builder);

    // Update the "expr" rule to point to the infix expression
    builder.update_rule("expr", expr_atom);

    builder.build()
}

/// Expression type (native Rust, no serialization needed)
pub enum Expr {
    Number(i64),
    BinOp {
        left: Box<Expr>,
        op: BinOp,
        right: Box<Expr>,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
}

/// Native Transform Mode: Parse + Transform in Rust → Return native Expr
/// No serialization overhead - direct return
pub fn parse_and_transform(input: &str) -> Result<Expr, String> {
    let grammar = build_calculator_grammar();
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    let ast = parser
        .parse()
        .map_err(|e| format!("Parse error: {:?}", e))?;
    ast_to_expr(&ast, &arena, input)
}

/// Evaluate expression (all in Rust, no FFI)
pub fn evaluate(input: &str) -> Result<i64, String> {
    let expr = parse_and_transform(input)?;
    eval_expr(&expr)
}

/// Evaluate Expr to i64
fn eval_expr(expr: &Expr) -> Result<i64, String> {
    match expr {
        Expr::Number(n) => Ok(*n),
        Expr::BinOp { left, op, right } => {
            let l = eval_expr(left)?;
            let r = eval_expr(right)?;
            match op {
                BinOp::Add => Ok(l + r),
                BinOp::Sub => Ok(l - r),
                BinOp::Mul => Ok(l * r),
                BinOp::Div => Ok(l / r),
            }
        }
    }
}

/// Convert AST to native Expr (no serialization)
fn ast_to_expr(node: &AstNode, arena: &AstArena, input: &str) -> Result<Expr, String> {
    match node {
        AstNode::InputRef { offset, length } => {
            let text = &input[*offset as usize..*offset as usize + *length as usize];
            // Skip operators and parentheses - they're handled in Array case
            if text == "(" || text == ")" || ["+", "-", "*", "/"].contains(&text) {
                return Err(format!("Unexpected operator/paren in InputRef: {}", text));
            }
            let n = text
                .parse::<i64>()
                .map_err(|_| format!("Failed to parse: {}", text))?;
            Ok(Expr::Number(n))
        }
        AstNode::Array { pool_index, length } => {
            let items = arena.get_array(*pool_index as usize, *length as usize);

            // Check for parenthesized expression: ( expr )
            // AST would be: [ "(", expr, ")" ]
            if items.len() == 3
                && is_paren(&items[0], arena, input, "(")
                && is_paren(&items[2], arena, input, ")")
            {
                // It's a parenthesized expression - just return the inner expr
                return ast_to_expr(&items[1], arena, input);
            }

            // Infix grammar produces: operand (op operand)*
            if items.len() == 2 {
                let mut result = ast_to_expr(&items[0], arena, input)?;

                if let &AstNode::Array {
                    pool_index: rep_pool,
                    length: rep_len,
                } = &items[1]
                {
                    let rep_items = arena.get_array(rep_pool as usize, rep_len as usize);
                    for pair in &rep_items {
                        if let AstNode::Array {
                            pool_index: pair_pool,
                            length: pair_len,
                        } = pair
                        {
                            let pair_items =
                                arena.get_array(*pair_pool as usize, *pair_len as usize);
                            if pair_items.len() == 2 {
                                let op = parse_op(&pair_items[0], arena, input)?;
                                let right = ast_to_expr(&pair_items[1], arena, input)?;
                                result = Expr::BinOp {
                                    left: Box::new(result),
                                    op,
                                    right: Box::new(right),
                                };
                            }
                        }
                    }
                }

                return Ok(result);
            }

            // Fallback for simple cases
            match items.len() {
                0 => Err("Empty expression".into()),
                1 => ast_to_expr(&items[0], arena, input),
                3 => {
                    // Could be (expr) or left op right
                    if is_paren(&items[0], arena, input, "(") {
                        // Parenthesized expression
                        return ast_to_expr(&items[1], arena, input);
                    }
                    let left = ast_to_expr(&items[0], arena, input)?;
                    let op = parse_op(&items[1], arena, input)?;
                    let right = ast_to_expr(&items[2], arena, input)?;
                    Ok(Expr::BinOp {
                        left: Box::new(left),
                        op,
                        right: Box::new(right),
                    })
                }
                _ => ast_to_expr(&items[0], arena, input),
            }
        }
        _ => Err(format!("Unexpected node: {:?}", node)),
    }
}

/// Check if a node is a specific parenthesis character
fn is_paren(node: &AstNode, arena: &AstArena, input: &str, paren: &str) -> bool {
    match node {
        AstNode::InputRef { offset, length } => {
            &input[*offset as usize..*offset as usize + *length as usize] == paren
        }
        AstNode::StringRef { pool_index } => arena.get_string(*pool_index as usize) == paren,
        _ => false,
    }
}

fn parse_op(node: &AstNode, arena: &AstArena, input: &str) -> Result<BinOp, String> {
    let text = match node {
        AstNode::InputRef { offset, length } => {
            &input[*offset as usize..*offset as usize + *length as usize]
        }
        AstNode::StringRef { pool_index } => arena.get_string(*pool_index as usize),
        _ => return Err("Not an operator".into()),
    };

    match text {
        "+" => Ok(BinOp::Add),
        "-" => Ok(BinOp::Sub),
        "*" => Ok(BinOp::Mul),
        "/" => Ok(BinOp::Div),
        _ => Err(format!("Unknown operator: {}", text)),
    }
}

fn main() {
    println!("Calculator - Native Transform Mode (Parse + Transform in Rust)");
    println!("============================================================\n");

    let examples = vec![
        ("42", 42),
        ("1+2", 3),
        ("1+2*3", 7),   // With precedence: 1 + (2 * 3) = 7
        ("2*3+1", 7),   // With precedence: (2 * 3) + 1 = 7
        ("(1+2)*3", 9), // Parentheses: (1 + 2) * 3 = 9
    ];

    for (input, expected) in examples {
        match evaluate(input) {
            Ok(r) => println!("{} = {} (expected {})", input, r, expected),
            Err(e) => println!("{} Error: {}", input, e),
        }
    }

    println!("\nNOTE: All parsing and transformation happened in Rust,");
    println!("with no serialization/FFI overhead (fast!).");
}
