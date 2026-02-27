//! Streaming Parser Example
//!
//! Demonstrates how to parse large files without loading everything into memory.
//! Uses chunk-based parsing for memory efficiency.
//!
//! Run with: cargo run --example streaming --no-default-features

#![allow(clippy::print_literal)]

use parsanol::portable::{
    parser_dsl::{re, GrammarBuilder},
    AstArena, Grammar, PortableParser,
};

/// Build a grammar for CSV-like row parsing
fn build_row_grammar() -> Grammar {
    GrammarBuilder::new()
        // Row: fields separated by commas (simplified)
        .rule("row", re(r"[^,\n\r]+(?:,[^,\n\r]+)*"))
        .build()
}

/// Process input in chunks, demonstrating memory-efficient parsing
fn process_streaming(input: &str, chunk_size: usize) {
    println!(
        "Processing {} bytes in {}-byte chunks\n",
        input.len(),
        chunk_size
    );

    let grammar = build_row_grammar();

    // Simulate chunked reading
    let mut pending = String::new();
    let mut chunk_count = 0;
    let mut row_count = 0;

    for chunk in input.as_bytes().chunks(chunk_size) {
        chunk_count += 1;
        let chunk_str = String::from_utf8_lossy(chunk);
        pending.push_str(&chunk_str);

        // Process complete rows
        while let Some(newline_pos) = pending.find('\n') {
            let line: String = pending.drain(..=newline_pos).collect();
            let trimmed = line.trim();

            if !trimmed.is_empty() {
                let mut arena = AstArena::for_input(trimmed.len());
                let mut parser = PortableParser::new(&grammar, trimmed, &mut arena);

                match parser.parse() {
                    Ok(_) => {
                        row_count += 1;
                        println!("  Row {}: {:?}", row_count, trimmed);
                    }
                    Err(e) => {
                        println!("  Error parsing row: {:?}", e);
                    }
                }
            }
        }
    }

    // Process remaining data
    if !pending.trim().is_empty() {
        let mut arena = AstArena::for_input(pending.len());
        let mut parser = PortableParser::new(&grammar, &pending, &mut arena);

        match parser.parse() {
            Ok(_) => {
                row_count += 1;
                println!("  Row {} (final): {:?}", row_count, pending.trim());
            }
            Err(e) => {
                println!("  Error parsing final row: {:?}", e);
            }
        }
    }

    println!("\nProcessed {} chunks, {} rows", chunk_count, row_count);
}

fn main() {
    println!("Streaming Parser Example");
    println!("========================");
    println!();

    println!("This example demonstrates memory-efficient parsing of large files.");
    println!("Data is processed in chunks without loading the entire file.\n");

    // Simulated large input
    let input = "name,age,city\n\
                 Alice,30,NYC\n\
                 Bob,25,LA\n\
                 Charlie,35,Chicago\n\
                 Diana,28,Seattle\n\
                 Eve,32,Boston";

    println!("--- Simulated File Content ---");
    println!("{}", input);
    println!();

    println!("--- Processing in 32-byte chunks ---");
    process_streaming(input, 32);

    println!("\n--- Benefits ---");
    println!("* Constant memory usage regardless of file size");
    println!("* Immediate processing of parsed rows");
    println!("* No need to load entire file into memory");
    println!("* Suitable for GB-scale files");

    println!("\n--- Use Cases ---");
    println!("* Log file analysis (GBs of logs)");
    println!("* CSV/TSV processing for data pipelines");
    println!("* Real-time event stream parsing");
    println!("* Memory-constrained environments");
}
