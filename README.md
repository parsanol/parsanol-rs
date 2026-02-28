# Parsanol-rs

A high-performance PEG (Parsing Expression Grammar) parser library for Rust with packrat memoization and arena allocation.

[![Crates.io](https://img.shields.io/crates/v/parsanol.svg)](https://crates.io/crates/parsanol)
[![Documentation](https://docs.rs/parsanol/badge.svg)](https://docs.rs/parsanol)
[![License](https://img.shields.io/github/license/parsanol/parsanol-rs.svg)](https://github.com/parsanol/parsanol-rs/blob/main/LICENSE)
[![CI](https://github.com/parsanol/parsanol-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/parsanol/parsanol-rs/actions/workflows/ci.yml)

## Purpose

Parsanol-rs is a generic, domain-agnostic PEG parser library written in
Rust. It provides high-performance parsing capabilities with a focus on:

- **Speed**: Packrat memoization for O(n) parsing complexity

- **Memory efficiency**: Arena allocation for zero-copy AST construction

- **Developer experience**: Fluent API for building grammars, rich error
  reporting

- **Flexibility**: Transform system for converting parse trees to typed
  Rust structs

## Features

- [Quick Start](#quick-start) - Get started in minutes
- [Parser DSL](#parser-dsl) - Fluent API for grammar definition
- [Transform System](#transform-system) - Convert parse trees to typed structs
- [Streaming Builder](#streaming-builder) - Single-pass parsing with custom output
- [Parallel Parsing](#parallel-parsing) - Multi-file parsing with rayon
- [Infix Expression Parsing](#infix-expression-parsing) - Built-in operator precedence
- [Rich Error Reporting](#rich-error-reporting) - Tree-structured error messages
- [Source Location Tracking](#source-location-tracking) - Line/column tracking through transforms
- [Grammar Composition](#grammar-composition) - Import and compose grammars
- [Ruby FFI](#ruby-ffi) - Optional Ruby bindings
- [WASM Support](#wasm-support) - Optional WebAssembly bindings

# Architecture

    ┌─────────────────────────────────────────────────────────────┐
    │                    PARSANOL-RS                              │
    │              (Generic PEG Parser Library)                   │
    ├─────────────────────────────────────────────────────────────┤
    │  • Parser combinators (PEG atoms)                           │
    │  • Grammar representation                                   │
    │  • Packrat memoization                                      │
    │  • Arena allocation                                         │
    │  • Infix expression parsing                                 │
    │  • Rich error reporting (tree structure)                    │
    │  • Transform DSL (pattern matching)                         │
    │  • Optional Ruby FFI / WASM bindings                        │
    └─────────────────────────────────────────────────────────────┘
              ▲                                    ▲
              │ (build ON TOP)                     │ (build ON TOP)
              │                                    │
    ┌─────────┴──────────┐               ┌─────────┴─────────┐
    │   parsanol-express │               │   Your Language   │
    │   (EXPRESS lexer)  │               │   (Your DSL)      │
    └────────────────────┘               └───────────────────┘

> [!IMPORTANT]
> Parsanol-rs is a **GENERIC** parser library. It has no knowledge of
> any specific domain (EXPRESS, Ruby, JSON, YAML, etc.). Domain-specific
> parsers should be built ON TOP of this library.

# Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
parsanol = "0.1"
```

## Optional Features

- `ruby` - Enable Ruby FFI bindings (requires `magnus`, `rb-sys`)

- `wasm` - Enable WebAssembly bindings (requires `wasm-bindgen`,
  `js-sys`)

- `parallel` - Enable parallel parsing (requires `rayon`)

```toml
[dependencies]
parsanol = { version = "0.1", features = ["ruby", "parallel"] }
```

# Quick Start

## Basic Parsing

```rust
use parsanol::portable::{Grammar, PortableParser, AstArena, parser_dsl::*};

// Build a simple grammar
let grammar = GrammarBuilder::new()
    .rule("greeting", str("hello").then(str("world")))
    .build();

let input = "helloworld";
let mut arena = AstArena::for_input(input.len());
let mut parser = PortableParser::new(&grammar, input, &mut arena);

match parser.parse() {
    Ok(ast) => println!("Parsed successfully: {:?}", ast),
    Err(e) => println!("Parse error: {:?}", e),
}
```

## Calculator with Operator Precedence

```rust
use parsanol::portable::{
    GrammarBuilder, PortableParser, AstArena, Grammar,
    parser_dsl::{str, re, ref_, seq, choice, dynamic},
    infix::{InfixBuilder, Assoc},
};

fn build_calculator_grammar() -> Grammar {
    let mut builder = GrammarBuilder::new();

    // Define atoms
    builder = builder.rule("number", re(r"[0-9]+"));
    builder = builder.rule("primary", choice(vec![
        dynamic(seq(vec![
            dynamic(str("(")),
            dynamic(ref_("expr")),
            dynamic(str(")")),
        ])),
        dynamic(ref_("number")),
    ]));

    // Build infix with precedence
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
```

# Parser DSL

## Atom Types

| Atom | Description | Example |
|----|----|----|
| `str("literal")` | Match exact string | `str("hello")` |
| `re("pattern")` | Match regex pattern | `re(r"[0-9]+")` |
| `any()` | Match any single character | `any()` |
| `ref_("rule")` | Reference to named rule | `ref_("expr")` |
| `seq([…])` | Sequence of atoms | `seq(vec![a,` `b,` `c])` |
| `choice([…])` | Alternative atoms | `choice(vec![a,` `b])` |
| `cut()` | Commit to this branch (prevent backtracking) | `cut()` |

## Combinators

All atoms implement the `ParsletExt` trait with these methods:

```rust
use parsanol::portable::parser_dsl::*;

// Sequence: A >> B
let parser = str("hello").then(str("world"));

// Alternative: A | B
let parser = str("foo").or(str("bar"));

// Repetition
let parser = str("a").repeat(1, None);    // One or more
let parser = str("a").repeat(0, Some(3)); // Zero to three
let parser = str("a").many();              // Zero or more
let parser = str("a").many1();             // One or more
let parser = str("a").optional();          // Zero or one

// Named capture
let parser = re(r"[0-9]+").label("number");

// Ignore (don't include in AST)
let parser = str(" ").ignore();

// Lookahead (don't consume)
let parser = str("hello").lookahead();     // Positive: must match
let parser = str("hello").not_ahead();     // Negative: must NOT match
```

## Grammar Macro

For declarative grammar definition:

```rust
use parsanol::portable::parser_dsl::grammar;

let grammar = grammar! {
    "hello" => str("hello"),
    "world" => str("world"),
    "greeting" => ref_("hello").then(ref_("world")),
};
```

# Transform System

The transform system converts generic parse trees into typed Rust data
structures, similar to Parslet’s transformation system.

## Value Types

The `Value` enum represents transformed data:

```rust
pub enum Value {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Array(Vec<Value>),
    Hash(HashMap<String, Value>),
}
```

## Basic Transformations

```rust
use parsanol::portable::transform::{Transform, Value, TransformError};

let transform = Transform::new()
    // Transform "int" captures by doubling
    .rule("int", |v| {
        let n = v.as_int().ok_or_else(|| TransformError::Custom("not int".into()))?;
        Ok(Value::int(n * 2))
    });

let value = Value::hash(vec![("int", Value::int(21))]);
let result = transform.apply(&value)?;
assert_eq!(result.as_int(), Some(42));
```

## Pattern Matching

Pattern-based transformations similar to Parslet:

```rust
use parsanol::portable::transform::{Transform, Pattern, Value};

let transform = Transform::new()
    // Match hash with specific fields
    .pattern(
        Pattern::hash()
            .field("left", "l")
            .field("op", Pattern::str("+"))
            .field("right", "r"),
        |bindings| {
            let l = bindings.get_int("l")?;
            let r = bindings.get_int("r")?;
            Ok(Value::int(l + r))
        }
    );
```

## Pattern Types

| Pattern | Description | Example |
|----|----|----|
| `Pattern::simple("x")` | Match any leaf value and bind to variable | `Pattern::simple("n")` matches `42` |
| `Pattern::str("value")` | Match a specific string value | `Pattern::str("+")` matches `"+"` |
| `Pattern::int(n)` | Match a specific integer | `Pattern::int(42)` matches `42` |
| `Pattern::sequence("x")` | Match an array and bind to variable | `Pattern::sequence("items")` |
| `Pattern::subtree("x")` | Match anything and bind to variable | `Pattern::subtree("node")` |
| `Pattern::hash()` | Match a hash with specific fields | See example above |

## Converting AST to Value

```rust
use parsanol::portable::transform::{ast_to_value, Value};

// After parsing
let ast = parser.parse()?;
let value = ast_to_value(&ast, &arena, input);

// Now apply transforms
let result = transform.apply(&value)?;
```

# Streaming Builder

The streaming builder API allows single-pass parsing without
intermediate AST construction. This is ideal for:

- Maximum performance (eliminates AST allocation)

- Custom output formats

- Memory-constrained environments

## Implementing StreamingBuilder

```rust
use parsanol::portable::streaming_builder::{StreamingBuilder, BuildResult, BuildError};

// Custom builder that collects all strings
struct StringCollector {
    strings: Vec<String>,
}

impl StreamingBuilder for StringCollector {
    type Output = Vec<String>;

    fn on_string(&mut self, value: &str, _offset: usize, _length: usize) -> BuildResult<()> {
        self.strings.push(value.to_string());
        Ok(())
    }

    fn finish(&mut self) -> BuildResult<Self::Output> {
        Ok(std::mem::take(&mut self.strings))
    }
}
```

## Using parse_with_builder

```rust
use parsanol::portable::{Grammar, PortableParser, AstArena};

let grammar = /* ... */;
let input = "hello world";
let mut arena = AstArena::for_input(input.len());
let mut parser = PortableParser::new(&grammar, input, &mut arena);

// Create builder
let mut builder = StringCollector { strings: vec![] };

// Parse with streaming builder
let result = parser.parse_with_builder(&mut builder)?;
// result: Vec<String>
```

## Built-in Builders

Several useful builders are provided:

| Builder                  | Description                                  |
|--------------------------|----------------------------------------------|
| `DebugBuilder`           | Collects all events as strings for debugging |
| `BuilderStringCollector` | Collects all string values                   |
| `BuilderNodeCounter`     | Counts nodes by type                         |

## Ruby Integration

The streaming builder works with Ruby callbacks via FFI:

```ruby
require 'parsanol'

class MyBuilder
  include Parsanol::BuilderCallbacks

  def initialize
    @strings = []
  end

  def on_string(value, offset, length)
    @strings << value
  end

  def finish
    @strings
  end
end

builder = MyBuilder.new
result = Parsanol::Native.parse_with_builder(grammar_json, input, builder)
```

# Parallel Parsing

Parse multiple inputs in parallel using rayon for linear speedup on
multi-core systems.

## Enabling Parallel Feature

```toml
[dependencies]
parsanol = { version = "0.1", features = ["parallel"] }
```

## Batch Parallel Parsing

```rust
use parsanol::portable::{Grammar, parse_batch_parallel};

let grammar = /* ... */;
let inputs = vec!["file1.exp", "file2.exp", "file3.exp"];

// Parse all inputs in parallel
let results = parse_batch_parallel(&grammar, &inputs);

// Results are in same order as inputs
for (i, result) in results.iter().enumerate() {
    match result {
        Ok(ast) => println!("File {} parsed successfully", i),
        Err(e) => eprintln!("File {} failed: {}", i, e),
    }
}
```

## Parallel Configuration

```rust
use parsanol::portable::parallel::{parse_batch_parallel, ParallelConfig};

let config = ParallelConfig::new()
    .with_num_threads(4)        // Use 4 threads
    .with_min_chunk_size(10);   // Minimum inputs per thread

let results = parse_batch_parallel(&grammar, &inputs);
```

## Performance

| Scenario           | Speedup                                |
|--------------------|----------------------------------------|
| 8 cores, 100 files | ~8x faster than sequential             |
| 4 cores, 50 files  | ~4x faster than sequential             |
| Single core        | Same as sequential (graceful fallback) |

When the `parallel` feature is not enabled, the functions fall back to
sequential parsing automatically.

# Infix Expression Parsing

Built-in support for parsing infix expressions with operator precedence
and associativity.

## Using InfixBuilder

```rust
use parsanol::portable::infix::{InfixBuilder, Assoc};

let mut builder = GrammarBuilder::new();

let expr_idx = InfixBuilder::new()
    .primary(ref_("atom"))           // Base expression (numbers, parens)
    .op("*", 2, Assoc::Left)         // Higher precedence
    .op("/", 2, Assoc::Left)
    .op("+", 1, Assoc::Left)         // Lower precedence
    .op("-", 1, Assoc::Left)
    .op("^", 3, Assoc::Right)        // Right-associative
    .build(&mut builder);
```

## Associativity

| Associativity | Meaning | Example |
|----|----|----|
| `Assoc::Left` | Left-to-right evaluation | `a` `-` `b` `-` `c` = `(a` `-` `b)` `-` `c` |
| `Assoc::Right` | Right-to-left evaluation | `a` `=` `b` `=` `c` = `a` `=` `(b` `=` `c)` |
| `Assoc::NonAssoc` | Cannot chain | `a` `<` `b` `<` `c` is an error |

# Rich Error Reporting

Tree-structured error messages similar to Parslet for better debugging.

## Basic Usage

```rust
use parsanol::portable::error::{RichError, ErrorBuilder, Span};

// Create rich errors
let error = ErrorBuilder::new("Failed to parse expression")
    .at(10, 2, 5)  // offset, line, column
    .context("expression")
    .child(
        ErrorBuilder::new("Expected '+' or '-'")
            .at(10, 2, 5)
            .build(),
    )
    .build();

// Print as ASCII tree
println!("{}", error.ascii_tree());
```

## Example Output

    Error at line 3, column 5:
    `- Failed to parse expression (in expression)
       `- Expected '+' or '-'

## Source Context

```rust
// Format error with source code context
let formatted = error.format_with_source(input);
println!("{}", formatted);
```

Output:

    Error at line 3, column 5:
    let x = foo bar
                ^
    `- Failed to parse expression (in expression)
       `- Expected '+' or '-'

# Source Location Tracking

Track source positions through the parsing and transformation pipeline.

## Using SourceSpan

```rust
use parsanol::portable::source_location::{SourceSpan, SourcePosition};
use parsanol::portable::transform::{ast_to_value_with_span};

// Create a span from offsets
let span = SourceSpan::from_offsets(input, 10, 20);
println!("Line {}, Column {}", span.start.line, span.start.column);

// Merge adjacent spans
let merged = span1.merge(&span2);

// Check overlap
if span1.overlaps(&span2) {
    // Spans overlap
}

// Transform AST with source spans preserved
let (value, spans) = ast_to_value_with_span(&ast, &arena, input);
```

# Grammar Composition

Build complex grammars by importing and composing smaller grammars.

## Importing Grammars

```rust
use parsanol::portable::parser_dsl::*;

let mut builder = GrammarBuilder::new();

// Import another grammar with a prefix
builder.import(&expression_grammar, Some("expr"));
builder.import(&type_grammar, Some("type"));

// Reference imported rules
let combined = seq(vec![
    ref_("expr:root"),  // References expression_grammar's root
    str(":"),
    ref_("type:root"),  // References type_grammar's root
]);

builder.rule("typed_expr", combined);
let grammar = builder.build();
```

# Ruby FFI

Parsanol-rs can be compiled as a Ruby extension for use with
parsanol-ruby.

## Features

The Ruby FFI provides:

- Fast parsing via Rust (18-44x faster than pure Ruby)

- Three transformation options:

  - **Ruby Transform**: Parse in Rust, transform in Ruby
    (Parslet-compatible)

  - **Serialized**: Parse + transform in Rust, JSON output

  - **Native**: Direct Ruby object construction via FFI

- **Streaming Builder**: Single-pass parsing with Ruby callbacks (most
  efficient)

## Building for Ruby

```bash
# Build with Ruby support
cargo build --features ruby

# The resulting library can be loaded as a Ruby extension
```

## Ruby API

```ruby
require 'parsanol'

class MyParser < Parsanol::Parser
  rule(:number) { match('[0-9]').repeat(1).as(:int) }
  rule(:operator) { str('+') | str('-') }
  rule(:expr) { number.as(:left) >> operator.as(:op) >> number.as(:right) }
  root(:expr)
end

parser = MyParser.new
tree = parser.parse("42+8")
```

## Streaming Builder (Ruby)

For maximum performance, use the streaming builder API:

```ruby
require 'parsanol'

# Define a builder class
class StringCollector
  include Parsanol::BuilderCallbacks

  def initialize
    @strings = []
  end

  def on_string(value, offset, length)
    @strings << value
  end

  def on_int(value)
    @strings << value.to_s
  end

  def finish
    @strings
  end
end

# Parse with streaming builder
builder = StringCollector.new
result = Parsanol::Native.parse_with_builder(grammar_json, input, builder)
# result: ["42", "+", "8"]
```

See [parsanol-ruby](https://github.com/parsanol/parsanol-ruby) for full
documentation.

# WASM Support

Parsanol-rs can be compiled to WebAssembly for use in browsers or
Node.js.

## Building for WASM

```bash
# Install wasm-pack
cargo install wasm-pack

# Build for web
wasm-pack build --features wasm --target web
```

## JavaScript API

```javascript
import { Parser, Grammar } from 'parsanol';

const grammar = Grammar.fromJson({
  atoms: [
    { Str: { pattern: "hello" } }
  ],
  root: 0
});

const parser = new Parser(grammar);
const result = parser.parse("hello");
```

# Debug Tools

## Parser Tracing

Enable tracing for debugging:

```rust
let (result, trace) = parser.parse_with_trace();

// Print trace
println!("{}", trace.format(&grammar));
```

## Grammar Visualization

```rust
use parsanol::portable::debug::GrammarVisualizer;

let viz = GrammarVisualizer::new(&grammar);

// Generate Mermaid diagram
println!("{}", viz.to_mermaid());

// Generate GraphViz DOT
println!("{}", viz.to_dot());
```

# Performance

Parsanol-rs is designed for high performance:

- **18-44x Faster** than pure Ruby parsers (Parslet)

- **99.5% Fewer Allocations** through arena allocation

- **O(n) Parsing** via packrat memoization

- **SIMD Optimization**: Fast character matching via memchr

- **AHash**: Fast hashing for cache lookups

- **SmallVec**: Stack-allocated small collections

## Benchmarks

| Parser                       | Input Size | Time   |
|------------------------------|------------|--------|
| parsanol-rs (Ruby Transform) | 1KB JSON   | ~50µs  |
| parsanol-rs (Serialized)     | 1KB JSON   | ~30µs  |
| parsanol-rs (Native)         | 1KB JSON   | ~20µs  |
| Pure Ruby (Parslet)          | 1KB JSON   | ~800µs |

# Security

Parsanol-rs includes built-in protection against denial-of-service
attacks.

## Default Limits

| Limit | Default Value | Description |
|----|----|----|
| `max_input_size` | 100 MB | Maximum input size in bytes |
| `max_recursion_depth` | 1000 | Maximum recursion depth for nested structures |

## Custom Limits

For untrusted input, configure custom limits:

```rust
use parsanol::portable::{PortableParser, AstArena, Grammar, ParseError};

// For untrusted input, use stricter limits
let mut parser = PortableParser::with_limits(
    &grammar,
    input,
    &mut arena,
    10 * 1024 * 1024,  // 10 MB max input
    100,                // 100 max recursion depth
);

match parser.parse() {
    Ok(ast) => { /* success */ },
    Err(ParseError::InputTooLarge { input_size, max_size }) => {
        eprintln!("Input too large: {} > {}", input_size, max_size);
    },
    Err(ParseError::RecursionLimitExceeded { depth, max_depth }) => {
        eprintln!("Recursion too deep: {} > {}", depth, max_depth);
    },
    Err(e) => { /* other errors */ },
}
```

## Best Practices

1. **Always limit input size** when parsing untrusted data
2. **Use external timeouts** for network services (e.g., `tokio::time::timeout`)
3. **Monitor memory usage** in production environments

See [SECURITY.md](SECURITY.md) for complete security documentation.

# Module Reference

## Core Modules

| Module                        | Description                                 |
|-------------------------------|---------------------------------------------|
| `portable::parser`            | PEG parsing engine with packrat memoization |
| `portable::grammar`           | Grammar representation and serialization    |
| `portable::ast`               | AST node types                              |
| `portable::arena`             | Arena allocator for AST nodes               |
| `portable::cache`             | Dense cache for memoization                 |
| `portable::parser_dsl`        | Fluent API for grammar definition           |
| `portable::transform`         | Transform system for converting parse trees |
| `portable::error`             | Rich error reporting                        |
| `portable::infix`             | Infix expression parsing with precedence    |
| `portable::debug`             | Debugging and visualization tools           |
| `portable::source_location`   | Source span tracking with line/column info  |
| `portable::streaming`         | Streaming parser support for large inputs   |
| `portable::streaming_builder` | Single-pass parsing with custom builders    |
| `portable::parallel`          | Parallel parsing for batch processing       |
| `portable::incremental`       | Incremental parsing for editor integration  |
| `portable::visitor`           | AST visitor pattern implementation          |
| `portable::source_map`        | Source map generation for debugging         |

# Examples

See the `examples/` directory for 28 complete examples with both Rust
and Ruby implementations:

## Expression Parsers

| Example | Description |
|----|----|
| `calculator` | Parse and evaluate mathematical expressions with operator precedence |
| `boolean-algebra` | Parse boolean expressions with AND, OR, NOT operators |
| `expression-evaluator` | Evaluate expressions with variables and function calls |
| `prec-calc` | Precedence climbing algorithm for infix expressions |

## Data Formats

| Example      | Description                                                |
|--------------|------------------------------------------------------------|
| `json`       | JSON parser with pattern matching and transform approaches |
| `csv`        | CSV parser handling quoted fields and escaping             |
| `ini`        | INI configuration file parser                              |
| `simple-xml` | XML parser with tag matching                               |
| `markup`     | Lightweight markup language parser                         |
| `toml`       | TOML configuration file parser                             |
| `yaml`       | YAML subset parser                                         |
| `markdown`   | Markdown subset parser with headers and lists              |
| `iso-8601`   | ISO 8601 date/time/duration parser                         |
| `iso-6709`   | ISO 6709 geographic coordinate parser                      |

## URLs & Network

| Example      | Description                                   |
|--------------|-----------------------------------------------|
| `url`        | URL parser with scheme, host, path components |
| `email`      | Email address parser with validation          |
| `ip-address` | IPv4 address parser with octet validation     |

## Code & Templates

| Example    | Description                                      |
|------------|--------------------------------------------------|
| `erb`      | ERB template parser for Ruby templates           |
| `sexp`     | S-expression parser for Lisp-style syntax        |
| `minilisp` | MiniLisp parser demonstrating recursive grammars |

## Text Processing

| Example           | Description                                 |
|-------------------|---------------------------------------------|
| `balanced-parens` | Balanced parentheses parser                 |
| `string-literal`  | String literal parser with escape sequences |
| `sentence`        | Sentence parser with Unicode support        |
| `comments`        | Comment parser (line and block comments)    |

## Error Handling

| Example           | Description                              |
|-------------------|------------------------------------------|
| `error-reporting` | Rich error reporting with tree structure |
| `deepest-errors`  | Deepest error point tracking             |
| `nested-errors`   | Nested error tree visualization          |

## Conceptual Examples

| Example      | Description                      |
|--------------|----------------------------------|
| `modularity` | Grammar composition from modules |

Run examples with:

```bash
cargo run --example calculator/basic
cargo run --example json/basic
cargo run --example url/basic
```

Full documentation and interactive examples available at [the
website](https://parsanol.github.io/examples).

# API Stability

The API is currently in active development. Version 0.x indicates that
breaking changes may occur.

Stable APIs:

- `Grammar` and `GrammarBuilder`

- `PortableParser` basic parsing

- `AstArena` and `AstNode`

- Parser DSL combinators

- Streaming builder trait and built-in builders

- Parallel parsing functions

Experimental APIs (may change):

- `Transform` and pattern matching

- Rich error reporting

- Infix expression parsing

- Debug/trace tools

# License

MIT License - see [LICENSE](LICENSE) file for details.

# Contributing

Contributions are welcome! Please feel free to submit issues and pull
requests at [GitHub](https://github.com/parsanol/parsanol-rs).

## Development Setup

```bash
# Clone the repository
git clone https://github.com/parsanol/parsanol-rs.git
cd parsanol-rs

# Build
cargo build

# Run tests (30 passing unit tests + 18 ignored doc tests)
cargo test

# Run benchmarks
cargo bench

# Check code quality
cargo clippy
cargo fmt --check
```

## Testing

The test suite consists of multiple types of tests:

**Unit tests:** Test internal functionality of each module (parser, arena, cache, etc.).

**Integration tests:** Located in `tests/` directory, test end-to-end parsing scenarios.

**Examples:** Located in `examples/` directory, these are runnable parsers that demonstrate real-world usage. Examples are compiled and tested via `cargo test --examples`.

**Documentation tests (doc tests):** Code examples in documentation comments. Note that many doc tests are marked with `ignore` because they show **incomplete code snippets** (e.g., method signatures or pseudocode) rather than complete runnable examples. This is intentional - the doc tests illustrate API patterns, while the `examples/` directory contains fully runnable code that is verified by CI.

To run all tests:

```bash
# Unit + integration tests
cargo test

# Include doc tests (most will be ignored as designed)
cargo test --doc

# Test examples
cargo test --examples

# Run ignored doc tests (will fail if not complete)
cargo test -- --ignored
```

# Release Process

This project uses [release-plz](https://release-plz.dev/) for automated releases.

## How It Works

1. **Push to main** → release-plz creates/updates a Release PR

2. **Review and merge the Release PR** → Version is updated in main

3. **After merge** → release-plz automatically:
   - Creates a git tag (e.g., `v0.1.2`)
   - Publishes to crates.io
   - Creates a GitHub release

4. **Build artifacts** → CI builds native libraries and uploads them to the GitHub release

## Maintainer Workflow

### Normal Release (Recommended)

Just push commits with conventional commit messages:

```bash
git commit -m "feat: add new parser combinator"
git push origin main
```

release-plz will:
1. Create a Release PR with version bump (e.g., `0.1.1` → `0.1.2` for `feat:`)
2. Wait for you to review and merge
3. Publish automatically after merge

### Manual Release

If you need to trigger a release manually:

1. Go to **Actions** → **Release** workflow
2. Click **Run workflow**
3. Select action:
   - `auto` (default): Let release-plz decide
   - `release-pr`: Just create/update the Release PR
   - `release`: Force a release immediately

### Version Bump Rules

release-plz uses [conventional commits](https://www.conventionalcommits.org/):

| Commit Type | Version Bump |
|-------------|--------------|
| `feat:` | Minor (0.1.0 → 0.2.0) |
| `fix:` | Patch (0.1.0 → 0.1.1) |
| `feat!:` or `fix!:` | Major (0.1.0 → 1.0.0) |
| `docs:`, `chore:`, etc. | No bump (changelog only) |

### What Gets Released

- **crates.io**: `parsanol` crate
- **GitHub Release**: With release notes
- **Build Artifacts**: Native libraries for Linux, macOS, Windows (x64, ARM64)

### Troubleshooting

**"Already published" error:**
- release-plz sees an existing tag and thinks the version is already published
- Solution: Ensure Cargo.toml version matches what you want to publish

**No Release PR created:**
- Check that commits follow conventional commit format
- Check GitHub Actions logs for the `release-pr` job

**Publish failed:**
- Check crates.io API token is valid
- Check version doesn't already exist on crates.io

## FFI Feature Testing

This crate supports optional Ruby and WebAssembly (WASM) FFI features.
These must be tested explicitly.

> [!IMPORTANT]
> FFI features require additional setup and may not compile/link in all
> environments. Always verify FFI code compiles before pushing to CI.

### Ruby FFI Testing

The Ruby FFI uses the `magnus` crate to provide Ruby bindings.

**Prerequisites:**
- Ruby 3.0+ installed
- Ruby development headers (macOS: `brew install ruby`, Ubuntu: `sudo apt-get install ruby-dev`)

**Testing:**
```bash
# Compile-time check (no Ruby required for linking)
cargo check --features ruby
cargo clippy --features ruby --lib -- -D warnings

# Full integration tests (requires Ruby runtime)
# Note: These tests are marked #[ignore] - run manually
cargo test --features ruby --test ruby_ffi -- --ignored
```

**Test coverage:**
- `tests/ruby_ffi.rs` - Comprehensive tests for RubyBuilder, RubyObject trait
- Magnus type annotations (e.g., `funcall::<&str, (), Value>`)
- Error handling from Ruby callbacks
- Parse result conversion

### WebAssembly FFI Testing

The WASM FFI uses `wasm-bindgen` for JavaScript bindings.

**Prerequisites:**
- `wasm-pack` installed (`cargo install wasm-pack`)

**Testing:**
```bash
# Compile-time check
cargo check --features wasm
cargo clippy --features wasm --lib -- -D warnings

# Full WASM build and test
wasm-pack build --features wasm
wasm-pack test --node --features wasm
```

**Test coverage:**
- `tests/wasm_ffi.rs` - Tests for WASM exports, grammar serialization
- JsValue conversions
- Error handling for WASM

### CI Integration

CI automatically tests FFI features:

``` yaml
# From .github/workflows/ci.yml
strategy:
  matrix:
    feature: ["", "logging", "ruby", "wasm"]
```

The Ruby and WASM feature tests run on every push to catch FFI
regressions early.

# Release Process

This project uses [release-plz](https://release-plz.dev/) for automated releases.

## How It Works

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         RELEASE-PLZ WORKFLOW                                │
└─────────────────────────────────────────────────────────────────────────────┘

  Push to main
       │
       ▼
  ┌─────────────────┐
  │  release-pr job │  Creates/updates Release PR
  └────────┬────────┘
           │
           ▼
  ┌─────────────────┐
  │   Release PR    │  Contains version bump + changelog
  │   (on GitHub)   │
  └────────┬────────┘
           │
           │  Maintainer reviews and merges
           ▼
  ┌─────────────────┐
  │  release job    │  Runs release-plz release
  └────────┬────────┘
           │
           ├──────────────────────────────┐
           │                              │
           ▼                              ▼
  ┌─────────────────┐          ┌─────────────────┐
  │  Create tag     │          │ Publish to      │
  │  (v0.1.2)       │          │ crates.io       │
  └────────┬────────┘          └─────────────────┘
           │
           ▼
  ┌─────────────────┐
  │  GitHub Release │  With release notes
  └────────┬────────┘
           │
           ▼
  ┌─────────────────┐
  │  Build jobs     │  Build native libraries
  └────────┬────────┘
           │
           ▼
  ┌─────────────────┐
  │  Update Release │  Upload artifacts
  └─────────────────┘
```

## Maintainer Workflow

### Normal Release (Recommended)

Just push commits with conventional commit messages:

```bash
git commit -m "feat: add new parser combinator"
git push origin main
```

release-plz will:
1. Create a Release PR with version bump (e.g., `0.1.1` → `0.1.2` for `feat:`)
2. Wait for you to review and merge
3. Publish automatically after merge

### Manual Release

If you need to trigger a release manually:

1. Go to **Actions** → **Release** workflow
2. Click **Run workflow**
3. Select action:
   - `auto` (default): Let release-plz decide
   - `release-pr`: Just create/update the Release PR
   - `release`: Force a release immediately

### Version Bump Rules

release-plz uses [conventional commits](https://www.conventionalcommits.org/):

| Commit Type | Version Bump |
|-------------|--------------|
| `feat:` | Minor (0.1.0 → 0.2.0) |
| `fix:` | Patch (0.1.0 → 0.1.1) |
| `feat!:` or `fix!:` | Major (0.1.0 → 1.0.0) |
| `docs:`, `chore:`, etc. | No bump (changelog only) |

### What Gets Released

- **crates.io**: `parsanol` crate
- **GitHub Release**: With release notes
- **Build Artifacts**: Native libraries for Linux, macOS, Windows (x64, ARM64)

### Troubleshooting

**"Already published" error:**
- release-plz sees an existing tag and thinks the version is already published
- Solution: Ensure Cargo.toml version matches what you want to publish

**No Release PR created:**
- Check that commits follow conventional commit format
- Check GitHub Actions logs for the `release-pr` job

**Publish failed:**
- Check that the `crates.io` environment is configured in repository settings
- Check that trusted publishing is enabled

# See Also

- [parsanol-ruby](https://github.com/parsanol/parsanol-ruby) - Ruby
  bindings

- [Documentation
  Website](https://github.com/parsanol/parsanol.github.io)

- [Parslet](https://github.com/kschiess/parslet) - Original Ruby PEG
  parser (inspiration)
