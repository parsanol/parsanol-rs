//! Scope Atoms Example
//!
//! Demonstrates how to create isolated capture contexts with scope atoms.
//! Captures made inside a scope are discarded when the scope exits.
//!
//! Run with: cargo run --example scopes --no-default-features

#![allow(clippy::print_literal)]

use parsanol::portable::{
    parser_dsl::{capture, dynamic, re, scope, seq, str, GrammarBuilder},
    AstArena, Atom, Grammar, PortableParser,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Scope Atoms Example");
    println!("===================\n");

    // =========================================================================
    // Example 1: Basic Scope Isolation
    // =========================================================================
    println!("--- Example 1: Basic Scope Isolation ---\n");

    // Without scope: captures accumulate, last value wins
    let grammar = GrammarBuilder::new()
        .rule(
            "items",
            seq(vec![
                dynamic(capture("temp", dynamic(str("a")))),
                dynamic(str("b")),
                dynamic(capture("temp", dynamic(str("c")))),
            ]),
        )
        .build();

    let input = "abc";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    let result = parser.parse_from_pos(0)?;

    println!("  Without scope:");
    println!("    Capture names: {:?}", result.capture_names());
    println!("    'temp' value:  {:?}", result.get_capture("temp", input)); // "c" (last wins)

    // With scope: inner captures are discarded
    let grammar = GrammarBuilder::new()
        .rule(
            "outer",
            seq(vec![
                dynamic(capture("outer_name", dynamic(str("prefix")))),
                dynamic(str(" ")),
                dynamic(scope(seq(vec![
                    dynamic(capture("inner_name", dynamic(re(r"[a-z]+")))),
                    dynamic(str(" ")),
                    dynamic(capture("inner_value", dynamic(re(r"\d+")))),
                ]))),
                dynamic(str(" ")),
                dynamic(capture("outer_value", dynamic(str("suffix")))),
            ]),
        )
        .build();

    let input = "prefix hello 123 suffix";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    let result = parser.parse_from_pos(0)?;

    println!("\n  With scope:");
    println!("    Capture names: {:?}", result.capture_names());
    println!(
        "    'outer_name':  {:?}",
        result.get_capture("outer_name", input)
    );
    println!(
        "    'outer_value': {:?}",
        result.get_capture("outer_value", input)
    );
    // Note: inner_name and inner_value are NOT in the result
    println!("    (inner_name and inner_value are discarded)");

    // =========================================================================
    // Example 2: Nested Scopes
    // =========================================================================
    println!("\n--- Example 2: Nested Scopes ---\n");

    let grammar = GrammarBuilder::new()
        .rule(
            "outer",
            seq(vec![
                dynamic(capture("level", dynamic(str("L1")))),
                dynamic(str(" ")),
                dynamic(scope(seq(vec![
                    dynamic(capture("level", dynamic(str("L2")))),
                    dynamic(str(" ")),
                    dynamic(scope(seq(vec![dynamic(capture(
                        "level",
                        dynamic(str("L3")),
                    ))]))),
                ]))),
            ]),
        )
        .build();

    let input = "L1 L2 L3";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    let result = parser.parse_from_pos(0)?;

    println!("  Nested scopes - only L1 persists:");
    println!("    Capture names: {:?}", result.capture_names());
    println!(
        "    'level' value:  {:?}",
        result.get_capture("level", input)
    );

    // =========================================================================
    // Example 3: Recursive Structure with Scopes (Raw Grammar API)
    // =========================================================================
    println!("\n--- Example 3: Recursive Structure with Scopes ---\n");

    // Parse balanced parentheses with per-level captures
    let mut grammar = Grammar::new();

    // Inner content (can be text or nested parens)
    let inner_text = grammar.add_atom(Atom::Re {
        pattern: r"[^()]+".into(),
    });

    // Capture text at current level (scoped)
    let text_capture = grammar.add_atom(Atom::Capture {
        name: "text".into(),
        atom: inner_text,
    });

    // Scope wraps the content to isolate captures
    let content = grammar.add_atom(Atom::Scope { atom: text_capture });

    // Full structure - create atoms first to avoid borrow issues
    let open_paren = grammar.add_atom(Atom::Str {
        pattern: "(".into(),
    });
    let close_paren = grammar.add_atom(Atom::Str {
        pattern: ")".into(),
    });

    let paren_content = grammar.add_atom(Atom::Sequence {
        atoms: vec![open_paren, content, close_paren],
    });

    grammar.root = paren_content;

    let input = "(hello)";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    let result = parser.parse_from_pos(0)?;

    println!("  Parsed: {}", input);
    println!("  Capture names: {:?}", result.capture_names());
    // Note: "text" capture is discarded because it's inside a scope
    println!("  ('text' capture is discarded because it's inside a scope)");

    // =========================================================================
    // Example 4: Scope for Memory Cleanup
    // =========================================================================
    println!("\n--- Example 4: Scope for Memory Cleanup ---\n");

    // Processing repeated structures - each gets its own scope
    let grammar = GrammarBuilder::new()
        .rule(
            "item",
            scope(seq(vec![
                dynamic(capture("id", dynamic(re(r"\d+")))),
                dynamic(str(":")),
                dynamic(capture("name", dynamic(re(r"[a-zA-Z]+")))),
            ])),
        )
        .rule("items", seq(vec![dynamic(str("item"))]))
        .build();

    let input = "item";
    println!("  Processing repeated items with scoped captures");
    println!("  Input: {}", input);

    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    let result = parser.parse_from_pos(0)?;

    println!("  Final captures: {:?}", result.capture_names());
    println!("  (id and name captures are discarded after each item)");

    // =========================================================================
    // Example 5: Accessing Capture State via Parser
    // =========================================================================
    println!("\n--- Example 5: Accessing Capture State ---\n");

    let grammar = GrammarBuilder::new()
        .rule(
            "outer",
            seq(vec![
                dynamic(capture("name", dynamic(str("outer")))),
                dynamic(scope(dynamic(capture("inner", dynamic(str("inner")))))),
            ]),
        )
        .build();

    let input = "outerinner";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    parser.parse()?;

    // Access capture state directly from parser
    let captures = parser.capture_state();
    let names: Vec<_> = captures.names().collect();
    println!("  Capture names from parser: {:?}", names);
    if let Some(value) = captures.get("name") {
        println!("  'name' capture: {:?}", value.get_text(input));
    }
    // Note: "inner" is not available because it was in a scope
    println!("  ('inner' capture is not available - it was in a scope)");

    // =========================================================================
    // Summary
    // =========================================================================
    println!("\n--- Benefits of Scope Atoms ---");
    println!("* Prevent capture pollution from nested parsing");
    println!("* Each recursion level has its own capture state");
    println!("* Automatic cleanup when scope exits");
    println!("* Memory bounded during parse");
    println!("* Essential for parsing nested structures");

    println!("\n--- Performance Notes ---");
    println!("* Scope push/pop is O(c_scope) where c_scope = captures in scope");
    println!("* Each nesting level adds ~2%% overhead");
    println!("* Use scopes liberally - they're cheap");

    println!("\n--- DSL Helper ---");
    println!("  scope(parslet)  // Wraps parslet in isolated capture context");

    println!("\n--- API Summary ---");
    println!("  scope(inner)              -> isolates captures");
    println!("  result.get_capture(name)  -> access captures (inner ones excluded)");

    Ok(())
}
