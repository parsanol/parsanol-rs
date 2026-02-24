//! Calculator Example - Serialized Transform Mode
//!
//! This example demonstrates parsing in Rust, then serializing the result to
//! JSON for FFI transfer to a host language. The host language would then
//! deserialize and process the result.
//!
//! IMPORTANT: This uses the SAME grammar as calculator_transform.rs
//! for fair comparison!
//!
//! Run with: cargo run --example calculator_pattern --no-default-features

use parsanol::portable::{
    infix::{Assoc, InfixBuilder},
    parser_dsl::{choice, dynamic, re, ref_, seq, str, GrammarBuilder},
    AstArena, AstNode, Grammar, PortableParser,
};
use serde::{Deserialize, Serialize};

/// Build a calculator grammar with proper precedence (SAME as calculator_transform.rs)
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

    // Build infix with precedence - same as calculator_transform.rs
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

/// Serializable expression tree for FFI transfer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Expr {
    Number(i64),
    BinOp {
        left: Box<Expr>,
        op: String,
        right: Box<Expr>,
    },
}

/// Serialized Transform Mode: Parse → Convert to Expr → Serialize to JSON
/// This simulates returning data to host language via FFI (serialization required)
pub fn parse_to_json(input: &str) -> Result<String, String> {
    let grammar = build_calculator_grammar();
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    let ast = parser
        .parse()
        .map_err(|e| format!("Parse error: {:?}", e))?;
    let expr = ast_to_expr(&ast, &arena, input)?;
    serde_json::to_string(&expr).map_err(|e| e.to_string())
}

/// Convert AST to serializable Expr
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
            if items.len() == 3
                && is_paren(&items[0], arena, input, "(")
                && is_paren(&items[2], arena, input, ")")
            {
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
                                let op = extract_text(&pair_items[0], arena, input)?;
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
                        return ast_to_expr(&items[1], arena, input);
                    }
                    let left = ast_to_expr(&items[0], arena, input)?;
                    let op = extract_text(&items[1], arena, input)?;
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

fn extract_text(node: &AstNode, arena: &AstArena, input: &str) -> Result<String, String> {
    match node {
        AstNode::InputRef { offset, length } => {
            Ok(input[*offset as usize..*offset as usize + *length as usize].to_string())
        }
        AstNode::StringRef { pool_index } => Ok(arena.get_string(*pool_index as usize).to_string()),
        _ => Err("Not text".into()),
    }
}

fn main() {
    println!("Calculator - Serialized Transform Mode (Parse → Serialize for FFI)");
    println!("================================================================\n");

    let examples = vec![
        ("42", 42),
        ("1+2", 3),
        ("1+2*3", 7),   // With precedence: 1 + (2 * 3) = 7
        ("(1+2)*3", 9), // Parentheses: (1 + 2) * 3 = 9
    ];

    for (input, _expected) in examples {
        match parse_to_json(input) {
            Ok(json) => println!("{} => {}", input, json),
            Err(e) => println!("{} Error: {}", input, e),
        }
    }

    println!("\nNOTE: In Serialized mode, this JSON is sent via FFI to the host");
    println!("language, which deserializes and processes it.");
}
