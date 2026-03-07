# Design: Capture, Scope, and Dynamic Support

## Executive Summary

This document describes the design for implementing `capture`, `scope`, and `dynamic`
atoms in parsanol-rs for both native Rust users and via FFI callbacks to Ruby.

**Critical Constraint**: Parsanol has TWO parsing backends, and features must work
consistently across both.

## Current Architecture: Dual Backend System

### Backend Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           Grammar (Atom tree)                           │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                    ┌───────────────┴───────────────┐
                    ▼                               ▼
┌───────────────────────────────────┐ ┌───────────────────────────────────┐
│       Packrat Backend             │ │         Bytecode Backend          │
│       (PortableParser)            │ │         (BytecodeVM)              │
├───────────────────────────────────┤ ├───────────────────────────────────┤
│ • DenseCache for memoization      │ │ • Compiler → bytecode program     │
│ • O(n) guaranteed time            │ │ • Stack-based VM execution        │
│ • O(n × r) memory                 │ │ • O(n) to O(2^n) time             │
│ • Recursive descent               │ │ • O(d) memory (d = depth)         │
│ • Supports incremental            │ │ • Supports streaming              │
│ • ParseContext for state          │ │ • CaptureFrame infrastructure     │
└───────────────────────────────────┘ └───────────────────────────────────┘
                    │                               │
                    └───────────────┬───────────────┘
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                         Shared Infrastructure                           │
│  • Grammar (Atom enum)       • AstArena       • AstNode                │
│  • DynamicCallbackRegistry   • CaptureState   • FFI bindings           │
└─────────────────────────────────────────────────────────────────────────┘
```

### Backend Characteristics

| Characteristic | Packrat | Bytecode |
|---------------|---------|----------|
| Time Complexity | O(n) guaranteed | O(n) to O(2^n) |
| Memory Usage | O(n × r) | O(d) |
| Memoization | Yes (DenseCache) | No |
| Streaming | No | Yes |
| Incremental | Yes | No |
| Safe for all grammars | Yes | No (nested repetition risk) |
| Existing capture support | None | Partial (CaptureFrame) |

## Deep Reasoning Chain

### 1. Why Both Backends Need Support

The `Backend::Auto` selection means users don't know which backend runs. Features
must work identically on both:

```rust
// User code - backend is selected automatically
let mut parser = Parser::auto(grammar);
let result = parser.parse(input);  // Could use Packrat OR Bytecode
```

If capture/scope/dynamic only work on one backend, grammars using these features
would fail unpredictably.

### 2. Implementation Strategy Per Backend

**Packrat Backend (Recursive Descent):**

```
parse_atom(Atom::Capture { name, atom }, pos, ctx):
    result = parse_atom(atom, pos, ctx)
    if success:
        ctx.captures.insert(name, result)  // Direct storage
        ctx.capture_heights.push(ctx.captures.len())
    return result

parse_atom(Atom::Scope { atom }, pos, ctx):
    ctx.push_scope()                        // Stack push
    result = parse_atom(atom, pos, ctx)
    ctx.pop_scope()                         // Stack pop, restores visibility
    return result

parse_atom(Atom::Dynamic { callback_id }, pos, ctx):
    callback = REGISTRY.get(callback_id)
    new_atom = callback.resolve(input, pos, ctx.captures)
    return parse_atom(new_atom, pos, ctx)
```

**Bytecode Backend (Stack VM):**

```
Compile Capture { name, atom }:
    <compile atom>
    StoreCapture key_idx=name

Compile Scope { atom }:
    PushScope
    <compile atom>
    PopScope

Compile Dynamic { callback_id }:
    InvokeDynamic callback_id

VM Execution:
    StoreCapture: vm.captures[name] = vm.capture_stack.pop()
    PushScope: vm.scope_stack.push(vm.captures.snapshot())
    PopScope: vm.captures.restore(vm.scope_stack.pop())
    InvokeDynamic: callback = registry.get(id)
                   new_atom = callback.resolve(...)
                   // Must compile on-the-fly or use pre-compiled
