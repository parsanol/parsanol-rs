//! INI Configuration Parser Example
//!
//! This example demonstrates parsing INI-style configuration files.
//! Shows handling of sections, key-value pairs, and comments.
//!
//! Run with: cargo run --example ini_parser --no-default-features

use parsanol::portable::{
    parser_dsl::{choice, dynamic, re, seq, str, GrammarBuilder},
    AstArena, Grammar, PortableParser,
};
use std::collections::HashMap;

/// Build an INI grammar
fn build_ini_grammar() -> Grammar {
    GrammarBuilder::new()
        // Whitespace
        .rule("space", re("[ \t]+"))
        // Comment: ; or # to end of line
        .rule("comment", re("[;#][^\\n]*"))
        // Key: alphanumeric with underscores
        .rule("key", re("[a-zA-Z_][a-zA-Z0-9_]*"))
        // Value: anything until end of line (trimmed)
        .rule("value", re("[^\\n]+"))
        // Key-value pair: key = value
        .rule(
            "pair",
            seq(vec![
                dynamic(re("[a-zA-Z_][a-zA-Z0-9_]*")), // key
                dynamic(re("[ \t]*=[ \t]*")),          // =
                dynamic(re("[^\\n]+")),                // value
            ]),
        )
        // Section: [name]
        .rule("section_name", re("[a-zA-Z_][a-zA-Z0-9_.]*"))
        .rule(
            "section",
            seq(vec![
                dynamic(str("[")),
                dynamic(re("[a-zA-Z_][a-zA-Z0-9_.]*")),
                dynamic(str("]")),
            ]),
        )
        // Line: pair, section, comment, or blank
        .rule(
            "line",
            choice(vec![
                dynamic(seq(vec![
                    dynamic(re("[a-zA-Z_][a-zA-Z0-9_]*")),
                    dynamic(re("[ \t]*=[ \t]*")),
                    dynamic(re("[^\\n]+")),
                ])),
                dynamic(seq(vec![
                    dynamic(str("[")),
                    dynamic(re("[a-zA-Z_][a-zA-Z0-9_.]*")),
                    dynamic(str("]")),
                ])),
                dynamic(re("[;#][^\\n]*")), // comment
                dynamic(re("[ \t]*")),      // blank
            ]),
        )
        // INI file: multiple lines
        .rule("ini", re("([^\\n]*\\n)*[^\\n]*"))
        .build()
}

/// Parsed INI configuration
#[derive(Debug, Clone, Default)]
pub struct IniConfig {
    /// Global key-value pairs (before any section)
    pub global: HashMap<String, String>,
    /// Sections with their key-value pairs
    pub sections: HashMap<String, HashMap<String, String>>,
}

/// Parse an INI configuration string
pub fn parse_ini(input: &str) -> Result<IniConfig, String> {
    let grammar = build_ini_grammar();
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    let _ast = parser
        .parse()
        .map_err(|e| format!("Parse error: {:?}", e))?;

    // Manual parsing for clarity
    let mut config = IniConfig::default();
    let mut current_section: Option<String> = None;

    for line in input.lines() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }

        // Section header
        if line.starts_with('[') && line.ends_with(']') {
            current_section = Some(line[1..line.len() - 1].to_string());
            config
                .sections
                .entry(current_section.clone().unwrap())
                .or_insert_with(HashMap::new);
            continue;
        }

        // Key-value pair
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim().to_string();
            let value = value.trim().to_string();

            match &current_section {
                Some(section) => {
                    config
                        .sections
                        .entry(section.clone())
                        .or_insert_with(HashMap::new)
                        .insert(key, value);
                }
                None => {
                    config.global.insert(key, value);
                }
            }
        }
    }

    Ok(config)
}

fn main() {
    println!("INI Configuration Parser Example");
    println!("=================================\n");

    let ini_content = r#"
# Database configuration
[database]
host = localhost
port = 5432
name = myapp_db
user = admin

; Server settings
[server]
host = 0.0.0.0
port = 8080
debug = true

# Feature flags
[features]
enable_cache = true
max_connections = 100
"#;

    println!("Input INI:");
    println!("----------");
    println!("{}\n", ini_content);

    match parse_ini(ini_content) {
        Ok(config) => {
            println!("Parsed Configuration:");
            println!("---------------------");

            if !config.global.is_empty() {
                println!("[global]");
                for (key, value) in &config.global {
                    println!("  {} = {}", key, value);
                }
            }

            for (section, pairs) in &config.sections {
                println!("[{}]", section);
                for (key, value) in pairs {
                    println!("  {} = {}", key, value);
                }
            }
        }
        Err(e) => println!("Error: {}", e),
    }

    // Example: accessing specific values
    println!("\nAccessing specific values:");
    if let Ok(config) = parse_ini(ini_content) {
        if let Some(db) = config.sections.get("database") {
            println!("Database host: {:?}", db.get("host"));
            println!("Database port: {:?}", db.get("port"));
        }
        if let Some(server) = config.sections.get("server") {
            println!("Server debug mode: {:?}", server.get("debug"));
        }
    }
}
