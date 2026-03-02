# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

#### Architecture Improvements (Phase 1-4)

- **Prelude Module** - Easy imports with `use parsanol::prelude::*`
  - Exports core types: `Grammar`, `PortableParser`, `AstArena`, `AstNode`
  - Exports DSL: `str`, `re`, `seq`, `choice`, `any`, `ref_`
  - Exports error types: `ParseError`, `RichError`

- **Custom Atom Extension Points** - Register custom parsing logic
  - `CustomAtom` trait for defining custom parsers
  - `register_custom_atom(id, atom)` - Register with specific ID
  - `register_custom_atom_auto(atom)` - Auto-generate ID
  - Built-in atoms: `BalancedParens`, `BalancedBrackets`, `BalancedBraces`
  - Well-known IDs: `BALANCED_PARENS=100`, `BALANCED_BRACKETS=101`, `BALANCED_BRACES=102`

- **Plugin Architecture** - Third-party extension system
  - `ParsanolPlugin` trait with lifecycle hooks
  - `AtomRegistry` for custom atom registration
  - `TransformRegistry` for transform registration
  - Global plugin registry: `register_plugin()`, `list_plugins()`

- **C ABI** - Stable interface for external language bindings
  - Grammar lifecycle: `parsanol_grammar_new()`, `parsanol_grammar_free()`
  - Parsing: `parsanol_parse()`, `parsanol_parse_simple()`, `parsanol_parse_end_pos()`
  - Result accessors: `parsanol_result_success()`, `parsanol_result_ast_json()`, etc.
  - Error codes: `PARSANOL_OK`, `PARSANOL_ERROR_*`

- **Derive Macros** (always available, no feature flag needed)
  - `#[derive(FromAst)]` for typed AST conversion
  - Container attributes: `#[parsanol(rule = "...")]`
  - Variant attributes: `#[parsanol(tag = "...")]`, `#[parsanol(tag_expr = ...)]`
  - Field attributes: `#[parsanol(field = "...")]`, `#[parsanol(default)]`, `#[parsanol(default = expr)]`
  - Single-field tuple structs get automatic transparent conversion

- **SIMD Helpers** - Performance-critical byte operations
  - `find_byte()`, `find_byte2()`, `find_byte3()` - memchr-based search
  - `find_pattern()` - memmem-based substring search
  - `skip_while()` - Bulk character class matching
  - Integrated into `parse_repetition_bulk()` for 8x throughput

#### New Modules

- `src/portable/custom.rs` - Custom atom registry
- `src/portable/plugin.rs` - Plugin system
- `src/portable/c_ffi.rs` - C ABI bindings
- `src/derive.rs` - Derive macro support types
- `src/prelude.rs` - Convenience imports
- `parsanol-derive/` - Procedural macro crate (workspace member)

#### New Macros

- `all![p1, p2, ...]` - Ergonomic sequence construction
- `oneof![p1, p2, ...]` - Ergonomic alternative construction

#### New Types

- `CustomAtom` trait - Custom parsing logic interface
- `CustomResult` - Result type for custom atoms
- `ParsanolPlugin` trait - Plugin interface
- `PluginRegistry`, `AtomRegistry`, `TransformRegistry` - Registries
- `ParsanolGrammar`, `ParsanolResult` - Opaque C ABI handles
- `FromAstError` - Derive macro conversion errors
- `PluginInfo`, `AtomInfo` - Information structs

### Changed

#### API Improvements

- **Error Type Unification**
  - `ParseError::into_rich(self, input)` - Convert to rich error
  - `impl From<ParseError> for RichError` - Seamless conversion
  - `RichError` re-exported from `portable` module

- **Regex Cache Optimization**
  - Added `CacheStats` with hit/miss counts
  - Added `stats()` and `reset_stats()` functions

- **Serde Support**
  - `AstNode` now derives `Serialize` and `Deserialize`

#### Code Organization

- **Workspace Restructure** - Now matches tokio workspace pattern
  - Root `Cargo.toml` is workspace-only (no package section)
  - Main crate in `parsanol/` subdirectory
  - `parsanol-derive` as workspace member
- **Ruby FFI Separation** - Moved to `src/ruby_ffi/` modules
- **Consolidated Re-exports** - Clear groupings in `mod.rs`

### Removed

- **`derive` feature flag** - Derive macros are now always available
  - No longer need to enable `features = ["derive"]`
  - `parsanol-derive` is a required dependency

