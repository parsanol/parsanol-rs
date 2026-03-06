# Parsanol Architecture Refactoring Plan

## Overview

Refactor parsanol-rs to achieve:
- **Clean architecture**: Files under 1000 lines, clear module boundaries
- **High performance**: Maintain O(n) Packrat, optimized Bytecode
- **Easy to use**: Clear API, good documentation
- **Easy to extend**: Backend trait abstraction, plugin system
- **OOP Design**: Proper separation of concerns, no GOD CLASSes

## Status

- [x] Phase 0: Backend Selection Simplification (DONE)
- [x] Phase 0.1: Documentation Update (DONE)
- [x] Phase 1: Add ParsingBackend Trait (DONE)
- [x] Phase 2: Split Largest Files (DONE)
- [x] Phase 2.1: Extract ResourceGovernor (DONE)
- [x] Phase 2.2: Split Backend Tests Module (DONE)
- [x] Phase 3: Complete GOD CLASS Decomposition (DONE - ResourceGovernor extracted)
- [-] Phase 4: Module Renaming (DEFERRED - current naming is clear)
- [-] Phase 5: FFI Consolidation (DEFERRED - current structure works well)

---

## Phase 0: Completed Work

### 0.1 Backend Selection Simplification
- Removed arbitrary `complexity_score` and thresholds
- Simplified to single criterion: `has_nested_repetition`
- Hard rule: nested repetitions → Packrat, otherwise → Bytecode
- Updated `GrammarAnalysis` to only two fields

### 0.2 Documentation
- Added detailed rationale for backend selection
- Added "When to Override Auto-Selection" section
- Moved completed bytecode-vm-design.md to old-docs/
- Created continuation prompt for future sessions

---

## Phase 1: Add ParsingBackend Trait (COMPLETED)

### Implementation Summary

Created `portable/backend/` module with:

- **`mod.rs`**: Backend enum for runtime selection, module exports
- **`traits.rs`**: ParsingBackend trait, BackendCharacteristics struct, DynBackend type alias
- **`packrat.rs`**: PackratBackend wrapping PortableParser
- **`bytecode.rs`**: BytecodeBackend wrapping bytecode VM

### What Was Implemented

```rust
/// Common interface for parsing backends
pub trait ParsingBackend {
    fn parse(&mut self, grammar: &Grammar, input: &str) -> BackendResult;
    fn name(&self) -> &'static str;
    fn characteristics(&self) -> BackendCharacteristics;
    fn supports_streaming(&self) -> bool;
    fn supports_incremental(&self) -> bool;
    fn is_safe_for_all_grammars(&self) -> bool;
    fn parse_with_arena(&mut self, grammar: &Grammar, input: &str, arena: &mut AstArena) -> BackendResult;
}

pub struct BackendCharacteristics {
    pub time_complexity: &'static str,
    pub memory_complexity: &'static str,
    pub uses_memoization: bool,
    pub supports_streaming: bool,
    pub supports_incremental: bool,
    pub safe_for_all_grammars: bool,
}
```

---

## Phase 2: Split Largest Files (COMPLETED)

### Files Refactored

| Original File | Lines | New Structure | Max Lines |
|---------------|-------|---------------|-----------|
| `parser.rs` | 1802 | `parser/mod.rs`, `config.rs`, `context.rs`, `simd.rs`, `tests.rs` | 896 |
| `transform.rs` | 1599 | `transform/mod.rs`, `value.rs`, `pattern.rs`, `transform.rs`, `direct.rs`, `helpers.rs` | 390 |
| `bytecode/backend.rs` | 1512 | `backend/mod.rs`, `tests/` (mod.rs, basic.rs, complex.rs, captures.rs, parity.rs) | 570 |
| `parser_dsl.rs` | 1219 | `parser_dsl.rs`, `parser_dsl_tests.rs` | 982 |
| `bytecode/optimizer.rs` | 1074 | `optimizer.rs`, `optimizer_tests.rs` | 739 |

### All Main Source Files Under 1000 Lines ✅

Largest files after refactoring:
- `parser_dsl.rs`: 982 lines
- `bytecode/vm.rs`: 934 lines
- `parser/mod.rs`: 860 lines (reduced after removing unused imports)
- `streaming_builder.rs`: 891 lines
- `streaming.rs`: 882 lines

---

## Phase 2.1: Extract ResourceGovernor (COMPLETED)

### Problem: GOD CLASS

The `PortableParser` was a GOD CLASS with 14+ fields and mixed responsibilities:
- Grammar interpretation
- Input management
- Arena allocation
- Cache management
- Resource tracking (recursion, timeout, memory)

### Solution: Composition

Extracted `ResourceGovernor` to handle all resource limits:

