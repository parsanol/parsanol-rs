//! Capture Atoms Example
//!
//! Demonstrates how to extract named values from parsed input using capture atoms.
//! Captures work like named groups in regular expressions, but are integrated
//! into the parsing grammar and work across all backends.
//!
//! Run with: cargo run --example captures --no-default-features

#![allow(clippy::print_literal)]

use parsanol::portable::{
    parser_dsl::{capture, dynamic, re, seq, str, GrammarBuilder},
    AstArena, Grammar, PortableParser, Atom,
};
use std::collections::HashMap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Capture Atoms Example");
    println!("=====================\n");

    // =========================================================================
    // Example 1: Basic Capture
    // =========================================================================
    println!("--- Example 1: Basic Capture ---\n");

    let grammar = GrammarBuilder::new()
        .rule("greeting", capture("greeting", dynamic(str("hello"))))
        .build();

    let input = "hello world";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    let result = parser.parse_from_pos(0)?;

    // Extract the captured value
    if let Some(text) = result.get_capture("greeting", input) {
        println!("  Captured 'greeting': {:?}", text);  // "hello"
    }

    // =========================================================================
    // Example 2: Email Parsing with Nested Captures
    // =========================================================================
    println!("\n--- Example 2: Email Parsing with Nested Captures ---\n");

    let grammar = GrammarBuilder::new()
        .rule("email", capture("email",
            seq(vec![
                dynamic(capture("local", dynamic(re(r"[a-zA-Z0-9._%+-]+")))),
                dynamic(str("@")),
                dynamic(capture("domain", dynamic(re(r"[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}")))),
            ])
        ))
        .build();

    let input = "user@example.com";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    let result = parser.parse_from_pos(0)?;

    println!("  Full email: {:?}", result.get_capture("email", input));
    println!("  Local part: {:?}", result.get_capture("local", input));
    println!("  Domain:     {:?}", result.get_capture("domain", input));

    // =========================================================================
    // Example 3: Inspecting All Captures
    // =========================================================================
    println!("\n--- Example 3: Inspecting All Captures ---\n");

    // Get all capture names
    println!("  Capture names: {:?}", result.capture_names());

    // Get all captures as HashMap
    let all_captures: HashMap<&str, &str> = result.captures(input);
    println!("  All captures:");
    for (name, value) in &all_captures {
        println!("    {} = {:?}", name, value);
    }

    // =========================================================================
    // Example 4: Using Raw Grammar API
    // =========================================================================
    println!("\n--- Example 4: Using Raw Grammar API ---\n");

    let mut grammar = Grammar::new();

    let name = grammar.add_atom(Atom::Re { pattern: r"[a-zA-Z]+".into() });
    let name_capture = grammar.add_atom(Atom::Capture {
        name: "name".into(),
        atom: name,
    });

    let comma = grammar.add_atom(Atom::Str { pattern: ", ".into() });

    let age = grammar.add_atom(Atom::Re { pattern: r"\d+".into() });
    let age_capture = grammar.add_atom(Atom::Capture {
        name: "age".into(),
        atom: age,
    });

    let person = grammar.add_atom(Atom::Sequence {
        atoms: vec![name_capture, comma, age_capture],
    });
    grammar.root = person;

    let input = "Alice, 30";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    let result = parser.parse_from_pos(0)?;

    println!("  Name: {:?}", result.get_capture("name", input));
    println!("  Age:  {:?}", result.get_capture("age", input));

    // =========================================================================
    // Example 5: Accessing Capture State Directly
    // =========================================================================
    println!("\n--- Example 5: Accessing Capture State Directly ---\n");

    let grammar = GrammarBuilder::new()
        .rule("item", capture("item", dynamic(re(r"[a-z]+"))))
        .build();

    let input = "apple";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    parser.parse()?;

    // Access capture state directly from parser
    let captures = parser.capture_state();
    let names: Vec<_> = captures.names().collect();
    println!("  Capture names from parser: {:?}", names);
    if let Some(value) = captures.get("item") {
        println!("  Item: {:?}", value.get_text(input));
    }

    // =========================================================================
    // Summary
    // =========================================================================
    println!("\n--- Benefits of Capture Atoms ---");
    println!("* Zero-copy: captures store offsets, not strings");
    println!("* Works across all backends (Packrat, Bytecode, Streaming)");
    println!("* Clean API: capture_names(), get_capture(), captures()");
    println!("* No AST construction needed for simple extraction");
    println!("* DSL helpers: capture(\"name\", parslet)");

    println!("\n--- Performance Notes ---");
    println!("* Captures add minimal overhead (~5%% for heavy use)");
    println!("* Capture lookup is O(n) where n = number of captures");
    println!("* Consider scope atoms for nested contexts");

    println!("\n--- API Summary ---");
    println!("  parser.parse_from_pos(0)  -> ParseResult with captures");
    println!("  parser.capture_state()    -> &CaptureState");
    println!("  result.get_capture(name, input) -> Option<&str>");

    Ok(())
}
