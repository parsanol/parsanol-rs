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
  Rust structs via derive macros

## Features

- [Quick Start](#quick-start) - Get started in minutes
- [Backend Abstraction](#backend-abstraction) - Extensible backend trait system
- [Bytecode Backend](#bytecode-backend) - Optional VM backend for linear patterns
- [Parser DSL](#parser-dsl) - Fluent API for grammar definition
- [Capture Atoms](#capture-atoms) - Extract named values during parsing
- [Scope Atoms](#scope-atoms) - Isolated capture contexts
- [Dynamic Atoms](#dynamic-atoms) - Runtime-determined parsing via callbacks
- [Streaming with Captures](#streaming-with-captures) - Memory-efficient parsing with capture support
- [Transform System](#transform-system) - Convert parse trees to typed structs
- [Derive Macros](#derive-macros) - Automatic typed AST generation
- [Streaming Builder](#streaming-builder) - Single-pass parsing with custom output
- [Parallel Parsing](#parallel-parsing) - Multi-file parsing with rayon
- [Infix Expression Parsing](#infix-expression-parsing) - Built-in operator precedence
- [Rich Error Reporting](#rich-error-reporting) - Tree-structured error messages
- [Source Location Tracking](#source-location-tracking) - Line/column tracking through transforms
- [Grammar Composition](#grammar-composition) - Import and compose grammars
- [Ruby FFI](#ruby-ffi) - Optional Ruby bindings
- [WASM Support](#wasm-support) - Optional WebAssembly bindings

# Bytecode Backend

Parsanol-rs supports two parsing backends:

1. **Packrat (default)**: Memoization-based parser with O(n) time complexity for all grammars
2. **Bytecode VM**: Stack-based virtual machine with optimization passes

## Backend Comparison

Both backends produce **identical parsing results** for all valid inputs. The difference lies in performance characteristics:

| Aspect | Packrat | Bytecode VM |
|--------|---------|-------------|
| **Time Complexity** | Guaranteed O(n) | O(n) to O(2^n) depending on grammar |
| **Memory Usage** | Higher (memoization table) | Lower (stack-based) |
| **Compilation** | None required | Pre-compilation needed |
| **Nested Repetitions** | Handles efficiently | Can be exponential |
| **Simple Patterns** | Good | Excellent |
| **Predictability** | Consistent performance | Varies by grammar |

### Performance Characteristics

**Packrat Backend:**
- Memoization stores parse results at each position
- Guarantees O(n) time complexity regardless of grammar structure
- Memory overhead scales with input size and grammar complexity
- Ideal when predictable performance is required

**Bytecode VM Backend:**
- Stack-based execution with backtracking
- O(n) for linear patterns (most common case)
- Can exhibit O(2^n) behavior for pathological patterns like `(a*)*`
- Lower memory footprint, good for memory-constrained environments
- Pre-compilation enables optimization passes

### Decision Matrix

| Grammar Type | Recommended Backend | Reason |
|--------------|---------------------|--------|
| JSON, XML, config files | Either | Linear patterns, both perform well |
| Programming languages | Packrat | Complex grammar with nested structures |
| Log parsing | Bytecode | Simple patterns, streaming potential |
| Nested repetitions `(a*)*` | Packrat | Avoids exponential backtracking |
| Memory-constrained | Bytecode | Lower memory footprint |
| Need predictable O(n) | Packrat | Guaranteed linear time |

### Automatic Selection

Use `Backend::Auto` (the default) to let parsanol analyze your grammar:

```rust
// Automatic selection (default)
let mut parser = Parser::auto(grammar);

// Or explicitly:
let mut parser = Parser::new(grammar, Backend::Auto);

// Check the analysis
let analysis = parser.analysis();
println!("Has nested repetitions: {}", analysis.has_nested_repetition);
println!("Recommended: {:?}", analysis.recommended_backend());
```

### Why Nested Repetitions Are the Criterion

The backend selection is based on a **single hard rule**:

- **Has nested repetitions** (e.g., `(a*)*`) → **Packrat**
- **Otherwise** → **Bytecode**

This is the only criterion because nested repetitions are the **only pattern that causes exponential time complexity** in the bytecode backend. Here's why:

**The Algorithmic Problem:**

When a repetition contains another repetition, the parser must try all possible ways to divide the input. For pattern `(a*)*` on input "aaa":

```
Division 1: (aaa)           - outer * matches 1 group
Division 2: (aa)(a)         - outer * matches 2 groups
Division 3: (a)(aa)         - outer * matches 2 groups (different split)
Division 4: (a)(a)(a)       - outer * matches 3 groups
... and so on
```

The number of ways to partition n characters is O(2^n). The bytecode VM tries each possibility via backtracking, leading to exponential time.

**Why Packrat Solves It:**

Packrat memoizes results by (position, rule). Once `(a*)` is evaluated at position i, the result is cached. Subsequent evaluations at the same position are O(1) cache hits. This guarantees O(n) total time.

**Why Other Patterns Don't Matter:**

| Pattern | Time Impact | Backend Difference |
|---------|-------------|-------------------|
| Overlapping choices (`"a" \| "aa"`) | Linear backtracking | Both handle identically |
| Deep nesting | Stack depth increases | Both handle fine |
| Many alternatives | More choice points | Linear in alternative count |
| Left recursion | Infinite loop | **Both fail** - not a backend issue |

### How the Analysis Works

The grammar analysis is deliberately simple:

```rust
pub struct GrammarAnalysis {
    /// Total atoms in the grammar
    pub atom_count: usize,
    /// Whether any Repetition contains another Repetition
    pub has_nested_repetition: bool,
}
```

The algorithm iterates through all atoms and checks: "Is this a Repetition whose inner atom is also a Repetition?"

```rust
for atom in &grammar.atoms {
    if let Atom::Repetition { atom: inner_idx, .. } = atom {
        if let Some(inner) = grammar.get_atom(*inner_idx) {
            if matches!(inner, Atom::Repetition { .. }) {
                has_nested_repetition = true;
                break;
            }
        }
    }
}
```

This is O(atoms) time and detects the only pattern that matters for backend selection.

### When to Override Auto-Selection

The auto-selection only considers **time complexity**. You may want to manually select based on:

| Scenario | Manual Selection | Rationale |
|----------|------------------|-----------|
| **Memory-constrained** (embedded, WASM) | `Backend::Bytecode` | Lower memory: O(depth) vs O(n×rules) |
| **Very large files** (>100MB) | `Backend::Bytecode` | Packrat table grows with input size |
| **Predictable latency required** | `Backend::Packrat` | Guaranteed O(n), no pathological cases |
| **Streaming parsing** | `Backend::Bytecode` | Packrat requires full input in memory |
| **Incremental re-parsing** | `Backend::Packrat` | Memo table can be reused for unchanged portions |
| **Grammar has nested repetitions but input is bounded** | Either | If input is always small, exponential doesn't matter |
| **Testing/debugging** | `Backend::Packrat` | Consistent behavior across all inputs |

```rust
// Memory-constrained environment
let mut parser = Parser::bytecode(grammar);

// Safety-critical with guaranteed O(n)
let mut parser = Parser::packrat(grammar);

// Explicit choice regardless of analysis
let mut parser = Parser::new(grammar, Backend::Packrat);
```

### Problematic Grammar Patterns

The following patterns can cause exponential O(2^n) behavior in the Bytecode backend.
They are **safe with Packrat** due to memoization. If your grammar contains these,
use Packrat explicitly or rely on `Backend::Auto`.

**Critical Pattern: Nested Repetitions**
```
(a*)*     // CRITICAL: Outer * tries O(2^n) ways to divide input
(a+)+     // Same issue
((a|b)*)* // Even worse with choice

// Safe alternatives:
a*        // Single repetition - O(n)
(a b)*    // Fixed sequence inside - O(n)
```

**Moderate Pattern: Overlapping Choice Prefixes**
```
// Problematic: All start with 'a'
("a" | "aa" | "aaa")+

// Better: Distinct first characters
("a" | "b" | "c")+
```

**Safe Pattern: Deep Recursion (Both handle well)**
```
expr = term (("+" | "-") term)*
// Recursive but structured - both backends handle efficiently
```

### Analyzing Your Grammar

Use the GrammarAnalysis API to check for nested repetitions:

```rust
use parsanol::portable::{
    parser_dsl::{str, re, GrammarBuilder},
    bytecode::{Backend, GrammarAnalysis, Parser},
};

fn main() {
    let grammar = GrammarBuilder::new()
        .rule("expr", re(r"[0-9]+"))
        .build();

    // Analyze the grammar
    let analysis = GrammarAnalysis::analyze(&grammar);

    // The only field that matters for backend selection
    if analysis.has_nested_repetition {
        println!("⚠️  Nested repetitions detected - use Packrat!");
    } else {
        println!("✅ No nested repetitions - Bytecode is efficient");
    }

    // Get recommendation (hard rule: nested repetition → Packrat, else → Bytecode)
    println!("Recommended: {:?}", analysis.recommended_backend());
}
```

**GrammarAnalysis Fields:**

| Field | Type | Purpose |
|-------|------|---------|
| `atom_count` | `usize` | Number of atoms in grammar (informational) |
| `has_nested_repetition` | `bool` | **The criterion** - if true, use Packrat |

**The `recommended_backend()` Method:**

Returns `Backend::Packrat` if `has_nested_repetition` is true, otherwise `Backend::Bytecode`. This is what `Backend::Auto` uses internally.

## Using the Bytecode Backend

```rust
use parsanol::portable::{
    parser_dsl::{str, re, GrammarBuilder},
    bytecode::{Backend, Parser},
};

let grammar = GrammarBuilder::new()
    .rule("number", re(r"[0-9]+"))
    .build();

// Create parser with bytecode backend
let mut parser = Parser::new(grammar, Backend::Bytecode);
let result = parser.parse("42");

// Or use auto-selection (analyzes grammar complexity)
let mut parser = Parser::auto(grammar);
let result = parser.parse("42");
```

### Known Differences

Both backends produce **identical results** for the vast majority of patterns. However, there are edge cases where behavior differs:

**Alternatives in Sequences**: For patterns like `("a" | "aa") "b"` on input `"aab"`:
- **Packrat**: May succeed due to memoization re-evaluation
- **Bytecode**: Fails (standard PEG semantics - once "a" succeeds, "aa" is not tried)

This difference only affects patterns with:
- Alternatives containing overlapping prefixes ("a" vs "aa")
- The alternative is followed by content that fails
- The later alternative would allow the following content to succeed

For most practical grammars, this difference never manifests. Use `Backend::Auto` to let parsanol choose the appropriate backend.

## Backend Abstraction

Parsanol provides a trait-based backend abstraction for extensibility. You can implement custom backends or use the built-in ones interchangeably.

### Using the ParsingBackend Trait

```rust
use parsanol::portable::backend::{ParsingBackend, PackratBackend, BytecodeBackend, Backend};

// Use Packrat backend for predictable O(n) performance
let mut packrat = PackratBackend::new();
let result = packrat.parse(&grammar, input)?;

// Use Bytecode backend for lower memory usage
let mut bytecode = BytecodeBackend::new();
let result = bytecode.parse(&grammar, input)?;

// Configure backends
let packrat = PackratBackend::new()
    .with_max_recursion_depth(500)
    .with_timeout_ms(5000);

let bytecode = BytecodeBackend::new()
    .with_auto_fallback(true);  // Falls back to Packrat for complex grammars
```

### Runtime Backend Selection

```rust
use parsanol::portable::backend::Backend;

// Select backend at runtime
let backend_type = Backend::default_for_grammar(&grammar);

match backend_type {
    Backend::Packrat => {
        let mut parser = PackratBackend::new();
        parser.parse(&grammar, input)?
    }
    Backend::Bytecode => {
        let mut parser = BytecodeBackend::new();
        parser.parse(&grammar, input)?
    }
};
```

### Backend Characteristics

Each backend documents its performance characteristics:

```rust
use parsanol::portable::backend::{ParsingBackend, PackratBackend};

let backend = PackratBackend::new();
let chars = backend.characteristics();

println!("Time: {}", chars.time_complexity);        // "O(n)"
println!("Memory: {}", chars.memory_complexity);    // "O(n × r)"
println!("Memoization: {}", chars.uses_memoization); // true
println!("Streaming: {}", chars.supports_streaming); // false
println!("Incremental: {}", chars.supports_incremental); // true
println!("Safe: {}", chars.safe_for_all_grammars);  // true
```

### Implementing Custom Backends

```rust
use parsanol::portable::backend::{ParsingBackend, BackendCharacteristics, BackendResult};
use parsanol::portable::grammar::Grammar;

struct MyCustomBackend;

impl ParsingBackend for MyCustomBackend {
    fn parse(&mut self, grammar: &Grammar, input: &str) -> BackendResult {
        // Custom parsing logic here
        todo!()
    }

    fn name(&self) -> &'static str {
        "my-custom"
    }

    fn characteristics(&self) -> BackendCharacteristics {
        BackendCharacteristics {
            time_complexity: "O(n log n)",
            memory_complexity: "O(n)",
            uses_memoization: false,
            supports_streaming: true,
            supports_incremental: false,
            safe_for_all_grammars: true,
        }
    }
}
```

### Dynamic Backend Dispatch

For runtime polymorphism:

```rust
use parsanol::portable::backend::{DynBackend, PackratBackend, BytecodeBackend};

fn get_backend(use_packrat: bool) -> DynBackend {
    if use_packrat {
        Box::new(PackratBackend::new())
    } else {
        Box::new(BytecodeBackend::new())
    }
}

let mut backend: DynBackend = get_backend(true);
let result = backend.parse(&grammar, input)?;
```

## Quick Start Examples

Using the bytecode backend explicitly:

```rust
use parsanol::portable::{
    parser_dsl::{str, re, GrammarBuilder},
    bytecode::{Backend, Parser},
};

let grammar = GrammarBuilder::new()
    .rule("number", re(r"[0-9]+"))
    .build();

// Create parser with bytecode backend
let mut parser = Parser::new(grammar, Backend::Bytecode);
let result = parser.parse("42");
```

Using packrat backend explicitly:

```rust
let mut parser = Parser::new(grammar, Backend::Packrat);
let result = parser.parse("42");
```

## Optimization Passes

The bytecode backend applies 11 optimization passes automatically:

1. `DeadCodeElimination` - Remove unreachable code
2. `JumpChainSimplification` - Simplify jump chains
3. `JumpToReturnSimplification` - Direct returns
4. `JumpToFailSimplification` - Direct failures
5. `CombineAdjacentChars` - Char merging
6. `SpanOptimization` - CharSet* to Span
7. `FullCaptureOptimization` - Capture pairs to FullCapture
8. `TestCharOptimization` - Choice patterns to TestChar
9. `TestSetOptimization` - Choice patterns to TestSet
10. `TailCallOptimization` - Tail calls to jumps
11. `LookaheadOptimization` - Choice to PredChoice for predicates

## Bytecode VM Architecture

```
Grammar (Atoms) ──► Compiler ──► Program (bytecode)
                                    │
                                    ▼
Input ──────────────────────────► VM ──► AstNode
```

The bytecode VM uses:
- **Backtracking stack**: For choice point management
- **Capture stack**: For building AST nodes
- **Instruction pointer**: Sequential execution
- **Optimization passes**: Peephole optimization on compiled bytecode

## Instruction Set

The VM supports 28 instructions covering all PEG operations:

| Category | Instructions |
|----------|-------------|
| Matching | `Char`, `CharSet`, `String`, `Regex`, `Any`, `Custom` |
| Control Flow | `Jump`, `Call`, `Return`, `End` |
| Backtracking | `Choice`, `Commit`, `PartialCommit`, `BackCommit`, `Fail`, `FailTwice` |
| Captures | `OpenCapture`, `CloseCapture`, `FullCapture` |
| Tests | `TestChar`, `TestSet`, `TestAny` |
| Special | `Behind`, `Span`, `NoOp`, `PredChoice` |

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
    │  • Derive macros for typed ASTs                             │
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

# Workspace Structure

This repository uses a Cargo workspace with two crates:

```
parsanol-rs/
├── parsanol/              # Main parser library
│   ├── src/
│   └── Cargo.toml
├── parsanol-derive/       # Derive macros (always included)
│   ├── src/
│   └── Cargo.toml
├── examples/              # 39 example parsers
└── Cargo.toml             # Workspace root
```

# Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
parsanol = "0.1"
```

The `parsanol-derive` crate is automatically included as a dependency,
providing the `#[derive(FromAst)]` macro for typed AST conversion.

## Optional Features

- `ruby` - Enable Ruby FFI bindings (requires `magnus`)

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
|------|-------------|---------|
| `str("literal")` | Match exact string | `str("hello")` |
| `re("pattern")` | Match regex pattern | `re(r"[0-9]+")` |
| `any()` | Match any single character | `any()` |
| `ref_("rule")` | Reference to named rule | `ref_("expr")` |
| `seq([...])` | Sequence of atoms | `seq(vec![a, b, c])` |
| `choice([...])` | Alternative atoms | `choice(vec![a, b])` |
| `cut()` | Commit to this branch (prevent backtracking) | `cut()` |
| `capture("name", atom)` | Extract named value during parsing | `capture("id", re(r"[a-z]+"))` |
| `scope(atom)` | Create isolated capture context | `scope(seq([...]))` |
| `dynamic(callback)` | Runtime-determined parsing via callback | `dynamic(callback_id)` |

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

# Capture Atoms

Capture atoms extract named values during parsing, similar to regex named groups. They work with all backends (Packrat, Bytecode, Streaming).

## Basic Usage

```rust
use parsanol::portable::{
    parser_dsl::{capture, dynamic, re, seq, GrammarBuilder},
    PortableParser, AstArena,
};

let grammar = GrammarBuilder::new()
    .rule("greeting", seq(vec![
        capture("word", dynamic(re(r"[a-zA-Z]+"))),
    ]))
    .build();

let mut arena = AstArena::for_input(64);
let mut parser = PortableParser::packrat(grammar);
let result = parser.parse_from_pos(0, "hello world", &mut arena)?;

// Access captures
if let Some(text) = result.get_capture("word", "hello world") {
    println!("Captured: {}", text); // Prints: "hello"
}
```

## Capture API

```rust
// Get a single capture by name
let value = result.get_capture("name", input);

// Get all capture names
for name in result.capture_names() {
    println!("Capture: {}", name);
}

// Check if capture exists
if result.has_capture("name") {
    // ...
}
```

## Backend Compatibility

| Backend | Capture Support | Notes |
|---------|-----------------|-------|
| Packrat | Full | Native support |
| Bytecode | Full | Uses capture instructions |
| Streaming | Full | Captures persist across chunks |

# Scope Atoms

Scope atoms create isolated capture contexts. Captures made inside a scope are discarded when the scope exits, preventing pollution of the parent context.

## Use Cases

- Nested parsing where inner captures shouldn't affect outer state
- Repetitive patterns where each iteration starts fresh
- Context isolation in recursive grammars

## Basic Usage

```rust
use parsanol::portable::parser_dsl::{scope, seq, capture, dynamic, re, GrammarBuilder};

let grammar = GrammarBuilder::new()
    .rule("outer", seq(vec![
        capture("outer_name", dynamic(re(r"[a-z]+"))),
        scope(seq(vec![
            capture("inner_name", dynamic(re(r"[0-9]+"))),
        ])),
        // "inner_name" is NOT available here
    ]))
    .build();
```

# Dynamic Atoms

Dynamic atoms enable runtime-determined parsing via registered callbacks. This allows context-sensitive parsing where the grammar itself depends on input or previously captured values.

## Registering a Callback

```rust
use parsanol::portable::{
    Grammar, Atom, Parser,
    dynamic::{DynamicCallback, DynamicContext, register_dynamic_callback},
    parser_dsl::*,
};

struct KeywordCallback;

impl DynamicCallback for KeywordCallback {
    fn call(&self, ctx: &DynamicContext) -> Option<Atom> {
        // Access current position
        let pos = ctx.pos();
        // Access input
        let input = ctx.input();
        // Access captures made so far
        if let Some(lang) = ctx.get_capture("language") {
            match lang {
                "ruby" => Some(Atom::Str { pattern: "def".into() }),
                "python" => Some(Atom::Str { pattern: "lambda".into() }),
                _ => None,
            }
        } else {
            None
        }
    }

    fn description(&self) -> &str {
        "keyword_callback"
    }
}

let callback_id = register_dynamic_callback(Box::new(KeywordCallback));
```

## Using Dynamic Atoms in Grammars

```rust
let grammar = GrammarBuilder::new()
    .rule("keyword", dynamic_with_id(callback_id))
    .build();
```

## Backend Compatibility

| Backend | Dynamic Support | Notes |
|---------|-----------------|-------|
| Packrat | Full | Native support (recommended) |
| Bytecode | Fallback | Uses Packrat internally |
| Streaming | Fallback | Uses Packrat internally |

**Note:** For heavy dynamic atom usage, prefer the Packrat backend for best performance.

# Streaming with Captures

The streaming parser supports captures while maintaining bounded memory usage. Captures persist across streaming parse operations.

## Basic Usage

```rust
use parsanol::portable::{
    parser_dsl::{capture, dynamic, re, GrammarBuilder},
    streaming::{StreamingParser, ChunkConfig},
    arena::AstArena,
};
use std::io::Cursor;

let grammar = GrammarBuilder::new()
    .rule("word", capture("word", dynamic(re(r"[a-zA-Z]+"))))
    .build();

let config = ChunkConfig {
    chunk_size: 65536,  // 64 KB chunks
    window_size: 2,      // Keep 2 chunks in memory
};

let mut parser = StreamingParser::new(&grammar, config);
let mut arena = AstArena::for_input(65536);
let mut cursor = Cursor::new(input.as_bytes());

let result = parser.parse_from_reader(&mut cursor, &mut arena)?;

if let Some(captures) = &result.capture_state {
    for name in captures.names() {
        if let Some(value) = captures.get(&name) {
            println!("{} = {:?}", name, value.get_text(input));
        }
    }
}
```

## Chunk Configuration

| Preset | Chunk Size | Window | Use Case |
|--------|------------|--------|----------|
| `small()` | 16 KB | 2 | Real-time feeds |
| `medium()` | 64 KB | 3 | Default |
| `large()` | 256 KB | 4 | Log files |
| `huge()` | 1 MB | 5 | Large files |

## Performance Notes

- Memory: O(chunk_size × window_size + capture_state)
- Captures accumulate during parse, available at end
- For very large captures, use `reset()` to process incrementally

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

# Derive Macros

The `FromAst` derive macro automatically generates code to convert `Value`
types into typed Rust structs and enums. This eliminates boilerplate code
for AST transformation.

## Basic Usage

```rust
use parsanol::derive::FromAst;
use parsanol::portable::transform::Value;

#[derive(FromAst, Debug)]
pub enum Expr {
    #[parsanol(tag = "number")]
    Number(i64),

    #[parsanol(tag = "binop")]
    BinOp {
        left: Box<Expr>,
        op: String,
        right: Box<Expr>,
    },
}

// Convert Value to typed Expr
let value: Value = /* ... parsed value ... */;
let expr: Expr = value.try_into()?;
```

## Container Attributes

| Attribute | Description |
|-----------|-------------|
| `#[parsanol(rule = "name")]` | Specify the grammar rule name |

## Variant Attributes (for enums)

| Attribute | Description |
|-----------|-------------|
| `#[parsanol(tag = "literal")]` | Match by literal tag string |
| `#[parsanol(tag_expr = expr)]` | Match by expression (for dynamic tags) |

## Field Attributes

| Attribute | Description |
|-----------|-------------|
| `#[parsanol(field = "name")]` | Map to different hash field name |
| `#[parsanol(default)]` | Use `Default::default()` if missing |
| `#[parsanol(default = expr)]` | Use expression if missing |

## Complete Example

```rust
use parsanol::derive::FromAst;
use parsanol::portable::transform::Value;

#[derive(FromAst, Debug)]
#[parsanol(rule = "statement")]
pub enum Statement {
    #[parsanol(tag = "assignment")]
    Assignment {
        #[parsanol(field = "name")]
        variable: String,
        value: Box<Expr>,
    },

    #[parsanol(tag = "return")]
    Return {
        #[parsanol(default)]
        value: Option<Box<Expr>>,
    },

    #[parsanol(tag = "if")]
    If {
        condition: Box<Expr>,
        then_block: Vec<Statement>,
        #[parsanol(default)]
        else_block: Option<Vec<Statement>>,
    },
}

// Usage
fn parse_statement(value: Value) -> Result<Statement, parsanol::derive::FromAstError> {
    value.try_into()
}
```

## Single-Field Tuple Structs

Single-field tuple structs automatically get transparent conversion:

```rust
#[derive(FromAst)]
pub struct Identifier(pub String);

// Value::String("foo") directly converts to Identifier("foo")
```

## Error Handling

```rust
use parsanol::derive::FromAstError;

match value.try_into() {
    Ok(expr) => println!("Parsed: {:?}", expr),
    Err(FromAstError::MissingField(field)) => {
        eprintln!("Missing field: {}", field);
    }
    Err(FromAstError::UnknownTag) => {
        eprintln!("Unknown tag in enum");
    }
    Err(e) => eprintln!("Conversion error: {}", e),
}
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

- **26x faster** parsing than pure Ruby (Parslet)
- **Single `parse()` API** - no confusing options
- **Lazy line/column** - zero overhead unless needed
- **Streaming Builder** - single-pass parsing with callbacks

## Building for Ruby

> **Ruby 4.0 Support**: This version uses unreleased magnus 0.9.0 and rb-sys HEAD
> for Ruby 4.0 compatibility. The workspace `Cargo.toml` patches rb-sys automatically.

```bash
# Build with Ruby support
cargo build --features ruby

# The resulting library can be loaded as a Ruby extension
```

## Ruby API

```ruby
require 'parsanol/native'

# Serialize grammar once
grammar = str('hello').as(:greeting) >> str(' ').maybe >> match('[a-z]').repeat(1).as(:name)
grammar_json = Parsanol::Native.serialize_grammar(grammar)

# Parse - simple and clean
result = Parsanol::Native.parse(grammar_json, "hello world")
# => {greeting: "hello"@0, name: "world"@6}

# Line/column available when needed (computed lazily)
result[:greeting].line_and_column  # => [1, 1]
result[:name].line_and_column      # => [1, 7]
```

## Lazy Line/Column

Slice objects support lazy line/column computation:

- `slice.offset` - character position (always available, zero cost)
- `slice.content` - string value (always available, zero cost)
- `slice.line_and_column` - [line, column] tuple (computed lazily, cached)

This provides **zero overhead** for users who don't need position info,
while keeping line/column **always available** when needed.

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

See the `examples/` directory for 39 complete examples demonstrating
real-world parsing scenarios:

## Expression Parsers

| Example | Description |
|----|----|
| `calculator-pattern` | Parse expressions with pattern-based transforms |
| `calculator-transform` | Parse and evaluate expressions with native transforms |
| `boolean-algebra` | Parse boolean expressions with AND, OR, NOT operators |
| `expression-evaluator` | Evaluate expressions with variables and function calls |
| `prec-calc` | Precedence climbing algorithm for infix expressions |

## Data Formats

| Example      | Description                                                |
|--------------|------------------------------------------------------------|
| `json-pattern` | JSON parser with pattern matching |
| `json-transform` | JSON parser with native transforms |
| `csv-pattern` | CSV parser handling quoted fields (pattern mode) |
| `csv-transform` | CSV parser handling quoted fields (transform mode) |
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
| `ip-address` | IPv4/IPv6 address parser with validation      |

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
| `error-recovery`  | Error recovery strategies                |
| `deepest-errors`  | Deepest error point tracking             |
| `nested-errors`   | Nested error tree visualization          |

## Advanced Features

| Example         | Description                            |
|-----------------|----------------------------------------|
| `streaming`     | Streaming parser for large inputs      |
| `incremental`   | Incremental parsing for editor integration |
| `linter`        | Code linter with custom validation     |
| `custom-atoms`  | Custom atom registration               |
| `modularity`    | Grammar composition from modules       |

Run examples with:

```bash
cargo run --example calculator-transform
cargo run --example json-pattern
cargo run --example url
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

# Documentation

## Architecture

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the overall system architecture.

## Development

- [docs/refactoring-plan.md](docs/refactoring-plan.md) - Current refactoring roadmap
- [docs/continuation-prompt.md](docs/continuation-prompt.md) - Prompt for continuing work
- [docs/MIGRATION.md](docs/MIGRATION.md) - Migration guide from Parslet

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

# Build (workspace)
cargo build

# Run tests (234 unit tests)
cargo test --lib

# Run all examples
cargo build --examples

# Run benchmarks
cargo bench

# Check code quality
cargo clippy --lib -- -D warnings
cargo fmt --check
```

## Testing

The test suite consists of multiple types of tests:

**Unit tests:** 234 tests covering internal functionality of each module
(parser, arena, cache, transform, derive, etc.).

**Integration tests:** Located in `tests/` directory, test end-to-end parsing scenarios.

**Examples:** 39 runnable parsers in `examples/` directory demonstrating real-world usage.
Examples are compiled and tested via `cargo build --examples`.

**Documentation tests (doc tests):** Code examples in documentation comments. Note that many doc tests are marked with `ignore` because they show **incomplete code snippets** (e.g., method signatures or pseudocode) rather than complete runnable examples. This is intentional - the doc tests illustrate API patterns, while the `examples/` directory contains fully runnable code that is verified by CI.

To run all tests:

```bash
# Unit + integration tests
cargo test

# Include doc tests (most will be ignored as designed)
cargo test --doc

# Test examples compile
cargo build --examples

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
- `.cargo/config.toml` configures WASM-specific rustflags (auto-applied)

**Testing:**
```bash
# Compile-time check
cargo check --features wasm
cargo clippy --features wasm --lib -- -D warnings

# Build for WASM target
cargo build --features wasm --target wasm32-unknown-unknown

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