```

### 3. Key Architectural Challenge: Dynamic in Bytecode

**Problem**: Bytecode compiles grammar once. Dynamic returns an Atom at runtime.

**Solutions**:

| Approach | Pros | Cons |
|----------|------|------|
| A: Compile dynamic result on-the-fly | Flexible | Slow, complex |
| B: Pre-compile all possible paths | Fast | Limited expressiveness |
| C: Fallback to Packrat for dynamic atoms | Correct, simple | Performance discontinuity |
| D: Hybrid - bytecode can call Packrat | Best of both | Complex interop |

**Recommendation**: **Approach D** - Bytecode VM has an `InvokePackrat` instruction
that delegates to Packrat for complex atoms. This is similar to how JIT compilers
fall back to interpreter for uncommon paths.

### 4. Capture State Must Be Shared

Both backends must use the same `CaptureState` structure:

```rust
/// Shared capture state - used by both backends
pub struct CaptureState {
    /// Named captures (keyed by string)
    captures: FxHashMap<String, CaptureValue>,

    /// Scope stack (indices into captures for restoration)
    scope_stack: Vec<ScopeFrame>,

    /// Current scope depth
    scope_depth: usize,
}

#[derive(Clone)]
pub struct ScopeFrame {
    /// Snapshot of capture names at scope entry
    visible_captures: Vec<String>,
}

#[derive(Clone)]
pub struct CaptureValue {
    /// The matched text
    pub text: String,
    /// Position information
    pub offset: usize,
    pub length: usize,
}
```

## Proposed Architecture

### Phase 1: Shared Capture Infrastructure

Create `portable/capture.rs`:

```rust
//! Capture state shared between Packrat and Bytecode backends

use rustc_hash::FxHashMap;

/// Capture value with position information
#[derive(Debug, Clone)]
pub struct CaptureValue {
    pub text: String,
    pub offset: usize,
    pub length: usize,
}

/// Scope frame for tracking visibility
#[derive(Debug, Clone)]
pub struct ScopeFrame {
    /// Captures that existed at scope entry
    captures_snapshot: Vec<String>,
}

/// Shared capture state
#[derive(Debug, Clone, Default)]
pub struct CaptureState {
    /// Named captures
    captures: FxHashMap<String, CaptureValue>,

    /// Scope stack
    scope_stack: Vec<ScopeFrame>,

    /// Current scope depth
    scope_depth: usize,
}

impl CaptureState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Store a capture in the current scope
    pub fn store(&mut self, name: &str, value: CaptureValue) {
        self.captures.insert(name.to_string(), value);
    }

    /// Retrieve a capture (searches from current scope upward)
    pub fn get(&self, name: &str) -> Option<&CaptureValue> {
        self.captures.get(name)
    }

    /// Push a new scope
    pub fn push_scope(&mut self) {
        let snapshot: Vec<String> = self.captures.keys().cloned().collect();
        self.scope_stack.push(ScopeFrame {
            captures_snapshot: snapshot,
        });
        self.scope_depth += 1;
    }

    /// Pop a scope, removing captures that were added in that scope
    pub fn pop_scope(&mut self) {
        if let Some(frame) = self.scope_stack.pop() {
            // Remove captures not in the snapshot
            self.captures.retain(|k, _| frame.captures_snapshot.contains(k));
        }
        self.scope_depth = self.scope_depth.saturating_sub(1);
    }

    /// Get all visible captures (for dynamic callbacks)
    pub fn all_captures(&self) -> &FxHashMap<String, CaptureValue> {
        &self.captures
    }

    /// Check if we're in a scope
    pub fn in_scope(&self) -> bool {
        self.scope_depth > 0
    }
}
```

### Phase 2: Extend Grammar AST

Add to `portable/grammar.rs`:

```rust
pub enum Atom {
    // ... existing variants ...

