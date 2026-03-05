# Backend Selection Guide

This guide helps you choose the best parsing backend for your grammar.

## Quick Reference

| Grammar Pattern | Recommended Backend | Performance |
|----------------|---------------------|-------------|
| Simple tokens (`true`, `"string"`, `123`) | Packrat | 2-5x faster |
| Expressions with precedence (`1+2*3`) | **Bytecode** | **10-30x faster** |
| Nested repetition (`(a*)*`) | Packrat | Safer |
| Linear sequences (`url`, `email`) | Either | Comparable |
| Recursive structures (`sexp`, `xml`) | Bytecode | 1.2-1.5x faster |

## Backend Characteristics

### Packrat Backend

**Strengths:**
- Guaranteed O(n) time complexity
- Predictable performance
- No exponential blowup risk
- Lower overhead for simple patterns

**Best for:**
- Grammars with nested repetition
- Safety-critical applications
- Simple token matching
- When worst-case latency matters

**Example:**
```rust
use parsanol::portable::{PackratBackend, ParsingBackend, Grammar};

let grammar: Grammar = /* ... */;
let mut backend = PackratBackend::new();
let result = backend.parse(&grammar, input)?;
```

### Bytecode VM Backend

**Strengths:**
- 10-30x faster for expressions
- Lower memory usage
- Better for complex grammars
- Stack-based recursion efficient

**Best for:**
- Expression parsing (calculators, DSLs)
- Recursive structures (S-expressions, XML)
- Memory-constrained environments
- When throughput matters

**Caution:**
- Potential exponential time for grammars with nested repetition
- Auto-selection avoids this by detecting risky patterns

**Example:**
```rust
use parsanol::portable::{BytecodeBackend, ParsingBackend, Grammar};

let grammar: Grammar = /* ... */;
let mut backend = BytecodeBackend::new();
let result = backend.parse(&grammar, input)?;
```

## Automatic Selection

Use automatic selection to get the best of both worlds:

```rust
use parsanol::portable::Backend;

// Automatic selection based on grammar analysis
let backend = Backend::default_for_grammar(&grammar);

match backend {
    Backend::Packrat => println!("Using Packrat (nested repetition detected)"),
    Backend::Bytecode => println!("Using Bytecode (linear patterns)"),
    Backend::Auto => unreachable!(),
}
```

The automatic selection uses this logic:
1. Analyze grammar for nested repetition patterns
2. If nested repetition detected → Packrat (safe)
3. Otherwise → Bytecode (fast)

## Detailed Performance Analysis

### Simple Token Matching

| Input | Packrat | Bytecode | Winner |
|-------|---------|----------|--------|
| `"true"` (4 bytes) | 222 ns | 1,127 ns | Packrat (5x) |
| `"false"` (5 bytes) | 225 ns | 6,102 ns | Packrat (27x) |
| `"null"` (4 bytes) | 229 ns | 5,981 ns | Packrat (26x) |

**Why?** Packrat has lower overhead for trivial matches. Bytecode has VM dispatch overhead.

**Recommendation:** Use Packrat for lexing/tokenization tasks.

### Expression Parsing

| Input | Packrat | Bytecode | Winner |
|-------|---------|----------|--------|
| `"1+2"` | 3,882 ns | 865 ns | **Bytecode (4.5x)** |
| `"1+2*3"` | 10,875 ns | 1,032 ns | **Bytecode (10.5x)** |
| `"(1+2)*3"` | 15,429 ns | 1,090 ns | **Bytecode (14x)** |
| Complex expr (55 bytes) | 54,868 ns | 1,727 ns | **Bytecode (32x)** |

**Why?** Bytecode VM handles precedence climbing efficiently without memoization overhead.

**Recommendation:** Use Bytecode for calculators, expression languages, DSLs.

### Nested Structures

