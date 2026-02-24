//! Comprehensive Benchmarks comparing Serialized vs Native Transform Modes
//!
//! THREE examples are benchmarked:
//! 1. JSON Parser - parse primitive values
//! 2. Calculator - parse expressions with operators
//! 3. CSV Parser - parse tabular data
//!
//! Serialized Mode: Parse → Serialize to JSON (for FFI to host language)
//! Native Mode: Parse + Transform in Rust (no serialization)
//!
//! Expected: Native Mode is faster because no serialization overhead
//!
//! Run with: cargo bench --no-default-features --bench comparison

use criterion::{black_box, criterion_group, criterion_main, Criterion};

// ============================================================================
// Example 1: JSON Parser
// ============================================================================

mod json {
    use parsanol::portable::{
        parser_dsl::{choice, dynamic, re, str, GrammarBuilder},
        AstArena, AstNode, PortableParser,
    };
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;

    pub fn build_grammar() -> parsanol::portable::Grammar {
        GrammarBuilder::new()
            .rule(
                "json",
                choice(vec![
                    dynamic(str("true")),
                    dynamic(str("false")),
                    dynamic(str("null")),
                    dynamic(re(r#"-?[0-9]+(\.[0-9]+)?"#)),
                    dynamic(re(r#""[^"]*""#)),
                ]),
            )
            .build()
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum JsonValue {
        Null,
        Bool(bool),
        Number(f64),
        String(String),
        Array(Vec<JsonValue>),
        Object(HashMap<String, JsonValue>),
    }

    // Serialized Mode: Parse → Serialize
    pub fn parse_serialize(input: &str) -> Result<String, String> {
        let grammar = build_grammar();
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        let ast = parser.parse().map_err(|e| format!("{:?}", e))?;
        let value = ast_to_json(&ast, &arena, input)?;
        serde_json::to_string(&value).map_err(|e| e.to_string())
    }

    fn ast_to_json(node: &AstNode, arena: &AstArena, input: &str) -> Result<JsonValue, String> {
        match node {
            AstNode::InputRef { offset, length } => {
                let s = &input[*offset as usize..*offset as usize + *length as usize];
                match s {
                    "true" => Ok(JsonValue::Bool(true)),
                    "false" => Ok(JsonValue::Bool(false)),
                    "null" => Ok(JsonValue::Null),
                    _ => {
                        if s.starts_with('"') && s.ends_with('"') {
                            Ok(JsonValue::String(s[1..s.len() - 1].to_string()))
                        } else if let Ok(n) = s.parse::<f64>() {
                            Ok(JsonValue::Number(n))
                        } else {
                            Err(format!("Unknown: {}", s))
                        }
                    }
                }
            }
            AstNode::Nil => Ok(JsonValue::Null),
            AstNode::Bool(b) => Ok(JsonValue::Bool(*b)),
            AstNode::Int(n) => Ok(JsonValue::Number(*n as f64)),
            AstNode::Float(f) => Ok(JsonValue::Number(*f)),
            AstNode::StringRef { pool_index } => Ok(JsonValue::String(
                arena.get_string(*pool_index as usize).to_string(),
            )),
            _ => Ok(JsonValue::Null),
        }
    }

    // Native Mode: Parse + Transform (no serialize)
    pub fn parse_transform(input: &str) -> Result<JsonValue, String> {
        let grammar = build_grammar();
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        let ast = parser.parse().map_err(|e| format!("{:?}", e))?;
        ast_to_json(&ast, &arena, input)
    }
}

// ============================================================================
// Example 2: Calculator
// ============================================================================

mod calc {
    use parsanol::portable::{
        infix::{Assoc, InfixBuilder},
        parser_dsl::{choice, dynamic, re, ref_, seq, str, GrammarBuilder},
        AstArena, AstNode, Grammar, PortableParser,
    };
    use serde::{Deserialize, Serialize};

    pub fn build_grammar() -> Grammar {
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

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Expr {
        pub op: String,
        pub args: Vec<i64>,
    }

    // Serialized Mode: Parse → Serialize
    pub fn parse_serialize(input: &str) -> Result<String, String> {
        let grammar = build_grammar();
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        let ast = parser.parse().map_err(|e| format!("{:?}", e))?;
        let expr = ast_to_expr(&ast, &arena, input)?;
        serde_json::to_string(&expr).map_err(|e| e.to_string())
    }

    fn ast_to_expr(node: &AstNode, arena: &AstArena, input: &str) -> Result<Expr, String> {
        match node {
            AstNode::InputRef { offset, length } => {
                let s = &input[*offset as usize..*offset as usize + *length as usize];
                if s.chars().all(|c| c.is_ascii_digit()) {
                    let n = s.parse::<i64>().map_err(|_| "parse error")?;
                    Ok(Expr {
                        op: "num".into(),
                        args: vec![n],
                    })
                } else {
                    // Operator or other token
                    Ok(Expr {
                        op: s.to_string(),
                        args: vec![],
                    })
                }
            }
            AstNode::Array { pool_index, length } => {
                let items = arena.get_array(*pool_index as usize, *length as usize);
                let mut args = vec![];
                let mut op = "expr".to_string();

                for item in &items {
                    if let Ok(e) = ast_to_expr(item, arena, input) {
                        if e.op == "num" {
                            args.extend(e.args);
                        } else if e.op != "expr" && !e.op.is_empty() {
                            // It's an operator
                            if e.op != "+" && e.op != "-" && e.op != "*" && e.op != "/" {
                                // It's a number-like expression
                                if !e.args.is_empty() {
                                    args.extend(e.args);
                                }
                            } else {
                                op = e.op;
                            }
                        } else if !e.args.is_empty() {
                            args.extend(e.args);
                        }
                    }
                }
                Ok(Expr { op, args })
            }
            _ => Ok(Expr {
                op: "num".into(),
                args: vec![],
            }),
        }
    }

    // Native Mode: Parse + Transform (no serialize)
    pub fn parse_transform(input: &str) -> Result<Expr, String> {
        let grammar = build_grammar();
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        let ast = parser.parse().map_err(|e| format!("{:?}", e))?;
        ast_to_expr(&ast, &arena, input)
    }
}

// ============================================================================
// Example 3: CSV Parser
// ============================================================================

mod csv {
    use parsanol::portable::{
        parser_dsl::{re, GrammarBuilder},
        AstArena, PortableParser,
    };
    use serde::{Deserialize, Serialize};

    pub fn build_grammar() -> parsanol::portable::Grammar {
        GrammarBuilder::new()
            .rule("csv", re(r"(?s).*")) // Match entire input including newlines
            .build()
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CsvRow(pub Vec<String>);

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CsvData {
        pub headers: Vec<String>,
        pub rows: Vec<CsvRow>,
    }

    // Serialized Mode: Parse → Serialize
    pub fn parse_serialize(input: &str) -> Result<String, String> {
        let grammar = build_grammar();
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        let _ast = parser.parse().map_err(|e| format!("{:?}", e))?;
        let data = parse_csv(input)?;
        serde_json::to_string(&data).map_err(|e| e.to_string())
    }

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

    // Native Mode: Parse + Transform (no serialize)
    pub fn parse_transform(input: &str) -> Result<CsvData, String> {
        let grammar = build_grammar();
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        let _ast = parser.parse().map_err(|e| format!("{:?}", e))?;
        parse_csv(input)
    }
}

// ============================================================================
// Benchmarks
// ============================================================================

fn bench_json(c: &mut Criterion) {
    let mut g = c.benchmark_group("json");
    // JSON tests
    g.bench_function("A_serialize_null", |b| {
        b.iter(|| json::parse_serialize(black_box("null")))
    });
    g.bench_function("B_direct_null", |b| {
        b.iter(|| json::parse_transform(black_box("null")))
    });
    g.bench_function("A_serialize_42", |b| {
        b.iter(|| json::parse_serialize(black_box("42")))
    });
    g.bench_function("B_direct_42", |b| {
        b.iter(|| json::parse_transform(black_box("42")))
    });
    g.finish();
}

fn bench_calc(c: &mut Criterion) {
    let mut g = c.benchmark_group("calculator");
    g.bench_function("A_serialize_simple", |b| {
        b.iter(|| calc::parse_serialize(black_box("42")))
    });
    g.bench_function("B_direct_simple", |b| {
        b.iter(|| calc::parse_transform(black_box("42")))
    });
    g.bench_function("A_serialize_add", |b| {
        b.iter(|| calc::parse_serialize(black_box("1+2")))
    });
    g.bench_function("B_direct_add", |b| {
        b.iter(|| calc::parse_transform(black_box("1+2")))
    });
    g.bench_function("A_serialize_complex", |b| {
        b.iter(|| calc::parse_serialize(black_box("1+2*3")))
    });
    g.bench_function("B_direct_complex", |b| {
        b.iter(|| calc::parse_transform(black_box("1+2*3")))
    });
    g.finish();
}

fn bench_csv(c: &mut Criterion) {
    let mut g = c.benchmark_group("csv");
    let input1 = "name,age\nAlice,30";
    let input2 = "name,age,city,phone\nAlice,30,NYC,555-1234\nBob,25,LA,555-5678";
    g.bench_function("A_serialize_small", |b| {
        b.iter(|| csv::parse_serialize(black_box(input1)))
    });
    g.bench_function("B_direct_small", |b| {
        b.iter(|| csv::parse_transform(black_box(input1)))
    });
    g.bench_function("A_serialize_large", |b| {
        b.iter(|| csv::parse_serialize(black_box(input2)))
    });
    g.bench_function("B_direct_large", |b| {
        b.iter(|| csv::parse_transform(black_box(input2)))
    });
    g.finish();
}

fn bench_summary(c: &mut Criterion) {
    let mut g = c.benchmark_group("summary");
    // Total workload - mix of all three parsers
    g.bench_function("A_total_serialize", |b| {
        b.iter(|| {
            let _ = json::parse_serialize("null");
            let _ = json::parse_serialize("42");
            let _ = calc::parse_serialize("1+2");
            let _ = calc::parse_serialize("1+2*3");
            let _ = csv::parse_serialize("name,age\nAlice,30");
        })
    });
    g.bench_function("B_total_direct", |b| {
        b.iter(|| {
            let _ = json::parse_transform("null");
            let _ = json::parse_transform("42");
            let _ = calc::parse_transform("1+2");
            let _ = calc::parse_transform("1+2*3");
            let _ = csv::parse_transform("name,age\nAlice,30");
        })
    });
    g.finish();
}

criterion_group!(benches, bench_json, bench_calc, bench_csv, bench_summary);
criterion_main!(benches);