    /// Capture the result with a name for later reference
    Capture {
        name: String,
        atom: usize,
    },

    /// Create a new capture scope
    Scope {
        atom: usize,
    },

    /// Dynamic atom - evaluates at parse time
    Dynamic {
        callback_id: u64,
    },
}
```

### Phase 3: Dynamic Callback Registry

Create `portable/dynamic.rs`:

```rust
//! Dynamic callback infrastructure

use super::capture::CaptureState;
use super::grammar::Atom;
use std::sync::{Mutex, OnceLock};

/// Trait for dynamic parsing callbacks
pub trait DynamicCallback: Send + Sync {
    /// Resolve to an atom at parse time
    fn resolve(&self, input: &str, pos: usize, captures: &CaptureState) -> Option<Atom>;
}

/// Global registry
static DYNAMIC_REGISTRY: OnceLock<Mutex<DynamicRegistry>> = OnceLock::new();

struct DynamicRegistry {
    callbacks: FxHashMap<u64, Box<dyn DynamicCallback>>,
    next_id: u64,
}

pub fn register_dynamic_callback(callback: Box<dyn DynamicCallback>) -> u64 {
    let registry = DYNAMIC_REGISTRY.get_or_init(|| Mutex::new(DynamicRegistry {
        callbacks: FxHashMap::default(),
        next_id: 1_000_000, // Start high to avoid conflicts
    }));

    let mut guard = registry.lock().unwrap();
    let id = guard.next_id;
    guard.next_id += 1;
    guard.callbacks.insert(id, callback);
    id
}

pub fn invoke_dynamic(id: u64, input: &str, pos: usize, captures: &CaptureState) -> Option<Atom> {
    let registry = DYNAMIC_REGISTRY.get()?;
    let guard = registry.lock().ok()?;
    guard.callbacks.get(&id)?.resolve(input, pos, captures)
}
```

### Phase 4: Packrat Backend Integration

Modify `PortableParser` to use `CaptureState`:

```rust
pub struct PortableParser<'a> {
    // ... existing fields ...

    /// Capture state for capture/scope/dynamic
    capture_state: CaptureState,
}

impl<'a> PortableParser<'a> {
    fn parse_atom(&mut self, atom_idx: usize, pos: usize) -> ParseResult {
        let atom = self.grammar.get_atom(atom_idx);

        match atom {
            // ... existing cases ...

            Atom::Capture { name, atom } => {
                let result = self.parse_atom(*atom, pos)?;
                if result.is_success() {
                    let value = CaptureValue {
                        text: self.input[pos..result.end_pos].to_string(),
                        offset: pos,
                        length: result.end_pos - pos,
                    };
                    self.capture_state.store(name, &value);
                }
                result
            }

            Atom::Scope { atom } => {
                self.capture_state.push_scope();
                let result = self.parse_atom(*atom, pos);
                self.capture_state.pop_scope();
                result
            }

            Atom::Dynamic { callback_id } => {
                let dynamic_atom = invoke_dynamic(
                    *callback_id,
                    self.input,
                    pos,
                    &self.capture_state,
                );

                match dynamic_atom {
                    Some(atom) => {
                        // Add atom to grammar temporarily and parse
                        let temp_idx = self.grammar.add_temp_atom(atom);
                        self.parse_atom(temp_idx, pos)
                    }
                    None => ParseResult::failure(pos),
                }
            }
        }
    }
}
```

### Phase 5: Bytecode Backend Integration

**Compiler changes** (`bytecode/compiler.rs`):

```rust
fn compile_atom(&mut self, atom_idx: usize) -> Result<usize, CompileError> {
    let atom = self.grammar.get_atom(atom_idx);

    match atom {
        // ... existing cases ...

        Atom::Capture { name, atom } => {
            let entry = self.compile_atom(*atom)?;
            let key_idx = self.program.add_key(name);
            self.program.add_instruction(Instruction::StoreCapture { key_idx });
            Ok(entry)
        }

        Atom::Scope { atom } => {
            let entry = self.program.instruction_count();
            self.program.add_instruction(Instruction::PushScope);
            self.compile_atom(*atom)?;
            self.program.add_instruction(Instruction::PopScope);
            Ok(entry)
        }

        Atom::Dynamic { callback_id } => {
            let entry = self.program.instruction_count();
            self.program.add_instruction(Instruction::InvokeDynamic { callback_id: *callback_id });
            Ok(entry)
        }
    }
}
```

**VM changes** (`bytecode/vm.rs`):

```rust
pub struct BytecodeVM<'a> {
    // ... existing fields ...

    /// Capture state (shared structure with Packrat)
    capture_state: CaptureState,
}