| Input | Packrat | Bytecode | Winner |
|-------|---------|----------|--------|
| `"(+ 1 2)"` | 4,478 ns | 3,737 ns | **Bytecode (1.2x)** |
| `"(* (+ 1 2) (- 4 3))"` | 9,035 ns | 5,839 ns | **Bytecode (1.5x)** |
| `"((()))"` | 1,175 ns | 896 ns | **Bytecode (1.3x)** |

**Why?** Stack-based recursion is efficient for balanced structures.

**Recommendation:** Use Bytecode for S-expressions, XML, JSON-like structures.

### Linear Patterns

| Pattern | Packrat | Bytecode | Winner |
|---------|---------|----------|--------|
| URL | 5-8 µs | 5-7 µs | Tie |
| Email | 5-9 µs | 5-8 µs | Tie |
| IP address | 12.8 µs | 13.8 µs | Packrat (1.1x) |
| ISO date | 4.4 µs | 5.1 µs | Packrat (1.2x) |

**Recommendation:** Either backend works well. Use automatic selection.

## Memory Comparison

| Backend | Memory Usage | Notes |
|---------|--------------|-------|
| Packrat | O(n × r) | n = input length, r = rule count |
| Bytecode | O(d) | d = nesting depth |

**Recommendation:** Use Bytecode for memory-constrained environments (embedded, WASM).

## Decision Tree

```
                    ┌─────────────────────┐
                    │ Does your grammar   │
                    │ have nested         │
                    │ repetition?         │
                    │ (e.g., (a*)*)       │
                    └──────────┬──────────┘
                               │
               ┌───────────────┼───────────────┐
               │ Yes                          │ No
               ▼                              ▼
        ┌──────────────┐              ┌──────────────────┐
        │ Use Packrat  │              │ Is it an         │
        │ (guaranteed  │              │ expression with  │
        │ O(n))        │              │ precedence?      │
        └──────────────┘              └────────┬─────────┘
                                               │
                               ┌───────────────┼───────────────┐
                               │ Yes                          │ No
                               ▼                              ▼
                        ┌──────────────┐              ┌──────────────┐
                        │ Use Bytecode │              │ Either works │
                        │ (10-30x      │              │ well. Use    │
                        │ faster)      │              │ auto-select. │
                        └──────────────┘              └──────────────┘
```

## Example Use Cases

### Calculator Language → Bytecode
```rust
// Expressions with precedence benefit massively from Bytecode
let grammar = build_calculator_grammar();
let mut backend = BytecodeBackend::new(); // 30x faster!
```

### JSON Parser → Packrat (or auto)
```rust
// JSON has nested structures but not nested repetition
// Either backend works, Packrat slightly faster for simple values
let grammar = build_json_grammar();
let backend = Backend::default_for_grammar(&grammar);
```

### Template Language → Packrat
```rust
// Templates with nested loops like {{#each items}}{{#each tags}}{{/each}}{{/each}}
// have nested repetition - use Packrat
let grammar = build_template_grammar();
let mut backend = PackratBackend::new(); // Safe O(n)
```

### Configuration File → Either
```rust
// INI, TOML-like configs are linear
let grammar = build_config_grammar();
let backend = Backend::default_for_grammar(&grammar); // Auto-select
```

## Running Your Own Benchmarks

```bash
# Run all backend benchmarks
cargo bench --no-default-features --bench backend-comparison

# Run with your specific grammar
cargo bench --no-default-features --bench backend-comparison -- <pattern>

# Compare against baseline
cargo bench --no-default-features --bench backend-comparison -- --baseline main
```

## Summary

| Situation | Backend | Reason |
|-----------|---------|--------|
| Simple tokens | Packrat | Lower overhead |
| Expressions | **Bytecode** | 10-30x faster |
| Nested repetition | Packrat | Guaranteed O(n) |
| Memory constrained | Bytecode | Lower memory |
| Unknown grammar | Auto | Best compromise |
| Need consistency | Packrat | Predictable latency |

**Default recommendation:** Use `Backend::default_for_grammar(&grammar)` for automatic selection.
