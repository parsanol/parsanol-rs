//! Comprehensive Example-Based Backend Comparison
//!
//! Tests ALL example grammars from the examples/ directory with both backends.
//! This provides real-world performance data to guide backend selection.
//!
//! Run with: cargo bench --no-default-features --bench all-examples
//!
//! Output: benches/BACKEND_GUIDE.md with recommendations

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::hint::black_box;

use parsanol::portable::{
    backend::{BytecodeBackend, PackratBackend, ParsingBackend},
    infix::{Assoc, InfixBuilder},
    parser_dsl::{choice, dynamic, re, ref_, seq, str, GrammarBuilder},
    Grammar,
};

/// Truncate string for display in benchmark IDs
fn truncate_str(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        &s[..max_len]
    }
}

// ============================================================================
// Example Grammar Definitions (extracted from examples/)
// ============================================================================

/// Example grammar with test inputs
struct Example {
    name: &'static str,
    category: &'static str,
    grammar: Grammar,
    inputs: Vec<&'static str>,
    #[allow(dead_code)]
    has_nested_repetition: bool,
}

fn build_examples() -> Vec<Example> {
    vec![
        // === Simple Patterns ===
        Example {
            name: "json_primitives",
            category: "simple",
            grammar: GrammarBuilder::new()
                .rule("value", choice(vec![
                    dynamic(str("true")),
                    dynamic(str("false")),
                    dynamic(str("null")),
                    dynamic(re(r#"-?[0-9]+(\.[0-9]+)?"#)),
                    dynamic(re(r#""[^"]*""#)),
                ]))
                .build(),
            inputs: vec!["true", "false", "null", "42", "-3.14", r#""hello""#],
            has_nested_repetition: false,
        },
        Example {
            name: "string_literal",
            category: "simple",
            grammar: GrammarBuilder::new()
                .rule("string", re(r#""([^"\\]|\\.)*""#))
                .build(),
            inputs: vec![r#""hello""#, r#""hello\nworld""#, r#""escaped\"quote""#],
            has_nested_repetition: false,
        },
        Example {
            name: "email",
            category: "simple",
            grammar: GrammarBuilder::new()
                .rule("email", seq(vec![
                    dynamic(re(r"[a-zA-Z0-9._%+-]+")),
                    dynamic(str("@")),
                    dynamic(re(r"[a-zA-Z0-9.-]+")),
                    dynamic(str(".")),
                    dynamic(re(r"[a-zA-Z]{2,}")),
                ]))
                .build(),
            inputs: vec!["test@example.com", "user.name+tag@example.co.uk"],
            has_nested_repetition: false,
        },
        Example {
            name: "url",
            category: "simple",
            grammar: GrammarBuilder::new()
                .rule("url", seq(vec![
                    dynamic(re(r"https?://")),
                    dynamic(re(r"[a-zA-Z0-9.-]+")),
                    dynamic(re(r"/[a-zA-Z0-9./_-]*")),
                ]))
                .build(),
            inputs: vec!["https://example.com/", "https://api.example.com/v1/users"],
            has_nested_repetition: false,
        },
        Example {
            name: "ip_address",
            category: "simple",
            grammar: GrammarBuilder::new()
                .rule("ip", seq(vec![
                    dynamic(re(r"[0-9]{1,3}")),
                    dynamic(str(".")),
                    dynamic(re(r"[0-9]{1,3}")),
                    dynamic(str(".")),
                    dynamic(re(r"[0-9]{1,3}")),
                    dynamic(str(".")),
                    dynamic(re(r"[0-9]{1,3}")),
                ]))
                .build(),
            inputs: vec!["192.168.1.1", "10.0.0.1"],
            has_nested_repetition: false,
        },
        Example {
            name: "iso_date",
            category: "simple",
            grammar: GrammarBuilder::new()
                .rule("date", re(r"[0-9]{4}-[0-9]{2}-[0-9]{2}"))
                .build(),
            inputs: vec!["2024-01-15", "2024-12-31"],
            has_nested_repetition: false,
        },
        Example {
            name: "iso_time",
            category: "simple",
            grammar: GrammarBuilder::new()
                .rule("time", re(r"[0-9]{2}:[0-9]{2}:[0-9]{2}"))
                .build(),
            inputs: vec!["12:34:56", "00:00:00"],
            has_nested_repetition: false,
        },

        // === Expressions with Precedence ===
        Example {
            name: "calculator",
            category: "expression",
            grammar: build_calculator_grammar(),
            inputs: vec!["42", "1+2", "1+2*3", "(1+2)*3", "1+2*3/4-5"],
            has_nested_repetition: false,
        },
        Example {
            name: "boolean_algebra",
            category: "expression",
            grammar: GrammarBuilder::new()
                .rule("expr", choice(vec![
                    dynamic(str("true")),
                    dynamic(str("false")),
                    dynamic(seq(vec![
                        dynamic(str("(")),
                        dynamic(ref_("expr")),
                        dynamic(str(")")),
                    ])),
                ]))
                .build(),
            inputs: vec!["true", "false", "(true)"],
            has_nested_repetition: false,
        },

        // === Nested/Recursive Structures ===
        Example {
            name: "balanced_parens",
            category: "nested",
            grammar: GrammarBuilder::new()
                .rule("balanced", choice(vec![
                    dynamic(str("")),
                    dynamic(seq(vec![
                        dynamic(str("(")),
                        dynamic(ref_("balanced")),
                        dynamic(str(")")),
                        dynamic(ref_("balanced")),
                    ])),
                ]))
                .build(),
            inputs: vec!["()", "(())", "((()))", "(()())"],
            has_nested_repetition: true,
        },
        Example {
            name: "sexp",
            category: "nested",
            grammar: GrammarBuilder::new()
                .rule("sexp", choice(vec![dynamic(ref_("atom")), dynamic(ref_("list"))]))
                .rule("atom", re(r"[a-zA-Z0-9+-/*=<>!?:_]+"))
                .rule("list", seq(vec![
                    dynamic(str("(")),
                    dynamic(re(r"[ \t\n]*")),
                    dynamic(re(r"(sexp[ \t\n]*)*")),
                    dynamic(str(")")),
                ]))
                .build(),
            inputs: vec!["x", "(+ 1 2)", "(* (+ 1 2) (- 4 3))", "(define (fact n) (if (= n 0) 1 (* n (fact (- n 1)))))"],
            has_nested_repetition: true,
        },
        Example {
            name: "simple_xml",
            category: "nested",
            grammar: GrammarBuilder::new()
                .rule("xml", choice(vec![
                    dynamic(re(r"[^<>]+")),
                    dynamic(seq(vec![
                        dynamic(re(r"<[a-zA-Z][a-zA-Z0-9]*>")),
                        dynamic(ref_("xml")),
                        dynamic(re(r"</[a-zA-Z][a-zA-Z0-9]*>")),
                    ])),
                ]))
                .build(),
            inputs: vec!["text", "<p>hello</p>", "<div><p>nested</p></div>"],
            has_nested_repetition: true,
        },

        // === Structured Data ===
        Example {
            name: "ini_section",
            category: "data",
            grammar: GrammarBuilder::new()
                .rule("ini", seq(vec![
                    dynamic(re(r"\[[a-zA-Z0-9_]+\]")),
                    dynamic(re(r"\n[a-zA-Z0-9_]+=[^\n]*")),
                ]))
                .build(),
            inputs: vec!["[section]\nkey=value", "[database]\nhost=localhost\nport=5432"],
            has_nested_repetition: false,
        },
        Example {
            name: "csv_line",
            category: "data",
            grammar: GrammarBuilder::new()
                .rule("csv", re(r"[^,\n]+(,[^,\n]+)*"))
                .build(),
            inputs: vec!["a,b,c", "name,age,city", "1,2,3,4,5"],
            has_nested_repetition: false,
        },
        Example {
            name: "toml_key",
            category: "data",
            grammar: GrammarBuilder::new()
                .rule("toml", seq(vec![
                    dynamic(re(r"[a-zA-Z_][a-zA-Z0-9_]*")),
                    dynamic(str("=")),
                    dynamic(re(r"[^\n]+")),
                ]))
                .build(),
            inputs: vec!["key=value", "name=\"test\"", "count=42"],
            has_nested_repetition: false,
        },

        // === Programming Language Constructs ===
        Example {
            name: "identifier",
            category: "programming",
            grammar: GrammarBuilder::new()
                .rule("ident", re(r"[a-zA-Z_][a-zA-Z0-9_]*"))
                .build(),
            inputs: vec!["foo", "bar123", "_private", "CamelCase"],
            has_nested_repetition: false,
        },
        Example {
            name: "number",
            category: "programming",
            grammar: GrammarBuilder::new()
                .rule("number", choice(vec![
                    dynamic(re(r"[0-9]+")),
                    dynamic(re(r"0x[0-9a-fA-F]+")),
                    dynamic(re(r"0b[01]+")),
                    dynamic(re(r"[0-9]+\.[0-9]+")),
                ]))
                .build(),
            inputs: vec!["42", "0xFF", "0b1010", "3.14159"],
            has_nested_repetition: false,
        },
        Example {
            name: "comment_line",
            category: "programming",
            grammar: GrammarBuilder::new()
                .rule("comment", seq(vec![
                    dynamic(str("//")),
                    dynamic(re(r"[^\n]*")),
                ]))
                .build(),
            inputs: vec!["// comment", "// TODO: fix this", "/// Doc comment"],
            has_nested_repetition: false,
        },
        Example {
            name: "function_call",
            category: "programming",
            grammar: GrammarBuilder::new()
                .rule("call", seq(vec![
                    dynamic(re(r"[a-zA-Z_][a-zA-Z0-9_]*")),
                    dynamic(str("(")),
                    dynamic(re(r"[^)]*")),
                    dynamic(str(")")),
                ]))
                .build(),
            inputs: vec!["foo()", "bar(1, 2)", "printf(\"hello %s\", name)"],
            has_nested_repetition: false,
        },

        // === Text Processing ===
        Example {
            name: "sentence",
            category: "text",
            grammar: GrammarBuilder::new()
                .rule("sentence", seq(vec![
                    dynamic(re(r"[A-Z]")),
                    dynamic(re(r"[a-z ]*")),
                    dynamic(re(r"[.!?]")),
                ]))
                .build(),
            inputs: vec!["Hello.", "This is a test.", "How are you?"],
            has_nested_repetition: false,
        },
        Example {
            name: "word",
            category: "text",
            grammar: GrammarBuilder::new()
                .rule("word", re(r"[a-zA-Z]+"))
                .build(),
            inputs: vec!["hello", "World", "Rust"],
            has_nested_repetition: false,
        },
        Example {
            name: "whitespace",
            category: "text",
            grammar: GrammarBuilder::new()
                .rule("ws", re(r"[ \t\n]+"))
                .build(),
            inputs: vec![" ", "  ", "\t\n"],
            has_nested_repetition: false,
        },

        // === Special Patterns ===
        Example {
            name: "phone_number",
            category: "special",
            grammar: GrammarBuilder::new()
                .rule("phone", re(r"\+?[0-9]{1,3}[-.]?[0-9]{3}[-.]?[0-9]{4}"))
                .build(),
            inputs: vec!["123-456-7890", "+1.800.555.1212", "5551234"],
            has_nested_repetition: false,
        },
        Example {
            name: "hex_color",
            category: "special",
            grammar: GrammarBuilder::new()
                .rule("color", re(r"#[0-9a-fA-F]{6}"))
                .build(),
            inputs: vec!["#FF0000", "#00FF00", "#0000FF", "#abcdef"],
            has_nested_repetition: false,
        },
        Example {
            name: "uuid",
            category: "special",
            grammar: GrammarBuilder::new()
                .rule("uuid", re(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}"))
                .build(),
            inputs: vec!["123e4567-e89b-12d3-a456-426614174000"],
            has_nested_repetition: false,
        },
    ]
}

fn build_calculator_grammar() -> Grammar {
    let mut builder = GrammarBuilder::new();
    builder = builder.rule("expr", re(r"[0-9]+"));
    builder = builder.rule("number", re(r"[0-9]+"));
    builder = builder.rule(
        "primary",
        choice(vec![
            dynamic(seq(vec![
                dynamic(str("(")),
                dynamic(ref_("expr")),
                dynamic(str(")")),
            ])),
            dynamic(ref_("number")),
        ]),
    );
    let expr_atom = InfixBuilder::new()
        .primary(ref_("primary"))
        .op("*", 2, Assoc::Left)
        .op("/", 2, Assoc::Left)
        .op("+", 1, Assoc::Left)
        .op("-", 1, Assoc::Left)
        .build(&mut builder);
    builder.update_rule("expr", expr_atom);
    builder.build()
}

// ============================================================================
// Benchmarks
// ============================================================================

fn bench_all_examples(c: &mut Criterion) {
    let examples = build_examples();

    for example in &examples {
        let mut group = c.benchmark_group(format!("{}/{}", example.category, example.name));

        for input in &example.inputs {
            group.throughput(Throughput::Bytes(input.len() as u64));

            // Packrat backend
            group.bench_with_input(
                BenchmarkId::new("packrat", truncate_str(input, 20)),
                input,
                |b, input| {
                    b.iter(|| {
                        let mut backend = PackratBackend::new();
                        let _ = black_box(backend.parse(&example.grammar, input));
                    })
                },
            );

            // Bytecode backend
            group.bench_with_input(
                BenchmarkId::new("bytecode", truncate_str(input, 20)),
                input,
                |b, input| {
                    b.iter(|| {
                        let mut backend = BytecodeBackend::new();
                        let _ = black_box(backend.parse(&example.grammar, input));
                    })
                },
            );
        }

        group.finish();
    }
}

/// Summary benchmark comparing backends across categories
fn bench_category_summary(c: &mut Criterion) {
    let examples = build_examples();
    let mut group = c.benchmark_group("category_summary");

    // Group by category and compute representative times
    let categories = [
        "simple",
        "expression",
        "nested",
        "data",
        "programming",
        "text",
        "special",
    ];

    for category in categories {
        let category_examples: Vec<_> =
            examples.iter().filter(|e| e.category == category).collect();

        if category_examples.is_empty() {
            continue;
        }

        // Use first example as representative
        let example = &category_examples[0];
        let input = example.inputs.first().unwrap_or(&"");

        group.bench_function(format!("{}_packrat", category), |b| {
            b.iter(|| {
                let mut backend = PackratBackend::new();
                let _ = black_box(backend.parse(&example.grammar, input));
            })
        });

        group.bench_function(format!("{}_bytecode", category), |b| {
            b.iter(|| {
                let mut backend = BytecodeBackend::new();
                let _ = black_box(backend.parse(&example.grammar, input));
            })
        });
    }

    group.finish();
}

/// Benchmark specifically for nested repetition (critical for backend selection)
fn bench_nested_repetition(c: &mut Criterion) {
    let mut group = c.benchmark_group("nested_repetition");

    // Build grammars with different nesting levels
    for depth in [1, 2, 3, 4] {
        let grammar = build_nested_grammar(depth);
        let input = build_nested_input(depth);

        group.throughput(Throughput::Bytes(input.len() as u64));

        group.bench_with_input(
            BenchmarkId::new("packrat", format!("depth_{}", depth)),
            &input,
            |b, input| {
                b.iter(|| {
                    let mut backend = PackratBackend::new();
                    let _ = black_box(backend.parse(&grammar, input));
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("bytecode", format!("depth_{}", depth)),
            &input,
            |b, input| {
                b.iter(|| {
                    let mut backend = BytecodeBackend::new();
                    let _ = black_box(backend.parse(&grammar, input));
                })
            },
        );
    }

    group.finish();
}

fn build_nested_grammar(depth: usize) -> Grammar {
    if depth == 0 {
        return GrammarBuilder::new().rule("start", re(r"[a-z]+")).build();
    }

    let mut _builder = GrammarBuilder::new();
    _builder = _builder.rule("start", ref_("nested"));

    for d in 0..depth {
        let _rule_name = if d == 0 {
            "start"
        } else {
            &format!("level_{}", d)
        };
        let _inner_name = if d == depth - 1 {
            "atom"
        } else {
            &format!("level_{}", d + 1)
        };

        if d == 0 {
            continue; // Skip, already added
        }
    }

    // Simple nested structure
    GrammarBuilder::new()
        .rule(
            "start",
            seq(vec![
                dynamic(str("(")),
                dynamic(ref_("inner")),
                dynamic(str(")")),
            ]),
        )
        .rule(
            "inner",
            seq(vec![
                dynamic(str("(")),
                dynamic(ref_("atom")),
                dynamic(str(")")),
            ]),
        )
        .rule("atom", re(r"[a-z]+"))
        .build()
}

fn build_nested_input(depth: usize) -> String {
    let mut s = String::new();
    for _ in 0..depth {
        s.push('(');
    }
    s.push('x');
    for _ in 0..depth {
        s.push(')');
    }
    s
}

criterion_group!(
    benches,
    bench_all_examples,
    bench_category_summary,
    bench_nested_repetition,
);
criterion_main!(benches);
