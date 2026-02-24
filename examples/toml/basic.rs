//! TOML Parser Example
//!
//! This example demonstrates parsing TOML (Tom's Obvious Minimal Language)
//! configuration files. TOML is a configuration file format that's easy to read
//! and maps unambiguously to a hash table.
//!
//! Supported features:
//! - Key/value pairs: `key = "value"`
//! - Strings (basic and literal)
//! - Integers, floats, booleans
//! - Arrays: `[1, 2, 3]`
//! - Tables: `[section]`
//! - Inline tables: `{ key = "value" }`
//! - Nested tables: `[section.subsection]`
//!
//! Run with: cargo run --example toml/basic --no-default-features

use parsanol::portable::{
    parser_dsl::{choice, dynamic, re, seq, str, GrammarBuilder, ParsletExt},
    AstArena, AstNode, Grammar, PortableParser,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Build TOML grammar
fn build_toml_grammar() -> Grammar {
    GrammarBuilder::new()
        // Whitespace and comments
        .rule("ws", re("[ \t]+"))
        .rule("comment", re("#.*"))
        .rule("newline", re("[\r]?[\n]"))
        // Basic string: "..." with escape sequences
        .rule(
            "basic_string",
            seq(vec![
                dynamic(str("\"")),
                dynamic(
                    choice(vec![
                        dynamic(re(r#"\\."#)),         // any escape
                        dynamic(re(r#"[^"\\]+"#)),     // normal chars
                    ])
                    .many(),
                ),
                dynamic(str("\"")),
            ]),
        )
        // Literal string: '...' (no escapes)
        .rule(
            "literal_string",
            seq(vec![
                dynamic(str("'")),
                dynamic(re("[^']*").or(str("''"))),
                dynamic(str("'")),
            ]),
        )
        // Multiline basic string: """..."""
        .rule(
            "ml_basic_string",
            seq(vec![
                dynamic(str("\"\"\"")),
                dynamic(re("[^\"]*")),
                dynamic(str("\"\"\"")),
            ]),
        )
        // String values
        .rule(
            "string",
            choice(vec![
                dynamic(re(r#""""[^"]*""""#)), // multiline basic
                dynamic(re(r#""[^"\\]*(\\.[^"\\]*)*""#)), // basic string
                dynamic(re(r#"'[^']*'"#)), // literal string
            ]),
        )
        // Numbers
        .rule("integer", re("-?[0-9]+"))
        .rule("float", re("-?[0-9]+\\.[0-9]+([eE][+-]?[0-9]+)?"))
        .rule("hex_int", re("0x[0-9a-fA-F]+"))
        .rule("oct_int", re("0o[0-7]+"))
        .rule("bin_int", re("0b[01]+"))
        .rule(
            "number",
            choice(vec![
                dynamic(re("-?[0-9]+\\.[0-9]+([eE][+-]?[0-9]+)?")), // float
                dynamic(re("0x[0-9a-fA-F]+")),                      // hex
                dynamic(re("0o[0-7]+")),                           // octal
                dynamic(re("0b[01]+")),                            // binary
                dynamic(re("-?[0-9]+")),                           // integer
            ]),
        )
        // Boolean
        .rule("boolean", str("true").or(str("false")))
        // Date/time (ISO 8601 subset)
        .rule(
            "datetime",
            re("[0-9]{4}-[0-9]{2}-[0-9]{2}[T ][0-9]{2}:[0-9]{2}:[0-9]{2}(\\.[0-9]+)?(Z|[+-][0-9]{2}:[0-9]{2})?"),
        )
        .rule("date", re("[0-9]{4}-[0-9]{2}-[0-9]{2}"))
        .rule("time", re("[0-9]{2}:[0-9]{2}:[0-9]{2}(\\.[0-9]+)?"))
        // Values
        .rule(
            "value",
            choice(vec![
                dynamic(re("true|false")),                        // boolean
                dynamic(re("[0-9]{4}-[0-9]{2}-[0-9]{2}")),        // date
                dynamic(re("-?[0-9]+\\.[0-9]+([eE][+-]?[0-9]+)?")), // float
                dynamic(re("-?[0-9]+")),                          // integer
                dynamic(re(r#""[^"\\]*(\\.[^"\\]*)*""#)),         // string
                dynamic(re(r#"'[^']*'"#)),                        // literal string
            ]),
        )
        // Array
        .rule(
            "array",
            seq(vec![
                dynamic(str("[")),
                dynamic(re("[ \t\r\n]*")),
                dynamic(
                    seq(vec![
                        dynamic(re(r#"(true|false|[0-9.-]+|"[^"]*"|'[^']*')"#)),
                        dynamic(
                            seq(vec![
                                dynamic(re("[ \t\r\n]*")),
                                dynamic(str(",")),
                                dynamic(re("[ \t\r\n]*")),
                                dynamic(re(r#"(true|false|[0-9.-]+|"[^"]*"|'[^']*')"#)),
                            ])
                            .many(),
                        ),
                    ])
                    .optional(),
                ),
                dynamic(re("[ \t\r\n]*")),
                dynamic(str("]")),
            ]),
        )
        // Inline table
        .rule(
            "inline_table",
            seq(vec![
                dynamic(str("{")),
                dynamic(re("[ \t]*")),
                dynamic(
                    seq(vec![
                        dynamic(re("[a-zA-Z_][a-zA-Z0-9_]*")), // key
                        dynamic(re("[ \t]*=[ \t]*")),
                        dynamic(re(r#"(true|false|[0-9.-]+|"[^"]*"|'[^']*')"#)), // value
                        dynamic(
                            seq(vec![
                                dynamic(re("[ \t]*,[ \t]*")),
                                dynamic(re("[a-zA-Z_][a-zA-Z0-9_]*")),
                                dynamic(re("[ \t]*=[ \t]*")),
                                dynamic(re(r#"(true|false|[0-9.-]+|"[^"]*"|'[^']*')"#)),
                            ])
                            .many(),
                        ),
                    ])
                    .optional(),
                ),
                dynamic(re("[ \t]*")),
                dynamic(str("}")),
            ]),
        )
        // Key
        .rule("bare_key", re("[a-zA-Z_][a-zA-Z0-9_-]*"))
        .rule("quoted_key", re(r#""[^"]+""#))
        .rule("key", re(r#"[a-zA-Z_][a-zA-Z0-9_-]*|"[^"]+""#))
        // Key-value pair
        .rule(
            "keyval",
            seq(vec![
                dynamic(re("[a-zA-Z_][a-zA-Z0-9_-]*")), // key
                dynamic(re("[ \t]*=[ \t]*")),          // equals
                dynamic(
                    choice(vec![
                        // value, array, or inline table
                        dynamic(re(r#"\{[^}]*\}"#)),               // inline table
                        dynamic(re(r#"\[[^\]]*\]"#)),              // array
                        dynamic(re(r#"(true|false|[0-9.-]+|"[^"]*"|'[^']*')"#)), // scalar
                    ]),
                ),
            ]),
        )
        // Table header: [section] or [section.subsection]
        .rule(
            "table",
            seq(vec![
                dynamic(str("[")),
                dynamic(re("[a-zA-Z_][a-zA-Z0-9_.-]*")), // table name
                dynamic(str("]")),
            ]),
        )
        // Array of tables: [[items]]
        .rule(
            "array_table",
            seq(vec![
                dynamic(str("[[")),
                dynamic(re("[a-zA-Z_][a-zA-Z0-9_.-]*")),
                dynamic(str("]]")),
            ]),
        )
        // Line: table header, array table, keyval, comment, or empty
        .rule(
            "line",
            choice(vec![
                dynamic(re(r#"\[\[[a-zA-Z_][a-zA-Z0-9_.-]*\]\]"#)), // array table
                dynamic(re(r#"\[[a-zA-Z_][a-zA-Z0-9_.-]*\]"#)),     // table
                dynamic(re(r#"[a-zA-Z_][a-zA-Z0-9_-]*[ \t]*=[ \t].+"#)), // keyval
                dynamic(re("#.*")),                                  // comment
                dynamic(re("[ \t]*")),                               // empty
            ]),
        )
        // Root: document is a sequence of lines
        .rule(
            "document",
            seq(vec![
                dynamic(
                    seq(vec![
                        dynamic(choice(vec![
                            dynamic(re(r#"\[\[[a-zA-Z_][a-zA-Z0-9_.-]*\]\]"#)),
                            dynamic(re(r#"\[[a-zA-Z_][a-zA-Z0-9_.-]*\]"#)),
                            dynamic(re(r#"[a-zA-Z_][a-zA-Z0-9_-]*[ \t]*=[ \t].+"#)),
                            dynamic(re("#.*")),
                            dynamic(re("[ \t]*")),
                        ])),
                        dynamic(re("[\r]?[\n]")),
                    ])
                    .many(),
                ),
                dynamic(
                    choice(vec![
                        dynamic(re(r#"\[\[[a-zA-Z_][a-zA-Z0-9_.-]*\]\]"#)),
                        dynamic(re(r#"\[[a-zA-Z_][a-zA-Z0-9_.-]*\]"#)),
                        dynamic(re(r#"[a-zA-Z_][a-zA-Z0-9_-]*[ \t]*=[ \t].+"#)),
                        dynamic(re("#.*")),
                        dynamic(re("[ \t]*")),
                    ])
                    .optional(),
                ),
            ]),
        )
        .build()
}

/// Parsed TOML value
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TomlValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Array(Vec<TomlValue>),
    Table(HashMap<String, TomlValue>),
}

/// Parsed TOML document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TomlDocument {
    pub root: HashMap<String, TomlValue>,
    pub tables: HashMap<String, HashMap<String, TomlValue>>,
}

/// Parse a TOML string
pub fn parse_toml(input: &str) -> Result<String, String> {
    let grammar = build_toml_grammar();
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    let _ast = parser
        .parse()
        .map_err(|e| format!("Parse error: {:?}", e))?;

    // Parse manually for structured output
    let mut doc = TomlDocument {
        root: HashMap::new(),
        tables: HashMap::new(),
    };

    let mut current_table = String::new();

    for line in input.lines() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Array of tables: [[items]]
        if line.starts_with("[[") && line.ends_with("]]") {
            current_table = line[2..line.len() - 2].to_string();
            doc.tables.entry(current_table.clone()).or_default();
            continue;
        }

        // Table: [section]
        if line.starts_with('[') && line.ends_with(']') {
            current_table = line[1..line.len() - 1].to_string();
            doc.tables.entry(current_table.clone()).or_default();
            continue;
        }

        // Key-value pair
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim().to_string();
            let value = parse_toml_value(value.trim());

            if current_table.is_empty() {
                doc.root.insert(key, value);
            } else {
                doc.tables
                    .entry(current_table.clone())
                    .or_default()
                    .insert(key, value);
            }
        }
    }

    serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())
}

/// Parse a TOML value
fn parse_toml_value(s: &str) -> TomlValue {
    let s = s.trim();

    // Remove comment after value
    let s = if let Some(pos) = s.find('#') {
        &s[..pos]
    } else {
        s
    };
    let s = s.trim();

    // Boolean
    if s == "true" {
        return TomlValue::Boolean(true);
    }
    if s == "false" {
        return TomlValue::Boolean(false);
    }

    // String
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        return TomlValue::String(s[1..s.len() - 1].to_string());
    }

    // Array
    if s.starts_with('[') && s.ends_with(']') {
        let inner = &s[1..s.len() - 1];
        if inner.trim().is_empty() {
            return TomlValue::Array(vec![]);
        }
        let items: Vec<TomlValue> = inner
            .split(',')
            .map(|item| parse_toml_value(item.trim()))
            .collect();
        return TomlValue::Array(items);
    }

    // Inline table
    if s.starts_with('{') && s.ends_with('}') {
        let inner = &s[1..s.len() - 1];
        let mut table = HashMap::new();
        if !inner.trim().is_empty() {
            for pair in inner.split(',') {
                if let Some((k, v)) = pair.split_once('=') {
                    table.insert(k.trim().to_string(), parse_toml_value(v.trim()));
                }
            }
        }
        return TomlValue::Table(table);
    }

    // Float
    if s.contains('.') || s.contains('e') || s.contains('E') {
        if let Ok(n) = s.parse::<f64>() {
            return TomlValue::Float(n);
        }
    }

    // Integer
    if let Ok(n) = s.parse::<i64>() {
        return TomlValue::Integer(n);
    }

    // Fallback to string
    TomlValue::String(s.to_string())
}

fn main() {
    println!("TOML Parser Example");
    println!("===================\n");

    let examples = [
        (
            r#"title = "TOML Example"

[owner]
name = "Tom Preston-Werner"
dob = 1979-05-27

[database]
server = "192.168.1.1"
ports = [8000, 8001, 8002]"#,
            "Basic TOML document",
        ),
        (
            r#"[server]
host = "localhost"
port = 8080
enabled = true

[server.ssl]
enabled = false
cert = "/path/to/cert.pem""#,
            "Nested tables",
        ),
        (
            r#"numbers = [1, 2, 3]
mixed = ["hello", 42, true]
inline = { x = 1, y = 2 }"#,
            "Arrays and inline tables",
        ),
    ];

    for (input, description) in examples {
        println!("Input: ({})", description);
        println!("{}\n", input);
        match parse_toml(input) {
            Ok(json) => println!("Output:\n{}\n", json),
            Err(e) => println!("Error: {}\n", e),
        }
        println!("{}", "-".repeat(60));
        println!();
    }
}