impl<'a> BytecodeVM<'a> {
    fn execute_instruction(&mut self, instr: &Instruction) -> Result<ExecutionResult, ParseError> {
        match instr {
            // ... existing cases ...

            Instruction::StoreCapture { key_idx } => {
                let name = self.program.get_key(*key_idx);
                if let Some(frame) = self.capture_stack.last() {
                    let value = CaptureValue {
                        text: self.input_str[frame.start_pos..frame.end_pos].to_string(),
                        offset: frame.start_pos,
                        length: frame.end_pos - frame.start_pos,
                    };
                    self.capture_state.store(name, value);
                }
                Ok(ExecutionResult::Continue)
            }

            Instruction::PushScope => {
                self.capture_state.push_scope();
                Ok(ExecutionResult::Continue)
            }

            Instruction::PopScope => {
                self.capture_state.pop_scope();
                Ok(ExecutionResult::Continue)
            }

            Instruction::InvokeDynamic { callback_id } => {
                let dynamic_atom = invoke_dynamic(
                    *callback_id,
                    self.input_str,
                    self.position,
                    &self.capture_state,
                );

                match dynamic_atom {
                    Some(atom) => {
                        // CRITICAL: Must handle dynamic atom compilation
                        // Option D: Delegate to Packrat for this subtree
                        self.execute_dynamic_via_packrat(atom)
                    }
                    None => Ok(ExecutionResult::Fail),
                }
            }
        }
    }

    fn execute_dynamic_via_packrat(&mut self, atom: Atom) -> Result<ExecutionResult, ParseError> {
        // Create a temporary grammar with the dynamic atom as root
        let temp_grammar = Grammar {
            atoms: vec![atom],
            root: 0,
        };

        // Use Packrat for this subtree
        let mut arena = AstArena::for_input(self.input_str.len() - self.position);
        let mut parser = PortableParser::new(&temp_grammar, &self.input_str[self.position..], &mut arena);

        match parser.parse() {
            Ok(ast) => {
                // Update position based on Packrat result
                self.position = parser.end_position();
                // Push result to capture stack
                // ...
                Ok(ExecutionResult::Continue)
            }
            Err(_) => Ok(ExecutionResult::Fail),
        }
    }
}
```

### Phase 6: Ruby FFI Integration

**Rust side** (`ffi/ruby/dynamic.rs`):

```rust
use magnus::{Error, Ruby, Value};
use crate::portable::capture::CaptureState;
use crate::portable::grammar::Atom;

/// Ruby dynamic callback wrapper
pub struct RubyDynamicCallback {
    proc: Value,
}

impl DynamicCallback for RubyDynamicCallback {
    fn resolve(&self, input: &str, pos: usize, captures: &CaptureState) -> Option<Atom> {
        let ruby = Ruby::get().ok()?;

        // Build Ruby hash from captures
        let captures_hash = ruby.hash_new();
        for (name, value) in captures.all_captures() {
            let slice = create_slice_object(&ruby, input, value.offset, value.length);
            let _ = captures_hash.aset(name.as_str(), slice);
        }

        // Call Ruby proc: proc.call(pos, captures_hash)
        let result: Value = self.proc
            .funcall("call", (pos, captures_hash))
            .ok()?;

        // Convert result back to Atom
        ruby_value_to_atom(&ruby, result, input, pos)
    }
}

