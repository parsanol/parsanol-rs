//! Dynamic Atoms Example
//!
//! Demonstrates runtime-determined parsing via callbacks.
//! Dynamic atoms allow context-sensitive parsing where the grammar
//! itself depends on the input or previously captured values.
//!
//! Run with: cargo run --example dynamic --no-default-features

#![allow(clippy::print_literal)]

use parsanol::portable::dynamic::{register_dynamic_callback, DynamicCallback, DynamicContext};
use parsanol::portable::{
    parser_dsl::{capture, dynamic, dynamic_with_id, re, seq, str, GrammarBuilder},
    AstArena, Atom, PortableParser,
};

// =========================================================================
// Example 1: Constant Callback
// =========================================================================

struct ConstCallback {
    atom: Atom,
    desc: &'static str,
}

impl DynamicCallback for ConstCallback {
    fn resolve(&self, ctx: &DynamicContext) -> Option<Atom> {
        println!(
            "    [ConstCallback '{}'] invoked at position {}",
            self.desc,
            ctx.pos()
        );
        Some(self.atom.clone())
    }

    fn description(&self) -> &str {
        self.desc
    }
}

fn example_const_callback() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Example 1: Constant Callback ---\n");

    let callback = ConstCallback {
        atom: Atom::Str {
            pattern: "hello".into(),
        },
        desc: "greeting",
    };
    let callback_id = register_dynamic_callback(Box::new(callback));

    let grammar = GrammarBuilder::new()
        .rule("dynamic", dynamic_with_id(callback_id))
        .build();

    let input = "hello world";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    let result = parser.parse_from_pos(0)?;

    println!("  Parsed successfully at position {}", result.end_pos);
    Ok(())
}

// =========================================================================
// Example 2: Context-Sensitive Callback
// =========================================================================

struct LanguageKeywordCallback;

impl DynamicCallback for LanguageKeywordCallback {
    fn resolve(&self, ctx: &DynamicContext) -> Option<Atom> {
        let input = ctx.input();
        let pos = ctx.pos();

        println!("    [LanguageKeywordCallback] at position {}", pos);

        // Look at preceding context to determine language
        if pos >= 5 && &input[pos - 5..pos] == "ruby " {
            println!("    -> Detected Ruby context, returning 'def'");
            Some(Atom::Str {
                pattern: "def".into(),
            })
        } else if pos >= 7 && &input[pos - 7..pos] == "python " {
            println!("    -> Detected Python context, returning 'lambda'");
            Some(Atom::Str {
                pattern: "lambda".into(),
            })
        } else {
            println!("    -> No context detected, returning 'function'");
            Some(Atom::Str {
                pattern: "function".into(),
            })
        }
    }

    fn description(&self) -> &str {
        "language_keyword"
    }
}

fn example_context_sensitive() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n--- Example 2: Context-Sensitive Callback ---\n");

    let callback_id = register_dynamic_callback(Box::new(LanguageKeywordCallback));

    let grammar = GrammarBuilder::new()
        .rule("keyword", dynamic_with_id(callback_id))
        .build();

    let test_cases = [
        ("ruby def method", "Ruby"),
        ("python lambda x", "Python"),
        ("function foo()", "JavaScript"),
    ];

    for (input, lang) in test_cases {
        println!("  Testing {} input: {:?}", lang, input);
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        match parser.parse_from_pos(0) {
            Ok(result) => println!("  ✓ Parsed at position {}", result.end_pos),
            Err(e) => println!("  ✗ Parse error: {:?}", e),
        }
        println!();
    }

    Ok(())
}

// =========================================================================
// Example 3: Position-Based Callback
// =========================================================================

struct PositionCallback;

impl DynamicCallback for PositionCallback {
    fn resolve(&self, ctx: &DynamicContext) -> Option<Atom> {
        let pos = ctx.pos();
        let input = ctx.input();

        println!("    [PositionCallback] at position {}", pos);

        // Different behavior at different positions
        if pos == 0 {
            // First position: expect a keyword (return regex that matches any of them)
            Some(Atom::Re {
                pattern: r"(let|const|var)".into(),
            })
        } else if pos < input.len() / 2 {
            // First half: expect identifier
            Some(Atom::Re {
                pattern: r"[a-zA-Z_][a-zA-Z0-9_]*".into(),
            })
        } else {
            // Second half: expect value
            Some(Atom::Re {
                pattern: r"\d+".into(),
            })
        }
    }

    fn description(&self) -> &str {
        "position_based"
    }
}

fn example_position_based() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Example 3: Position-Based Callback ---\n");

    let callback_id = register_dynamic_callback(Box::new(PositionCallback));

    let grammar = GrammarBuilder::new()
        .rule(
            "stmt",
            seq(vec![
                dynamic(dynamic_with_id(callback_id)),
                dynamic(str(" ")),
                dynamic(dynamic_with_id(callback_id)),
                dynamic(str("=")),
                dynamic(dynamic_with_id(callback_id)),
            ]),
        )
        .build();

    let input = "let x=42";
    println!("  Parsing: {:?}", input);
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    match parser.parse_from_pos(0) {
        Ok(result) => println!("  ✓ Parsed at position {}", result.end_pos),
        Err(e) => println!("  ✗ Parse error: {:?}", e),
    }

    Ok(())
}

// =========================================================================
// Example 4: Configuration-Driven Parsing
// =========================================================================

