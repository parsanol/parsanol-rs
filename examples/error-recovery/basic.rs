//! Error Recovery Example
//!
//! Demonstrates how to recover from parse errors and continue parsing.
//! Shows multiple strategies for handling malformed input gracefully.
//!
//! Run with: cargo run --example error-recovery --no-default-features

#![allow(clippy::print_literal)]

use parsanol::portable::{
    parser_dsl::{dynamic, re, seq, GrammarBuilder},
    AstArena, Grammar, PortableParser,
};

/// Represents a parse result that may contain errors
#[derive(Debug, Clone)]
struct ParseResult {
    value: Option<String>,
    errors: Vec<ParseError>,
    recovered: bool,
}

#[derive(Debug, Clone)]
struct ParseError {
    position: usize,
    message: String,
    context: String,
}

/// Error recovery parser that can continue after failures
struct ErrorRecoveringParser {
    grammar: Grammar,
}

impl ErrorRecoveringParser {
    fn new(grammar: Grammar) -> Self {
        Self { grammar }
    }

    /// Parse with error recovery - skip bad tokens and continue
    fn parse_with_recovery(&self, input: &str) -> Vec<ParseResult> {
        let mut results = Vec::new();
        let mut pos = 0;

        while pos < input.len() {
            match self.try_parse_at(input, pos) {
                Ok((value, next_pos)) => {
                    results.push(ParseResult {
                        value: Some(value),
                        errors: vec![],
                        recovered: false,
                    });
                    pos = next_pos;
                }
                Err(error) => {
                    // Skip to next synchronization point
                    let recovery_pos = self.find_sync_point(input, pos);
                    results.push(ParseResult {
                        value: None,
                        errors: vec![error],
                        recovered: recovery_pos > pos,
                    });
                    pos = recovery_pos;
                }
            }
        }

        results
    }

    fn try_parse_at(&self, input: &str, pos: usize) -> Result<(String, usize), ParseError> {
        let slice = &input[pos..];
        let mut arena = AstArena::for_input(slice.len());
        let mut parser = PortableParser::new(&self.grammar, slice, &mut arena);

        match parser.parse() {
            Ok(_) => {
                // Simple approach: consume to next delimiter or end
                let consumed = self.find_consumed_length(slice);
                Ok((slice[..consumed].to_string(), pos + consumed))
            }
            Err(_) => Err(ParseError {
                position: pos,
                message: "Failed to parse at this position".to_string(),
                context: self.get_context(input, pos),
            }),
        }
    }

    fn find_consumed_length(&self, slice: &str) -> usize {
        // Find next delimiter
        for (i, c) in slice.char_indices() {
            if c == ',' || c == ';' || c == '\n' {
                return i;
            }
        }
        slice.len()
    }

    fn find_sync_point(&self, input: &str, pos: usize) -> usize {
        // Find next delimiter or whitespace as sync point
        let slice = &input[pos..];
        for (i, c) in slice.char_indices() {
            if c == ',' || c == ';' || c == '\n' || c.is_whitespace() {
                return pos + i + 1;
            }
        }
        input.len()
    }

    fn get_context(&self, input: &str, pos: usize) -> String {
        let start = pos.saturating_sub(10);
        let end = (pos + 10).min(input.len());
        format!("...{}[HERE]{}...", &input[start..pos], &input[pos..end])
    }
}

/// Collect all errors without stopping
fn collect_errors(input: &str, grammar: &Grammar) -> Vec<ParseError> {
    let mut errors = Vec::new();
    let mut pos = 0;

    while pos < input.len() {
        let slice = &input[pos..];
        let mut arena = AstArena::for_input(slice.len());
        let mut parser = PortableParser::new(grammar, slice, &mut arena);

        if parser.parse().is_err() {
            errors.push(ParseError {
                position: pos,
                message: "Parse error".to_string(),
                context: format!("at character: {:?}", input.chars().nth(pos)),
            });
            pos += 1; // Advance by one character
        } else {
            // Find next delimiter and advance
            let mut consumed = 1;
            for (i, c) in slice.char_indices() {
                if c == ',' || c == ';' || c == '\n' {
                    consumed = i + 1;
                    break;
                }
            }
            pos += consumed;
        }
    }

    errors
}

fn main() {
    println!("Error Recovery Example");
    println!("======================");
    println!();

    println!("This example demonstrates strategies for handling parse errors gracefully.\n");

    // Simple grammar for demonstration - expression: number operator number
    let grammar = GrammarBuilder::new()
        .rule(
            "expr",
            seq(vec![
                dynamic(re(r"[0-9]+")),
                dynamic(re(r"[+\-*/]")),
                dynamic(re(r"[0-9]+")),
            ]),
        )
        .build();

    // Example 1: Partial recovery
    println!("--- Example 1: Partial Recovery ---");
    let input = "1+2, 3*, 5+6";
    println!("Input: {}", input);

    let parser = ErrorRecoveringParser::new(grammar.clone());
    let results = parser.parse_with_recovery(input);

    for (i, result) in results.iter().enumerate() {
        println!("\nSegment {}:", i + 1);
        if let Some(value) = &result.value {
            println!("  Parsed: {}", value);
        }
        for error in &result.errors {
            println!("  Error at {}: {}", error.position, error.message);
            println!("  Context: {}", error.context);
        }
        if result.recovered {
            println!("  (Recovered and continued)");
        }
    }

    // Example 2: Error collection
    println!("\n--- Example 2: Collect All Errors ---");
    let bad_input = "1+2, @#$, 3+4, %^&, 5+6";
    println!("Input: {}", bad_input);

    let errors = collect_errors(bad_input, &grammar);
    println!("Found {} errors:", errors.len());
    for error in &errors {
        println!("  Position {}: {:?}", error.position, error.context);
    }

    println!("\n--- Strategies Demonstrated ---");
    println!("1. Synchronization points: Skip to delimiters after errors");
    println!("2. Error collection: Gather all errors before reporting");
    println!("3. Context preservation: Show surrounding text for errors");
    println!("4. Graceful degradation: Continue parsing after failures");

    println!("\n--- Benefits ---");
    println!("* Better user experience with multiple error messages");
    println!("* IDE integration with error highlighting");
    println!("* Batch error fixing instead of one-at-a-time");
    println!("* Robust parsing for real-world messy input");
}