```rust
// Before: GOD CLASS with 14 fields
pub struct PortableParser<'a> {
    grammar: &'a Grammar,
    input: &'a str,
    input_bytes: &'a [u8],
    arena: &'a mut AstArena,
    cache: DenseCache,
    cached_nodes: Vec<AstNode>,
    max_input_size: usize,        // Resource management
    max_recursion_depth: usize,   // Resource management
    current_depth: usize,         // Resource management
    timeout_ms: u64,              // Resource management
    start_time: Option<Instant>,  // Resource management
    op_count: usize,              // Resource management
    max_memory: usize,            // Resource management
}

// After: Composition with clear responsibilities
pub struct PortableParser<'a> {
    // Core parsing data
    grammar: &'a Grammar,
    input: &'a str,
    input_bytes: &'a [u8],
    arena: &'a mut AstArena,
    cache: DenseCache,
    cached_nodes: Vec<AstNode>,

    // Resource management delegated
    governor: ResourceGovernor,
}
```

### ResourceGovernor API

```rust
pub struct ResourceGovernor {
    max_input_size: usize,
    max_recursion_depth: usize,
    current_depth: usize,
    timeout_ms: u64,
    start_time: Option<Instant>,
    op_count: usize,
    max_memory: usize,
}

impl ResourceGovernor {
    // Builder pattern
    pub fn new() -> Self;
    pub fn with_max_input_size(self, size: usize) -> Self;
    pub fn with_max_recursion_depth(self, depth: usize) -> Self;
    pub fn with_timeout_ms(self, timeout_ms: u64) -> Self;
    pub fn with_max_memory(self, max_memory: usize) -> Self;

    // Resource checking
    pub fn check_input_size(&self, input_len: usize) -> Result<(), ParseError>;
    pub fn enter_recursive(&mut self) -> Result<(), ParseError>;
    pub fn exit_recursive(&mut self);
    pub fn start_timeout_timer(&mut self);
    pub fn check_timeout(&mut self) -> Result<(), ParseError>;
    pub fn check_memory(&self, current_usage: usize) -> Result<(), ParseError>;
    pub fn check_resources(&mut self, current_memory: usize) -> Result<(), ParseError>;
}
```

---

## Phase 2.2: Split Backend Tests Module (COMPLETED)

### Problem: Large Test File

The `bytecode/backend/tests.rs` file was 1256 lines, exceeding the 1000-line limit.

### Solution: Organized Test Submodules

Split into logical test categories under `backend/tests/`:

```
backend/tests/
├── mod.rs          (194 lines) - Shared grammar builders
├── basic.rs        (266 lines) - Backend selection, characteristics
├── complex.rs      (285 lines) - JSON-like, arithmetic grammars
├── captures.rs     (408 lines) - Named capture parity tests
└── parity.rs       ( 12 lines) - Additional parity tests
```

### Test Categories

- **basic.rs**: Backend selection, names, analysis, simple parsing
- **complex.rs**: JSON-like structures, arithmetic expressions, nested alternatives
- **captures.rs**: Named capture parity (position + AST structure)
- **parity.rs**: Additional parity tests

---

## Phase 3: Complete GOD CLASS Decomposition (IN PROGRESS)

### Remaining Work

The `PortableParser` still contains all parsing logic. For full OOP decomposition:

1. **Extract PackratBackend** - Move parsing methods to a proper backend implementation
2. **Create AtomParser trait** - Extensible atom parsing via trait objects
3. **Separate InputManager** - Handle input/byte indexing concerns

### Target Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    PortableParser (Facade)                   │
│  - Public API only                                          │
│  - Coordinates components                                   │
└─────────────────────────────────────────────────────────────┘
                              │
        ┌─────────────────────┼─────────────────────┐
        ▼                     ▼                     ▼
┌───────────────┐    ┌──────────────────┐    ┌────────────────┐
│ResourceGovernor│    │ PackratBackend   │    │  InputManager  │
│               │    │    (trait impl)  │    │                │
│ - depth limit │    │ - parse_str()    │    │ - input: &str  │
│ - timeout     │    │ - parse_re()     │    │ - bytes: &[u8] │
│ - memory      │    │ - parse_seq()    │    │ - utf8 helpers │
└───────────────┘    │ - uses governor  │    └────────────────┘
                     │ - uses cache     │
                     └──────────────────┘
```

---

## Phase 4: Module Renaming (FUTURE)

Optional renaming for clarity:
- `portable/` → `core/` (if desired)

---

## Phase 5: FFI Consolidation (FUTURE)

Organize FFI code under `ffi/` module for better separation.

---

## Testing Commands

```bash
# Build
cargo build --package parsanol

# Unit tests
cargo test --package parsanol --lib

# Check line counts
find parsanol/src -name "*.rs" -exec wc -l {} \; | sort -rn | head -20

# All tests (including doc tests)
cargo test --package parsanol
```

## Key Principles

1. **Preserve public API**: All public functions/types must remain accessible
2. **Use re-exports**: `mod.rs` should re-export everything needed
3. **Keep tests passing**: Run tests after each change
4. **Target <1000 lines per file**: Split into logical chunks
5. **MECE**: Modules should be mutually exclusive, collectively exhaustive
6. **Object-oriented**: Prefer trait-based abstractions and composition
7. **Single Responsibility**: Each component has one clear purpose
8. **No GOD CLASSes**: Decompose into focused, composable components
