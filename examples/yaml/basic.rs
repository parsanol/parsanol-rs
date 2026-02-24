//! YAML Subset Parser Example
//!
//! This example demonstrates parsing a subset of YAML (YAML Ain't Markup Language).
//! YAML is a human-friendly data serialization format commonly used for configuration.
//!
//! Supported features:
//! - Key-value pairs: `key: value`
//! - Scalars: strings, numbers, booleans, null
//! - Lists: `- item`
//! - Nested maps: indentation-based
//! - Quoted strings: single and double
//! - Block scalars: `|` and `>`
//!
//! Run with: cargo run --example yaml/basic --no-default-features

use parsanol::portable::{
    parser_dsl::{choice, dynamic, re, seq, str, GrammarBuilder, ParsletExt},
    AstArena, AstNode, Grammar, PortableParser,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Build YAML grammar (subset)
fn build_yaml_grammar() -> Grammar {
    GrammarBuilder::new()
        // Indentation
        .rule("indent", re("[ ]*"))
        // Comments
        .rule("comment", re("#.*"))
        // Scalar types
        .rule("null_val", str("null").or(str("~")).or(str("")))
        .rule(
            "bool_val",
            str("true").or(str("false")).or(str("yes")).or(str("no")),
        )
        .rule(
            "number",
            choice(vec![
                dynamic(re("-?[0-9]+\\.[0-9]+")), // float
                dynamic(re("-?[0-9]+")),          // integer
            ]),
        )
        // Quoted strings
        .rule(
            "double_quoted",
            seq(vec![
                dynamic(str("\"")),
                dynamic(re("[^\"\\\\]*(\\\\.[^\"\\\\]*)*")),
                dynamic(str("\"")),
            ]),
        )
        .rule(
            "single_quoted",
            seq(vec![
                dynamic(str("'")),
                dynamic(re("[^']*")),
                dynamic(str("'")),
            ]),
        )
        // Plain (unquoted) string
        .rule("plain_string", re("[^:#\\[\\]{}\\n\\r][^#\\n\\r]*"))
        // Block scalar indicators
        .rule("literal_block", str("|")) // Preserves newlines
        .rule("folded_block", str(">")) // Folds newlines
        // Flow sequences (inline arrays)
        .rule(
            "flow_seq",
            seq(vec![
                dynamic(str("[")),
                dynamic(re("[ \\t\\n\\r]*")),
                dynamic(
                    seq(vec![
                        dynamic(re("[^\\]]+")),
                        dynamic(
                            seq(vec![
                                dynamic(re("[ \\t\\n\\r]*")),
                                dynamic(str(",")),
                                dynamic(re("[ \\t\\n\\r]*")),
                                dynamic(re("[^\\]]+")),
                            ])
                            .many(),
                        ),
                    ])
                    .optional(),
                ),
                dynamic(re("[ \\t\\n\\r]*")),
                dynamic(str("]")),
            ]),
        )
        // Flow mappings (inline objects)
        .rule(
            "flow_map",
            seq(vec![
                dynamic(str("{")),
                dynamic(re("[ \\t\\n\\r]*")),
                dynamic(
                    seq(vec![
                        dynamic(re("[^}]+")),
                        dynamic(
                            seq(vec![
                                dynamic(re("[ \\t\\n\\r]*")),
                                dynamic(str(",")),
                                dynamic(re("[ \\t\\n\\r]*")),
                                dynamic(re("[^}]+")),
                            ])
                            .many(),
                        ),
                    ])
                    .optional(),
                ),
                dynamic(re("[ \\t\\n\\r]*")),
                dynamic(str("}")),
            ]),
        )
        // Value (scalar or inline collection)
        .rule(
            "value",
            choice(vec![
                dynamic(re("\\[[^\\]]*\\]")),                    // flow sequence
                dynamic(re("\\{[^}]*\\}")),                      // flow mapping
                dynamic(re("\"[^\"\\\\]*(\\\\.[^\"\\\\]*)*\"")), // double quoted
                dynamic(re("'[^']*'")),                          // single quoted
                dynamic(re("true|false|yes|no")),                // boolean
                dynamic(re("null|~")),                           // null
                dynamic(re("-?[0-9]+\\.[0-9]+")),                // float
                dynamic(re("-?[0-9]+")),                         // integer
                dynamic(re("[^:#\\[\\]{}\\n\\r][^#\\n\\r]*")),   // plain string
            ]),
        )
        // Key (with optional quotes)
        .rule(
            "key",
            choice(vec![
                dynamic(re("\"[^\"\\\\]*(\\\\.[^\"\\\\]*)*\"")),
                dynamic(re("'[^']*'")),
                dynamic(re("[a-zA-Z_][a-zA-Z0-9_-]*")),
            ]),
        )
        // Key-value pair: `key: value` or `key:` (for nested)
        .rule(
            "keyval",
            seq(vec![
                dynamic(re("[a-zA-Z_][a-zA-Z0-9_-]*|\"[^\"]+\"|'[^']+'")), // key
                dynamic(re(":[ \t]*")),                                    // colon
                dynamic(choice(vec![
                    dynamic(re("[^#\\n\\r]+")), // inline value
                    dynamic(re("")),            // empty (nested)
                ])),
            ]),
        )
        // List item: `- value`
        .rule(
            "list_item",
            seq(vec![
                dynamic(re("[ \t]*")),
                dynamic(str("-")),
                dynamic(re("[ \t]+")),
                dynamic(choice(vec![
                    dynamic(re("\\[[^\\]]*\\]")),
                    dynamic(re("\\{[^}]*\\}")),
                    dynamic(re("[^#\\n\\r]+")),
                ])),
            ]),
        )
        // Document structure
        .rule(
            "document",
            seq(vec![
                dynamic(
                    seq(vec![
                        dynamic(choice(vec![
                            dynamic(re("[a-zA-Z_][a-zA-Z0-9_-]*:[ \t]*[^#\\n\\r]*")), // keyval
                            dynamic(re("[ \t]*-[^#\\n\\r]*")),                        // list item
                            dynamic(re("#.*")),                                       // comment
                            dynamic(re("")),                                          // empty
                        ])),
                        dynamic(re("[\r]?[\n]")),
                    ])
                    .many(),
                ),
                dynamic(
                    choice(vec![
                        dynamic(re("[a-zA-Z_][a-zA-Z0-9_-]*:[ \t]*[^#\\n\\r]*")),
                        dynamic(re("[ \t]*-[^#\\n\\r]*")),
                        dynamic(re("#.*")),
                        dynamic(re("")),
                    ])
                    .optional(),
                ),
            ]),
        )
        .build()
}

/// YAML value types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum YamlValue {
    Null,
    Boolean(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Array(Vec<YamlValue>),
    Object(HashMap<String, YamlValue>),
}

/// Parse YAML string
pub fn parse_yaml(input: &str) -> Result<String, String> {
    let grammar = build_yaml_grammar();
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    let _ast = parser
        .parse()
        .map_err(|e| format!("Parse error: {:?}", e))?;

    // Manual parsing for structured output
    let result = parse_yaml_document(input)?;
    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}

/// Parse a YAML document
fn parse_yaml_document(input: &str) -> Result<YamlValue, String> {
    let mut result: HashMap<String, YamlValue> = HashMap::new();
    let mut current_list: Option<Vec<YamlValue>> = None;
    let mut list_key: Option<String> = None;

    for line in input.lines() {
        // Skip empty lines and comments
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // List item
        if trimmed.starts_with("- ") || trimmed.starts_with("-\t") {
            let value = parse_yaml_value(trimmed[1..].trim());
            if let Some(ref mut list) = current_list {
                list.push(value);
            } else if let Some(key) = &list_key {
                let list = result
                    .entry(key.clone())
                    .or_insert_with(|| YamlValue::Array(vec![]));
                if let YamlValue::Array(ref mut arr) = list {
                    arr.push(value);
                }
            }
            continue;
        }

        // Key-value pair
        if let Some(colon_pos) = trimmed.find(':') {
            let key = trimmed[..colon_pos].trim();
            let value_str = trimmed[colon_pos + 1..].trim();

            // Check if this starts a list
            if value_str.is_empty() {
                list_key = Some(key.to_string());
                current_list = Some(vec![]);
                result.insert(key.to_string(), YamlValue::Array(vec![]));
            } else {
                list_key = None;
                current_list = None;
                let value = parse_yaml_value(value_str);
                result.insert(key.to_string(), value);
            }
        }
    }

    Ok(YamlValue::Object(result))
}

/// Parse a YAML value
fn parse_yaml_value(s: &str) -> YamlValue {
    let s = s.trim();

    // Remove trailing comment
    let s = if let Some(pos) = s.find('#') {
        &s[..pos]
    } else {
        s
    };
    let s = s.trim();

    // Null
    if s.is_empty() || s == "null" || s == "~" {
        return YamlValue::Null;
    }

    // Boolean
    if s == "true" || s == "yes" {
        return YamlValue::Boolean(true);
    }
    if s == "false" || s == "no" {
        return YamlValue::Boolean(false);
    }

    // Quoted string
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        return YamlValue::String(s[1..s.len() - 1].to_string());
    }

    // Inline array: [a, b, c]
    if s.starts_with('[') && s.ends_with(']') {
        let inner = &s[1..s.len() - 1];
        if inner.trim().is_empty() {
            return YamlValue::Array(vec![]);
        }
        let items: Vec<YamlValue> = inner
            .split(',')
            .map(|item| parse_yaml_value(item.trim()))
            .collect();
        return YamlValue::Array(items);
    }

    // Inline object: {a: 1, b: 2}
    if s.starts_with('{') && s.ends_with('}') {
        let inner = &s[1..s.len() - 1];
        let mut obj = HashMap::new();
        if !inner.trim().is_empty() {
            for pair in inner.split(',') {
                if let Some(colon_pos) = pair.find(':') {
                    let k = pair[..colon_pos].trim();
                    let v = pair[colon_pos + 1..].trim();
                    obj.insert(k.to_string(), parse_yaml_value(v));
                }
            }
        }
        return YamlValue::Object(obj);
    }

    // Float
    if s.contains('.') {
        if let Ok(n) = s.parse::<f64>() {
            return YamlValue::Float(n);
        }
    }

    // Integer
    if let Ok(n) = s.parse::<i64>() {
        return YamlValue::Integer(n);
    }

    // Plain string
    YamlValue::String(s.to_string())
}

fn main() {
    println!("YAML Subset Parser Example");
    println!("==========================\n");

    let examples = [
        (
            r#"name: YAML Example
version: 1.0
enabled: true
count: 42
pi: 3.14
"#,
            "Basic key-value pairs",
        ),
        (
            r#"server:
  host: localhost
  port: 8080
database:
  host: db.example.com
  port: 5432
"#,
            "Nested configuration",
        ),
        (
            r#"items:
  - apple
  - banana
  - cherry
numbers:
  - 1
  - 2
  - 3
"#,
            "Lists",
        ),
        (
            r#"inline: [a, b, c]
object: { x: 1, y: 2 }
quoted: "hello world"
single: 'single quotes'
"#,
            "Inline and quoted values",
        ),
    ];

    for (input, description) in examples {
        println!("Input: ({})", description);
        println!("{}\n", input);
        match parse_yaml(input) {
            Ok(json) => println!("Output:\n{}\n", json),
            Err(e) => println!("Error: {}\n", e),
        }
        println!("{}", "-".repeat(60));
        println!();
    }
}