/// Register a Ruby proc as a dynamic callback
pub fn register_ruby_dynamic(proc: Value) -> Result<u64, Error> {
    let callback = RubyDynamicCallback { proc };
    Ok(register_dynamic_callback(Box::new(callback)))
}
```

**Ruby side** (`lib/parsanol/native/dynamic.rb`):

```ruby
module Parsanol
  module Native
    module Dynamic
      # Registry to keep references alive (prevent GC)
      @callbacks = {}
      @next_id = 1_000_000

      def self.register(block)
        id = @next_id
        @next_id += 1
        @callbacks[id] = block
        Native.register_dynamic_callback(id, block)
        id
      end

      def self.unregister(id)
        @callbacks.delete(id)
        Native.unregister_dynamic_callback(id)
      end
    end
  end
end
```

**Serializer update** (`lib/parsanol/native/serializer.rb`):

```ruby
def serialize_capture(atom)
  {
    'Capture' => {
      'name' => atom.capture_key.to_s,
      'atom' => serialize_atom(atom.inner_atom)
    }
  }
end

def serialize_scope(atom)
  inner = atom.block.call rescue nil
  return serialize_unknown(atom) unless inner

  {
    'Scope' => {
      'atom' => serialize_atom(inner)
    }
  }
end

def serialize_dynamic(atom)
  # Register the Ruby block and get a callback ID
  callback_id = Parsanol::Native::Dynamic.register(atom.block)

  {
    'Dynamic' => {
      'callback_id' => callback_id
    }
  }
end
```

## Implementation Plan

### Milestone 1: Shared Infrastructure (2 days)

1. Create `portable/capture.rs` with `CaptureState`
2. Create `portable/dynamic.rs` with `DynamicCallback` trait
3. Add `Capture`, `Scope`, `Dynamic` to `Atom` enum
4. Add serde serialization for new atoms
5. Write unit tests for `CaptureState`

### Milestone 2: Packrat Backend (1.5 days)

1. Add `capture_state` field to `PortableParser`
2. Implement capture/scope/dynamic atom handling
3. Handle dynamic callback returning atoms
4. Write backend-specific tests

### Milestone 3: Bytecode Backend (2.5 days)

1. Add `StoreCapture`, `PushScope`, `PopScope`, `InvokeDynamic` instructions
2. Update compiler to generate new instructions
3. Add `capture_state` to `BytecodeVM`
4. Implement instruction execution
5. Implement Packrat fallback for dynamic atoms
6. Write backend-specific tests

### Milestone 4: Parser DSL (1 day)

```rust
pub fn capture(name: &str, atom: impl Parslet) -> CaptureParslet;
pub fn scope(atom: impl Parslet) -> ScopeParslet;
pub fn dynamic<F>(f: F) -> DynamicParslet where F: Fn(&str, usize, &CaptureState) -> Option<Box<dyn Parslet>> + Send + Sync;
```

### Milestone 5: Ruby FFI (2 days)

1. Add `register_ruby_dynamic` to FFI
2. Create `Parsanol::Native::Dynamic` Ruby module
3. Update serializer for new atoms
4. Handle GC safety for Ruby procs
5. Write integration tests

### Milestone 6: Testing & Documentation (1 day)

1. Cross-backend parity tests
2. Performance benchmarks
3. Update README
4. Update website compatibility table

**Total: ~10 days**

## Edge Cases

### 1. Dynamic Returns Atom with Dynamic

```rust
dynamic { |ctx| Some(dynamic { |ctx2| Some(str("a")) }) }
```

**Solution**: Recursive handling. Each `InvokeDynamic` calls the registry again.

### 2. Capture in Failed Alternative

```rust
(str("a").capture("x") | str("b"))
```

If `"a"` matches but subsequent parse fails, capture should not persist.

**Solution**: Backtracking restores capture stack height in both backends.

### 3. Cross-Backend Parity

Same grammar must produce same result on both backends.

**Solution**: Shared `CaptureState` ensures identical behavior. Parity tests required.

### 4. Thread Safety

Multiple threads parsing with same grammar but different captures.

**Solution**: `CaptureState` is per-parse, not per-grammar. Registry uses `Mutex`.

### 5. FFI GC Safety

Ruby proc must not be garbage collected while registered.

**Solution**: `Parsanol::Native::Dynamic` keeps strong reference in `@callbacks` hash.

### 6. Capture in Lookahead

```rust
// Positive lookahead with capture
str("a").present?.capture("x")  // Does x persist?

