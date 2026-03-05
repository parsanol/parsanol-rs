# Migration Guide: 0.1.6 to 0.2.0

Version 0.2.0 includes breaking changes to the FFI module organization and removes deprecated components. This guide helps you migrate your code.

## Summary of Changes

| Area | 0.1.6 | 0.2.0 |
|------|-------|-------|
| FFI utilities | `parsanol::portable::ffi` | `parsanol::ffi` |
| C ABI | `parsanol::portable::c_ffi` | `parsanol::ffi::c` |
| Ruby FFI | `parsanol::ruby_ffi` | `parsanol::ffi::ruby` |
| Lexer | `parsanol::ruby_ffi::drop_lexer` | **Removed** |
| `once_cell` feature | Available | **Removed** |

## Breaking Changes

### 1. FFI Module Reorganization

The FFI modules have been consolidated under a unified `ffi/` module structure.

**Before (0.1.6):**
```rust
use parsanol::portable::ffi::{
    flatten_ast_to_u64, parse_to_flat,
    TAG_NIL, TAG_BOOL, TAG_INT, TAG_STRING,
};
use parsanol::portable::c_ffi::{
    parsanol_grammar_new, parsanol_parse, parsanol_result_free,
    ParsanolGrammar, ParsanolResult,
};
```

**After (0.2.0):**
```rust
// FFI utilities are now at parsanol::ffi
use parsanol::ffi::{
    flatten_ast_to_u64, parse_to_flat,
    TAG_NIL, TAG_BOOL, TAG_INT, TAG_STRING,
};

// C ABI is now at parsanol::ffi::c
use parsanol::ffi::c::{
    parsanol_grammar_new, parsanol_parse, parsanol_result_free,
    ParsanolGrammar, ParsanolResult,
};
```

**Note:** For convenience, FFI utilities are also re-exported from `parsanol::portable`:
```rust
// This still works (re-exported)
use parsanol::portable::{
    flatten_ast_to_u64, parse_to_flat,
    TAG_NIL, TAG_BOOL, TAG_INT, TAG_STRING,
};
```

### 2. C ABI Function Paths

All C ABI functions have moved to `parsanol::ffi::c`:

| 0.1.6 Path | 0.2.0 Path |
|------------|------------|
| `parsanol::portable::c_ffi::parsanol_grammar_new` | `parsanol::ffi::c::parsanol_grammar_new` |
| `parsanol::portable::c_ffi::parsanol_grammar_new_with_error` | `parsanol::ffi::c::parsanol_grammar_new_with_error` |
| `parsanol::portable::c_ffi::parsanol_grammar_free` | `parsanol::ffi::c::parsanol_grammar_free` |
| `parsanol::portable::c_ffi::parsanol_parse` | `parsanol::ffi::c::parsanol_parse` |
| `parsanol::portable::c_ffi::parsanol_parse_simple` | `parsanol::ffi::c::parsanol_parse_simple` |
| `parsanol::portable::c_ffi::parsanol_result_end_pos` | `parsanol::ffi::c::parsanol_result_end_pos` |
| `parsanol::portable::c_ffi::parsanol_result_free` | `parsanol::ffi::c::parsanol_result_free` |
| `parsanol::portable::c_ffi::ParsanolGrammar` | `parsanol::ffi::c::ParsanolGrammar` |
| `parsanol::portable::c_ffi::ParsanolResult` | `parsanol::ffi::c::ParsanolResult` |

### 3. Ruby FFI

The Ruby FFI is still available at `parsanol::ruby_ffi` for backward compatibility, but the canonical location is now `parsanol::ffi::ruby`.

**Before (0.1.6):**
```rust
use parsanol::ruby_ffi::{parse_to_ruby_objects, parse_batch};
```

**After (0.2.0):**
```rust
// Both paths work
use parsanol::ruby_ffi::{parse_to_ruby_objects, parse_batch};  // Backward compatible
use parsanol::ffi::ruby::{parse_to_ruby_objects, parse_batch}; // Canonical
```

### 4. Removed: Lexer Module

The standalone lexer module has been removed. If you need tokenization, use the parser directly with appropriate grammar rules.

**Before (0.1.6):**
```rust
use parsanol::ruby_ffi::drop_lexer;
// or
let lexer = Parsanol::Lexer.new(grammar_json);
```

**After (0.2.0):**
```rust
// Use the parser directly instead
use parsanol::portable::{Grammar, PortableParser, AstArena};

let grammar: Grammar = serde_json::from_str(grammar_json)?;
let mut arena = AstArena::for_input(input.len());
let mut parser = PortableParser::new(&grammar, input, &mut arena);
let ast = parser.parse()?;
```

### 5. Removed: `once_cell` Feature

The `once_cell` feature has been removed. If you were using it, migrate to `std::sync::OnceLock` (available in Rust 1.70+) or the `once_cell` crate directly.

**Before (0.1.6):**
```toml
# Cargo.toml
[dependencies]
parsanol = { version = "0.1", features = ["once_cell"] }
```

**After (0.2.0):**
```toml
# Cargo.toml
[dependencies]
parsanol = "0.2"
# Add once_cell directly if needed
once_cell = "1.0"
```

## New Features in 0.2.0

### Backend Abstraction

A new trait-based backend system allows choosing between parsing strategies:

```rust
use parsanol::portable::{Backend, ParsingBackend, PackratBackend, BytecodeBackend};

// Automatically select best backend for grammar
let backend = Backend::default_for_grammar(&grammar);

// Or explicitly choose
let packrat = PackratBackend;
let bytecode = BytecodeBackend::new();
```

### Bytecode VM Backend

For grammars with linear patterns (no nested repetition), the bytecode VM can provide better performance:

```rust
use parsanol::portable::{BytecodeBackend, Grammar};
use parsanol::portable::bytecode::{compile_bytecode, parse_with_vm};

let grammar: Grammar = /* ... */;
let backend = BytecodeBackend::new();

// The backend automatically compiles and runs bytecode when appropriate
```

## Migration Checklist

- [ ] Update `Cargo.toml` to use `parsanol = "0.2"`
- [ ] Replace `parsanol::portable::ffi::*` imports with `parsanol::ffi::*`
- [ ] Replace `parsanol::portable::c_ffi::*` imports with `parsanol::ffi::c::*`
- [ ] Remove any usage of `parsanol::ruby_ffi::drop_lexer`
- [ ] Remove `once_cell` feature from Cargo.toml if used
- [ ] Run tests and fix any compilation errors
- [ ] Consider using the new backend abstraction for performance-critical code

## Need Help?

If you encounter issues migrating:
1. Check the [documentation](https://docs.rs/parsanol)
2. Open an issue on [GitHub](https://github.com/parsanol/parsanol-rs/issues)
