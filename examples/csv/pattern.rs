//! CSV Parser Example - Serialized Transform Mode
//!
//! This demonstrates parsing CSV data and serializing for FFI to a host language.
//!
//! Run with: cargo run --example csv_pattern --no-default-features

use parsanol::portable::{
    parser_dsl::{re, GrammarBuilder},
    AstArena, PortableParser,
};
use serde::{Deserialize, Serialize};

/// Build CSV grammar - match entire input including newlines
fn build_csv_grammar() -> parsanol::portable::Grammar {
    GrammarBuilder::new()
        .rule("csv", re(r"(?s).*")) // Match entire input including newlines
        .build()
}

/// Serializable CSV row
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CsvRow(pub Vec<String>);

/// Serializable CSV data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CsvData {
    pub headers: Vec<String>,
    pub rows: Vec<CsvRow>,
}

/// Serialized Transform Mode: Parse → Serialize to JSON string
pub fn parse_to_json(input: &str) -> Result<String, String> {
    let grammar = build_csv_grammar();
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    let _ast = parser
        .parse()
        .map_err(|e| format!("Parse error: {:?}", e))?;
    let csv = parse_csv(input)?;
    serde_json::to_string(&csv).map_err(|e| e.to_string())
}

/// Parse CSV from input string
fn parse_csv(input: &str) -> Result<CsvData, String> {
    let lines: Vec<&str> = input.lines().collect();
    if lines.is_empty() {
        return Ok(CsvData {
            headers: vec![],
            rows: vec![],
        });
    }

    let headers: Vec<String> = lines[0].split(',').map(|s| s.trim().to_string()).collect();

    let mut rows = vec![];
    for line in lines.iter().skip(1) {
        let values: Vec<String> = line.split(',').map(|s| s.trim().to_string()).collect();
        rows.push(CsvRow(values));
    }

    Ok(CsvData { headers, rows })
}

fn main() {
    println!("CSV Parser - Serialized Transform Mode (Parse → Serialize for FFI)");
    println!("================================================================\n");

    let input = "name,age,city\nAlice,30,NYC\nBob,25,LA";
    match parse_to_json(input) {
        Ok(json) => println!("Input:\n{}\n\nJSON: {}", input, json),
        Err(e) => println!("Error: {}", e),
    }
}