// Negative lookahead with capture
str("a").absent?.capture("y")   // Never executes, but what if it did?
```

**Solution**: Lookahead executes in isolated capture scope. Captures inside lookahead
are discarded after the lookahead completes (both success and failure).

```rust
Atom::Lookahead { atom, positive } => {
    self.capture_state.push_scope();  // Isolate captures
    let result = self.parse_atom(*atom, pos);
    self.capture_state.pop_scope();   // Discard lookahead captures
    // ... rest of lookahead logic
}
```

### 7. Incremental Parsing with Captures

Packrat supports incremental parsing with cache reuse. How do captures interact?

**Problem**: Captures depend on input content, which may change between parses.

**Solution**: Captures are NOT cached. Only parse results are memoized. On incremental
re-parse, captures are recomputed even if the parse result is cached.

```rust
// In cache hit scenario
if let Some(cached) = self.cache.get(pos, atom_idx) {
    // Recompute captures even on cache hit
    if let Atom::Capture { name, .. } = atom {
        self.capture_state.store(name, extract_capture_value(cached));
    }
    return cached;
}
```

### 8. Streaming with Captures

Bytecode supports streaming. What happens to captures when input is incomplete?

**Solution**: Captures are accumulated during streaming. On `StreamingResult::Incomplete`,
captures are preserved for continuation. On `StreamingResult::Error`, captures are
discarded.

```rust
pub struct StreamingCaptureState {
    /// Active captures
    captures: CaptureState,
    /// Whether we're in a continuation
    is_continuation: bool,
}

impl StreamingParser {
    fn continue_parse(&mut self, chunk: &str) -> StreamingResult {
        // Captures persist across chunks
        // ...
    }
}
```

### 9. Dynamic Callback Panics

What if a Rust dynamic callback panics?

**Solution**: Use `catch_unwind` around callback invocation:

```rust
pub fn invoke_dynamic(id: u64, input: &str, pos: usize, captures: &CaptureState) -> Option<Atom> {
    let registry = DYNAMIC_REGISTRY.get()?;
    let guard = registry.lock().ok()?;

    std::panic::catch_unwind(|| {
        guard.callbacks.get(&id)?.resolve(input, pos, captures)
    }).ok().flatten()
}
```

### 10. Dynamic Callback Returns Invalid Atom

What if dynamic returns an atom that references non-existent indices?

**Solution**: Validate atom before parsing:

```rust
fn validate_atom(atom: &Atom, grammar: &Grammar) -> bool {
    match atom {
        Atom::Entity { atom_idx } => *atom_idx < grammar.atom_count(),
        Atom::Sequence { atoms } => atoms.iter().all(|i| *i < grammar.atom_count()),
        // ... etc
        _ => true,
    }
}
```

### 11. WASM Support

How do dynamic callbacks work in WASM where there's no Rust trait objects?

**Problem**: WASM can't use Rust trait objects across the boundary.

**Solution**: Use integer IDs with pre-registered callbacks, similar to CustomAtom:

```rust
// WASM-specific API
#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn register_wasm_dynamic(callback_id: u64) {
    // Store callback_id, actual logic invoked via wasm-bindgen closures
}

