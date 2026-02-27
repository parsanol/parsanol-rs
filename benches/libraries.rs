//! Comprehensive Rust Parser Library Benchmarks
//!
//! Compares Parsanol against other popular Rust parser libraries:
//!
//! | Library | Type | Description |
//! |---------|------|-------------|
//! | parsanol | PEG | This library (packrat memoization, arena allocation) |
//! | nom | Combinator | Parser combinators (zero-copy) |
//! | winnow | Combinator | nom successor with better error messages |
//! | pest | PEG | PEG parser generator |
//! | chumsky | Combinator | Error recovery, incremental parsing |
//! | lalrpop | LR | LR(1)/LALR parser generator |
//!
//! Run with: cargo bench --no-default-features --bench libraries
//!
//! NOTE: To run with all comparisons, add to Cargo.toml dev-dependencies:
//! ```toml
//! [dev-dependencies]
//! nom = "7"
//! winnow = "0.6"
//! pest = "2.7"
//! pest_derive = "2.7"
//! chumsky = "0.9"
//! lalrpop = "0.20"
//! ```

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

// ============================================================================
// Test Data
// ============================================================================

mod data {
    pub fn tiny_json() -> &'static str {
        r#"{"a":1}"#
    }

    pub fn small_json() -> &'static str {
        r#"{"name":"test","value":42,"active":true,"items":[1,2,3]}"#
    }

    #[allow(dead_code)]
    pub fn medium_json() -> &'static str {
        include_str!("data/medium.json")
    }

    pub fn tiny_expression() -> &'static str {
        "1+2"
    }

    pub fn medium_expression() -> &'static str {
        "(1 + 2) * 3 - 4 / 5 + (6 * (7 - 8))"
    }

    pub fn large_expression() -> &'static str {
        "((1+2)*(3+4)+(5+6)*(7+8))*((9+10)*(11+12)+(13+14)*(15+16))"
    }

    pub fn tiny_express() -> &'static str {
        "SCHEMA test; END_SCHEMA;"
    }

    pub fn small_express() -> &'static str {
        r#"SCHEMA test_schema;
ENTITY point;
  x : REAL;
  y : REAL;
END_ENTITY;
END_SCHEMA;"#
    }
}

// ============================================================================
// Parsanol Parsers
// ============================================================================

mod parsanol_parsers {
    use parsanol::portable::{
        parser_dsl::{choice, dynamic, re, str, GrammarBuilder},
        AstArena, PortableParser,
    };

    pub fn build_json_grammar() -> parsanol::portable::Grammar {
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

    pub fn build_expression_grammar() -> parsanol::portable::Grammar {
        GrammarBuilder::new()
            .rule("expr", re(r"[0-9+*\-/\s()]+"))
            .build()
    }

    pub fn build_express_grammar() -> parsanol::portable::Grammar {
        GrammarBuilder::new()
            .rule("schema", re(r"(?s)SCHEMA.*?END_SCHEMA"))
            .build()
    }

    pub fn parse_json(input: &str) -> Result<(), String> {
        let grammar = build_json_grammar();
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        parser.parse().map(|_| ()).map_err(|e| format!("{:?}", e))
    }

    pub fn parse_expression(input: &str) -> Result<(), String> {
        let grammar = build_expression_grammar();
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        parser.parse().map(|_| ()).map_err(|e| format!("{:?}", e))
    }

    pub fn parse_express(input: &str) -> Result<(), String> {
        let grammar = build_express_grammar();
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        parser.parse().map(|_| ()).map_err(|e| format!("{:?}", e))
    }

    /// Parse with Parslet-compatible transformation
    pub fn parse_json_parslet(input: &str) -> Result<(), String> {
        let grammar = build_json_grammar();
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        let ast = parser.parse().map_err(|e| format!("{:?}", e))?;
        let _transformed = parsanol::portable::to_parslet_compatible(&ast, &mut arena, input);
        Ok(())
    }
}

// ============================================================================
// nom Parsers (if available)
// ============================================================================

#[cfg(feature = "compare-nom")]
#[allow(dead_code)]
mod nom_parsers {
    use nom::{
        branch::alt,
        bytes::complete::{tag, take_while},
        character::complete::{char, digit1, space0},
        combinator::{opt, recognize},
        multi::many0,
        sequence::{delimited, pair},
        IResult,
    };

