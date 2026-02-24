//! JSON Parser Example - Serialized Transform Mode
//!
//! This demonstrates parsing in Rust, then serializing to JSON for FFI
//! transfer to a host language (Ruby, Python, JavaScript, etc.).
//!
//! Run with: cargo run --example json_pattern --no-default-features

use parsanol::portable::{
    parser_dsl::{choice, dynamic, re, str, GrammarBuilder},
    AstArena, AstNode, PortableParser,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Build JSON grammar - SAME as json_transform for fair comparison
fn build_json_grammar() -> parsanol::portable::Grammar {
    GrammarBuilder::new()
        .rule(
            "json",
            choice(vec![
                dynamic(str("true")),
                dynamic(str("false")),
                dynamic(str("null")),
                dynamic(re(r#"-?[0-9]+(\.[0-9]+)?"#)),
                dynamic(re(r#""[^"]*""#)),
            ]),
        )
        .build()
}

/// Serializable JSON value
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<JsonValue>),
    Object(HashMap<String, JsonValue>),
}

/// Serialized Transform Mode: Parse → Convert to JsonValue → Serialize to JSON string
pub fn parse_to_json_string(input: &str) -> Result<String, String> {
    let grammar = build_json_grammar();
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    let ast = parser
        .parse()
        .map_err(|e| format!("Parse error: {:?}", e))?;
    let value = ast_to_json(&ast, &arena, input)?;
    serde_json::to_string(&value).map_err(|e| e.to_string())
}

/// Convert AST to JsonValue
fn ast_to_json(node: &AstNode, arena: &AstArena, input: &str) -> Result<JsonValue, String> {
    match node {
        AstNode::InputRef { offset, length } => {
            let start = *offset as usize;
            let end = (start + *length as usize).min(input.len());
            let s = &input[start..end];
            match s {
                "true" => Ok(JsonValue::Bool(true)),
                "false" => Ok(JsonValue::Bool(false)),
                "null" => Ok(JsonValue::Null),
                _ => {
                    if s.len() >= 2 && s.as_bytes()[0] == b'"' && s.as_bytes()[s.len() - 1] == b'"'
                    {
                        Ok(JsonValue::String(s[1..s.len() - 1].to_string()))
                    } else if let Ok(n) = s.parse::<f64>() {
                        Ok(JsonValue::Number(n))
                    } else {
                        Err(format!("Unknown: {}", s))
                    }
                }
            }
        }
        AstNode::Nil => Ok(JsonValue::Null),
        AstNode::Bool(b) => Ok(JsonValue::Bool(*b)),
        AstNode::Int(n) => Ok(JsonValue::Number(*n as f64)),
        AstNode::Float(f) => Ok(JsonValue::Number(*f)),
        AstNode::StringRef { pool_index } => Ok(JsonValue::String(
            arena.get_string(*pool_index as usize).to_string(),
        )),
        AstNode::Array { pool_index, length } => {
            let items = arena.get_array(*pool_index as usize, *length as usize);
            let values: Result<Vec<JsonValue>, _> = items
                .iter()
                .map(|item| ast_to_json(item, arena, input))
                .collect();
            Ok(JsonValue::Array(values?))
        }
        AstNode::Hash { pool_index, length } => {
            let pairs = arena.get_hash_items(*pool_index as usize, *length as usize);
            let mut map = HashMap::new();
            for (k, v) in pairs {
                map.insert(k.clone(), ast_to_json(&v, arena, input)?);
            }
            Ok(JsonValue::Object(map))
        }
    }
}

fn main() {
    println!("JSON Parser - Serialized Transform Mode (Parse → Serialize for FFI)");
    println!("================================================================\n");

    for input in ["true", "false", "null", "42", "-3.14", r#""hello""#] {
        match parse_to_json_string(input) {
            Ok(json) => println!("{} => {}", input, json),
            Err(e) => println!("{} Error: {}", input, e),
        }
    }
}