// At parse time, look up by ID
#[cfg(feature = "wasm")]
fn invoke_wasm_dynamic(id: u64, input: &str, pos: usize, captures_json: &str) -> Option<Atom> {
    // Call back into JavaScript via wasm-bindgen
    // Returns JSON representation of Atom
}
```

**Limitation**: Dynamic callbacks in WASM require JavaScript. Pure WASM without JS
host cannot use dynamic atoms.

### 12. Grammar Serialization for Ruby FFI

The `Dynamic` atom has a `callback_id` which is runtime-specific. How does this
serialize to JSON?

**Problem**: `callback_id` is meaningless across process boundaries.

**Solution**: Dynamic atoms are NOT serializable. Ruby serializer registers the
callback at serialization time, and the ID is only valid for the current process:

```ruby
def serialize_dynamic(atom)
  # Register NOW, get ID valid for this process
  callback_id = Parsanol::Native::Dynamic.register(atom.block)

  {
    'Dynamic' => {
      'callback_id' => callback_id
    }
  }
end
```

**Implication**: Grammars with dynamic atoms cannot be cached to disk and reused
across Ruby process restarts. This is documented as a limitation.

### 13. Self-Referential Dynamic

What if dynamic returns an atom that references itself?

```rust
dynamic { |ctx| Some(ref_("self"))) }  // Where "self" is the dynamic itself
```

**Solution**: Packrat handles this naturally via memoization. Bytecode's Packrat
fallback also handles it. No special case needed.

## Performance Considerations

| Operation | Packrat | Bytecode |
|-----------|---------|----------|
| Capture store | ~30ns (hash insert) | ~50ns (instr + hash) |
| Scope push/pop | ~20ns (stack ops) | ~30ns (2 instructions) |
| Dynamic (Rust) | ~200ns (trait call) | ~500ns (Packrat fallback) |
| Dynamic (Ruby) | ~5µs (FFI) | ~10µs (FFI + fallback) |

## Success Criteria

1. ✅ Features work on Packrat backend
2. ✅ Features work on Bytecode backend
3. ✅ Same results on both backends (parity tests)
4. ✅ FFI callbacks work from Ruby
5. ✅ No memory leaks (valgrind/MIRI)
6. ✅ Performance within targets
7. ✅ Documentation complete

## Testing Strategy

### Unit Tests

1. **CaptureState tests** (`capture.rs`):
   - Store and retrieve captures
   - Scope push/pop behavior
   - Nested scopes with shadowing
   - Empty scope handling

2. **DynamicCallback tests** (`dynamic.rs`):
   - Registration and invocation
   - Thread safety
   - Panic handling

### Integration Tests

1. **Packrat backend tests**:
   - Basic capture/scope/dynamic
   - Backtracking with captures
   - Dynamic returning various atom types

2. **Bytecode backend tests**:
   - Same tests as Packrat
   - Packrat fallback for dynamic
   - Streaming with captures

3. **Cross-backend parity tests**:
   ```rust
   #[test]
   fn test_capture_parity() {
       let grammar = /* grammar with capture */;
       let input = "test input";

       let packrat_result = Parser::packrat(grammar.clone()).parse(input);
       let bytecode_result = Parser::bytecode(grammar).parse(input);

       assert_eq!(packrat_result, bytecode_result);
   }
   ```

4. **FFI tests** (Ruby):
   - Ruby block as dynamic callback
   - GC safety (stress test with GC)
   - Capture visibility from Ruby

### Property Tests

```rust
proptest! {
    #[test]
    fn capture_backtrack_invariant(input: String, grammar: Grammar) {
        // If parse fails, no captures should persist
        let mut state = CaptureState::new();
        let result = parse_with_captures(&grammar, &input, &mut state);

        if result.is_err() {
            prop_assert!(state.all_captures().is_empty());
        }
    }
}
```

### Performance Tests

```rust
#[bench]
fn bench_capture_store(b: &mut Bencher) {
    let mut state = CaptureState::new();
    b.iter(|| state.store("x", CaptureValue { /* ... */ }));
}

