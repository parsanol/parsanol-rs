//! Grammar Modularity Example
//!
//! Demonstrates how to compose grammars from modules in Rust.
//!
//! Run with: cargo run --example modularity --no-default-features

#![allow(clippy::print_literal)]

use parsanol::portable::{
    parser_dsl::{choice, dynamic, re, ref_, seq, str, GrammarBuilder},
    AstArena, Grammar, PortableParser,
};

/// Module A: Simple identifier language
fn build_module_a() -> Vec<(&'static str, parsanol::portable::parser_dsl::Re<'static>)> {
    vec![("a_language", re(r"aaa"))]
}

/// Module B: Another simple language
fn build_module_b() -> Vec<(&'static str, parsanol::portable::parser_dsl::Re<'static>)> {
    vec![("b_language", re(r"bbb"))]
}

/// Module C: Yet another language
fn build_module_c() -> Vec<(&'static str, parsanol::portable::parser_dsl::Re<'static>)> {
    vec![("c_language", re(r"ccc"))]
}

/// Build combined grammar from modules
fn build_grammar() -> Grammar {
    let mut builder = GrammarBuilder::new();

    // Import rules from module A
    for (name, parslet) in build_module_a() {
        builder = builder.rule(name, parslet);
    }

    // Import rules from module B
    for (name, parslet) in build_module_b() {
        builder = builder.rule(name, parslet);
    }

    // Import rules from module C
    for (name, parslet) in build_module_c() {
        builder = builder.rule(name, parslet);
    }

    // Whitespace
    builder = builder.rule("space", re(r"[ \t]*"));

    // Root rule combining all modules
    builder = builder.rule(
        "root",
        choice(vec![
            // a(aaa)
            dynamic(seq(vec![
                dynamic(str("a(")),
                dynamic(ref_("a_language")),
                dynamic(str(")")),
            ])),
            // b(bbb)
            dynamic(seq(vec![
                dynamic(str("b(")),
                dynamic(ref_("b_language")),
                dynamic(str(")")),
            ])),
            // c(ccc)
            dynamic(seq(vec![
                dynamic(str("c(")),
                dynamic(ref_("c_language")),
                dynamic(str(")")),
            ])),
        ]),
    );

    builder.build()
}

fn main() {
    println!("Grammar Modularity Example");
    println!("==========================");
    println!();

    println!("This example demonstrates how to compose grammars from modules.");
    println!();

    // Demonstrate module functions
    println!("Module A defines: a_language -> 'aaa'");
    println!("Module B defines: b_language -> 'bbb'");
    println!("Module C defines: c_language -> 'ccc'");
    println!();

    // Combined grammar
    println!("Combined grammar root rule:");
    println!("  root = 'a(' a_language ')'");
    println!("       | 'b(' b_language ')'");
    println!("       | 'c(' c_language ')'");
    println!();

    // Build and test
    let grammar = build_grammar();

    // Test inputs
    let inputs = [
        ("a(aaa)", true),
        ("b(bbb)", true),
        ("c(ccc)", true),
        ("a(aab)", false), // invalid
    ];

    println!("Test inputs:");
    for (input, should_match) in inputs {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);

        match parser.parse() {
            Ok(_) => {
                let status = if should_match { "OK" } else { "UNEXPECTED" };
                println!("  '{}' - parsed ({})", input, status);
            }
            Err(_) => {
                let status = if !should_match {
                    "OK (expected failure)"
                } else {
                    "FAIL"
                };
                println!("  '{}' - failed ({})", input, status);
            }
        }
    }
    println!();

    // Alternative: Trait-based modularity
    println!("--- Alternative: Trait-based Modularity ---");
    println!();
    println!("In Rust, you can also use traits for modularity:");
    println!();
    println!("  trait GrammarModule {{");
    println!("      fn add_rules(&self, builder: &mut GrammarBuilder);");
    println!("  }}");
    println!();
    println!("  impl GrammarModule for ModuleA {{");
    println!("      fn add_rules(&self, builder: &mut GrammarBuilder) {{");
    println!("          builder.rule(\"a_language\", re(r\"aaa\"));");
    println!("      }}");
    println!("  }}");
    println!();

    println!("This demonstrates approaches to grammar modularity in Rust:");
    println!("1. Function-based: Functions returning rule vectors");
    println!("2. Trait-based: Types implementing GrammarModule trait");
    println!("3. Direct composition: Combining rules in build_grammar()");
}