### Fixed

- Various documentation improvements
- Fixed pattern matching for new `Atom::Custom` variant
- Fixed clippy warnings in derive macro code

---

## [0.1.3] - 2025-02-24

### Added

- Comprehensive test suite with 234 tests
- 37 standalone examples in examples/ directory
- GitHub Actions CI/CD workflow
- Integration tests for parser, transform, infix, error, and lexer modules
- Production readiness checklist
- Source location tracking (SourceSpan with line/column info)
- Grammar composition with import() functionality
- Transform pattern indexing for O(1) dispatch
- ast_to_value_with_span() for preserving source spans through transforms
- Streaming parser support for large inputs
- Incremental parsing support for editor integration

### Changed

- Renamed transformation modes from "Option A/B/C+" to proper names:
  - Ruby Transform: Parse in Rust, transform in Ruby (Parslet-compatible)
  - Serialized Transform: Parse → Serialize to JSON for FFI
  - Native Transform: Parse + Transform in Rust, direct FFI construction
- Performance: 18-44x faster than pure Ruby parsers
- 99.5% fewer allocations through arena allocation

### Security

- Input size limit configuration (max 100 MB by default)
- Recursion depth limit configuration (max 1000 by default)

## [0.1.0] - 2025-02-24

### Added

- Core PEG parsing with packrat memoization
- Arena allocation for zero-copy AST construction
- Parser DSL for idiomatic grammar definition
- Generic lexer framework
- Rich error reporting with tree-structured errors
- Transformation system for converting parse trees
- Infix expression parsing with precedence handling
- Debug tools (tracing, visualization)
- Optional Ruby FFI bindings
- Optional WASM bindings
- `parsanol-ruby-derive` proc macro crate
- Documentation website at parsanol.github.io

---

## Migration Guide

### From 0.1.3 to Unreleased

#### Workspace Structure Change

The repository now uses a Cargo workspace with the main crate in `parsanol/`:

```
parsanol-rs/
├── parsanol/              # Main parser library
├── parsanol-derive/       # Derive macros (always included)
├── examples/              # Example parsers
└── Cargo.toml             # Workspace root
```

#### Using the Prelude

```rust
// Before
use parsanol::portable::{Grammar, PortableParser, AstArena, AstNode};

// After
use parsanol::prelude::*;
```

#### Using Derive Macros

No feature flag needed - derive macros are always available:

```rust
use parsanol::derive::FromAst;

#[derive(FromAst)]
#[parsanol(rule = "expression")]
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
let expr: Expr = value.try_into()?;
```

#### Using Custom Atoms

```rust
use parsanol::portable::custom::{CustomAtom, CustomResult, register_custom_atom};

struct MyMatcher;
impl CustomAtom for MyMatcher {
    fn parse(&self, input: &str, pos: usize) -> Option<CustomResult> {
        // Your custom parsing logic
        None
    }
    fn description(&self) -> &str { "my matcher" }
}

// Register with ID >= 1000 to avoid conflicts
register_custom_atom(1000, Box::new(MyMatcher));
```

#### Using Plugins

```rust
use parsanol::portable::plugin::{ParsanolPlugin, register_plugin, AtomRegistry};

struct MyPlugin;
impl ParsanolPlugin for MyPlugin {
    fn name(&self) -> &str { "my_plugin" }
    fn register_atoms(&self, registry: &mut AtomRegistry) {
        // Register custom atoms
    }
}

register_plugin(Box::new(MyPlugin));
```

#### Using the C ABI

```c
#include <parsanol.h>

// Create grammar from JSON
ParsanolGrammar* grammar = parsanol_grammar_new(json);

// Parse
ParsanolResult* result = NULL;
if (parsanol_parse(grammar, input, &result) == PARSANOL_OK) {
    if (parsanol_result_success(result)) {
        const char* ast = parsanol_result_ast_json(result);
        printf("AST: %s\n", ast);
    }
    parsanol_result_free(result);
}

parsanol_grammar_free(grammar);
```

#### Using Ergonomic Macros

```rust
use parsanol::{all, oneof};

// Before
let parser = Sequence(vec![
    dynamic(str("hello")),
    dynamic(str("world")),
]);

// After
let parser = all![str("hello"), str("world")];

// Before
let parser = Choice(vec![
    dynamic(str("a")),
    dynamic(str("b")),
]);

// After
let parser = oneof![str("a"), str("b")];
```