#[bench]
fn bench_dynamic_invoke(b: &mut Bencher) {
    let callback = /* simple callback */;
    let id = register_dynamic_callback(Box::new(callback));
    b.iter(|| invoke_dynamic(id, "input", 0, &CaptureState::new()));
}
```

## Open Questions

### 1. Typed Captures?

Currently captures store strings. Should we support typed values?

```rust
pub enum CaptureValue {
    String(String, usize, usize),  // text, offset, length
    Int(i64),
    Float(f64),
    Bool(bool),
    // ...
}
```

**Trade-off**: More flexible but more complex FFI.

**Decision**: Defer to future. Start with string-only.

### 2. Capture in Error Messages?

Should capture values appear in parse error messages?

```rust
// Grammar captures "identifier"
// Parse fails later
// Error message: "expected ';' after identifier 'foo'"
```

**Decision**: Yes, but as a future enhancement. Not in initial implementation.

### 3. Capture Inspection API?

Should we provide a way to inspect captures mid-parse for debugging?

```rust
// Hypothetical API
parser.inspect_captures(|state| {
    eprintln!("Current captures: {:?}", state.all_captures());
});
```

**Decision**: Yes, useful for debugging. Add to both backends.

### 4. Streaming Capture Persistence?

In streaming mode, should captures persist across chunk boundaries?

```rust
// Chunk 1: matches "foo", captures as :x
// Chunk 2: continues parsing
// Is :x still visible in chunk 2?
```

**Decision**: Yes, captures persist across streaming chunks. This matches user expectations.

### 5. Maximum Scope Depth?

Should we limit scope nesting to prevent stack overflow?

**Decision**: Yes, add `MAX_SCOPE_DEPTH = 1000` constant. Configurable via `VMConfig`.

## File Changes Summary

| File | Change |
|------|--------|
| `portable/capture.rs` | **NEW** - `CaptureState`, `CaptureValue`, `ScopeFrame` |
| `portable/dynamic.rs` | **NEW** - `DynamicCallback` trait, registry |
| `portable/grammar.rs` | Add `Capture`, `Scope`, `Dynamic` to `Atom` enum |
| `portable/parser/mod.rs` | Add `capture_state` field, atom handlers |
| `bytecode/instruction.rs` | Add `StoreCapture`, `PushScope`, `PopScope`, `InvokeDynamic` |
| `bytecode/compiler.rs` | Compile new atoms to instructions |
| `bytecode/vm.rs` | Add `capture_state`, execute new instructions |
| `ffi/ruby/dynamic.rs` | **NEW** - `RubyDynamicCallback`, registration |
| `ffi/ruby/init.rs` | Export `register_ruby_dynamic` |
| Ruby `serializer.rb` | Update for new atoms |
| Ruby `dynamic.rb` | **NEW** - `Parsanol::Native::Dynamic` module |

## Conclusion

The dual-backend architecture requires careful design to ensure feature parity.
The key insight is:

1. **Shared state** (`CaptureState`) ensures consistent behavior
2. **Bytecode uses Packrat fallback** for dynamic atoms (hybrid approach)
3. **FFI bridges to Ruby** via callback registry

This approach provides:
- Consistent behavior across backends
- Native Rust performance when possible
- Ruby flexibility when needed
- Maintainable, testable code

---

**Document Status**: Ready for team review

**Estimated Implementation**: 10 days

**Risk Level**: Medium (cross-backend parity is critical)