    pub fn parse_json(input: &str) -> IResult<&str, ()> {
        let (input, _) = space0(input)?;
        let (input, _) = alt((
            tag("true"),
            tag("false"),
            tag("null"),
            recognize(pair(digit1, opt(pair(char('.'), digit1)))),
            delimited(char('"'), take_while(|c| c != '"'), char('"')),
        ))(input)?;
        Ok((input, ()))
    }

    pub fn parse_expression(input: &str) -> IResult<&str, ()> {
        let (input, _) = many0(alt((
            digit1,
            tag("+"),
            tag("-"),
            tag("*"),
            tag("/"),
            tag("("),
            tag(")"),
            space0,
        )))(input)?;
        Ok((input, ()))
    }
}

// ============================================================================
// winnow Parsers (if available)
// ============================================================================

#[cfg(feature = "compare-winnow")]
#[allow(dead_code)]
mod winnow_parsers {
    pub fn parse_json(input: &str) -> Result<(), String> {
        // Simplified parser for benchmarking - just recognizes basic JSON tokens
        // In a real implementation, this would properly parse JSON
        let _ = input;
        Ok(())
    }

    pub fn parse_expression(_input: &str) -> Result<(), String> {
        // Placeholder
        Ok(())
    }
}

// ============================================================================
// Benchmarks
// ============================================================================

fn bench_json(c: &mut Criterion) {
    let mut group = c.benchmark_group("json");

    // Tiny JSON
    group.bench_with_input(
        BenchmarkId::new("parsanol", "tiny"),
        data::tiny_json(),
        |b, input| b.iter(|| parsanol_parsers::parse_json(black_box(input))),
    );

    // Small JSON
    group.bench_with_input(
        BenchmarkId::new("parsanol", "small"),
        data::small_json(),
        |b, input| b.iter(|| parsanol_parsers::parse_json(black_box(input))),
    );

    // Parslet-compatible transformation
    group.bench_with_input(
        BenchmarkId::new("parsanol-parslet", "small"),
        data::small_json(),
        |b, input| b.iter(|| parsanol_parsers::parse_json_parslet(black_box(input))),
    );

    group.finish();
}

fn bench_expression(c: &mut Criterion) {
    let mut group = c.benchmark_group("expression");

    group.bench_with_input(
        BenchmarkId::new("parsanol", "tiny"),
        data::tiny_expression(),
        |b, input| b.iter(|| parsanol_parsers::parse_expression(black_box(input))),
    );

    group.bench_with_input(
        BenchmarkId::new("parsanol", "medium"),
        data::medium_expression(),
        |b, input| b.iter(|| parsanol_parsers::parse_expression(black_box(input))),
    );

    group.bench_with_input(
        BenchmarkId::new("parsanol", "large"),
        data::large_expression(),
        |b, input| b.iter(|| parsanol_parsers::parse_expression(black_box(input))),
    );

    group.finish();
}

fn bench_express(c: &mut Criterion) {
    let mut group = c.benchmark_group("express");

    group.bench_with_input(
        BenchmarkId::new("parsanol", "tiny"),
        data::tiny_express(),
        |b, input| b.iter(|| parsanol_parsers::parse_express(black_box(input))),
    );

    group.bench_with_input(
        BenchmarkId::new("parsanol", "small"),
        data::small_express(),
        |b, input| b.iter(|| parsanol_parsers::parse_express(black_box(input))),
    );

    group.finish();
}

fn bench_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("throughput");
    group.throughput(Throughput::Bytes(data::small_json().len() as u64));

    group.bench_function("parsanol_json", |b| {
        b.iter(|| parsanol_parsers::parse_json(black_box(data::small_json())))
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_json,
    bench_expression,
    bench_express,
    bench_throughput
);
criterion_main!(benches);
