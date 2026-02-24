//! CSV Parser Example - Native Transform Mode
//!
//! This demonstrates parsing CSV data and transforming in Rust - no serialization.
//!
//! Run with: cargo run --example csv_transform --no-default-features

use parsanol::portable::{
    parser_dsl::{re, GrammarBuilder},
    AstArena, PortableParser,
};

/// Build CSV grammar - match entire input including newlines
fn build_csv_grammar() -> parsanol::portable::Grammar {
    GrammarBuilder::new()
        .rule("csv", re(r"(?s).*")) // Match entire input including newlines
        .build()
}

/// Native Rust CSV row (no serialization)
#[derive(Debug, Clone)]
pub struct CsvRow(pub Vec<String>);

/// Native Rust CSV data (no serialization)
#[derive(Debug, Clone)]
pub struct CsvData {
    pub headers: Vec<String>,
    pub rows: Vec<CsvRow>,
}

/// Native Transform Mode: Parse + Transform in Rust → Return native CsvData
pub fn parse_and_transform(input: &str) -> Result<CsvData, String> {
    let grammar = build_csv_grammar();
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    let _ast = parser
        .parse()
        .map_err(|e| format!("Parse error: {:?}", e))?;
    parse_csv(input)
}

/// Parse CSV from input string (no serialization)
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
    println!("CSV Parser - Native Transform Mode (Parse + Transform in Rust)");
    println!("============================================================\n");

    let input = "name,age,city\nAlice,30,NYC\nBob,25,LA";
    match parse_and_transform(input) {
        Ok(csv) => {
            println!("Headers: {:?}", csv.headers);
            for (i, row) in csv.rows.iter().enumerate() {
                println!("Row {}: {:?}", i + 1, row.0);
            }
        }
        Err(e) => println!("Error: {}", e),
    }
}