struct ConfigCallback {
    strict_mode: bool,
}

impl DynamicCallback for ConfigCallback {
    fn resolve(&self, _ctx: &DynamicContext) -> Option<Atom> {
        println!("    [ConfigCallback] strict_mode={}", self.strict_mode);

        if self.strict_mode {
            // Strict mode: only lowercase identifiers
            Some(Atom::Re {
                pattern: r"[a-z][a-z0-9_]*".into(),
            })
        } else {
            // Lenient mode: any identifier
            Some(Atom::Re {
                pattern: r"[a-zA-Z_][a-zA-Z0-9_]*".into(),
            })
        }
    }

    fn description(&self) -> &str {
        "config_driven"
    }
}

fn example_config_driven() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n--- Example 4: Configuration-Driven Parsing ---\n");

    // Strict mode callback
    let strict_callback = ConfigCallback { strict_mode: true };
    let strict_id = register_dynamic_callback(Box::new(strict_callback));

    let strict_grammar = GrammarBuilder::new()
        .rule("id", dynamic_with_id(strict_id))
        .build();

    println!("  Strict mode (lowercase only):");
    for input in ["variable", "Variable"] {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&strict_grammar, input, &mut arena);
        match parser.parse_from_pos(0) {
            Ok(_) => println!("    ✓ {:?} - accepted", input),
            Err(_) => println!("    ✗ {:?} - rejected", input),
        }
    }

    Ok(())
}

// =========================================================================
// Example 5: Capture-Aware Callback
// =========================================================================

struct CaptureAwareCallback;

impl DynamicCallback for CaptureAwareCallback {
    fn resolve(&self, ctx: &DynamicContext) -> Option<Atom> {
        println!("    [CaptureAwareCallback] checking captures");

        // Check if we have a "type" capture - use get_capture_text for string
        if let Some(type_name) = ctx.get_capture_text("type") {
            println!("    -> Found type capture: {:?}", type_name);
            match type_name {
                "int" => Some(Atom::Re {
                    pattern: r"\d+".into(),
                }),
                "str" => Some(Atom::Re {
                    pattern: r"[a-zA-Z]+".into(),
                }),
                "bool" => Some(Atom::Re {
                    pattern: r"(true|false)".into(),
                }),
                _ => None,
            }
        } else {
            println!("    -> No type capture, matching identifier");
            Some(Atom::Re {
                pattern: r"[a-z]+".into(),
            })
        }
    }

    fn description(&self) -> &str {
        "capture_aware"
    }
}

fn example_capture_aware() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n--- Example 5: Capture-Aware Callback ---\n");

    let callback_id = register_dynamic_callback(Box::new(CaptureAwareCallback));

    // Grammar: type:name = value
    // The value parser depends on the type
    let grammar = GrammarBuilder::new()
        .rule(
            "declaration",
            seq(vec![
                dynamic(capture("type", dynamic(re(r"[a-z]+")))),
                dynamic(str(":")),
                dynamic(capture("name", dynamic(re(r"[a-z]+")))),
                dynamic(str("=")),
                dynamic(capture("value", dynamic(dynamic_with_id(callback_id)))),
            ]),
        )
        .build();

    let test_cases = [
        ("int:count=42", "int"),
        ("str:message=hello", "str"),
        ("bool:enabled=true", "bool"),
    ];

    for (input, _expected_type) in test_cases {
        println!("  Parsing: {:?}", input);
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        match parser.parse_from_pos(0) {
            Ok(result) => {
                println!("  ✓ Parsed successfully");
                println!("    type:  {:?}", result.get_capture("type", input));
                println!("    name:  {:?}", result.get_capture("name", input));
                println!("    value: {:?}", result.get_capture("value", input));
            }
            Err(e) => println!("  ✗ Parse error: {:?}", e),
        }
        println!();
    }

    Ok(())
}

// =========================================================================
// Main
// =========================================================================

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Dynamic Atoms Example");
    println!("====================\n");

    example_const_callback()?;
    example_context_sensitive()?;
    example_position_based()?;
    example_config_driven()?;
    example_capture_aware()?;

    // =========================================================================
    // Summary
    // =========================================================================
    println!("\n--- Benefits of Dynamic Atoms ---");
    println!("* Context-sensitive parsing at runtime");
    println!("* Access to position, input, and captures");
    println!("* Plugin architecture support");
    println!("* Configuration-driven grammars");

    println!("\n--- Backend Compatibility ---");
    println!("* Packrat:  Native support (direct callback invocation)");
    println!("* Bytecode: Packrat fallback (slower, uses internal Packrat)");
    println!("* Streaming: Packrat fallback (slower, uses internal Packrat)");

    println!("\n--- Performance Notes ---");
    println!("* Native (Packrat): ~5%% overhead per dynamic atom");
    println!("* Fallback (Bytecode/Streaming): ~20%% slower than native");
    println!("* For heavy dynamic usage, prefer Packrat backend");
    println!("* Callback should be fast - avoid I/O or heavy computation");

    println!("\n--- DSL Helper ---");
    println!("  dynamic_with_id(callback_id)  // Uses registered callback");

    println!("\n--- API Summary ---");
    println!("  trait DynamicCallback {{");
    println!("    fn resolve(&self, ctx: &DynamicContext) -> Option<Atom>;");
    println!("    fn description(&self) -> &str;");
    println!("  }}");

    Ok(())
}
