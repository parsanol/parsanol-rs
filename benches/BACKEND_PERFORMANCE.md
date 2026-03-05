# Backend Performance Comparison

## Benchmark Results (0.2.0)

Comparing **Packrat** vs **Bytecode VM** backends across different grammar types.

### Summary Table

| Grammar Type | Input | Packrat | Bytecode | Winner | Speedup |
|-------------|-------|---------|----------|--------|---------|
| JSON `true` | 4 bytes | 222 ns | 1,127 ns | Packrat | 5.1x |
| JSON `false` | 5 bytes | 225 ns | 6,102 ns | Packrat | 27x |
| JSON `null` | 4 bytes | 229 ns | 5,981 ns | Packrat | 26x |
| JSON `42` | 2 bytes | 3,742 ns | 7,266 ns | Packrat | 1.9x |
| JSON `-3.14` | 5 bytes | 4,750 ns | 8,302 ns | Packrat | 1.7x |
| JSON `"hello world"` | 13 bytes | 6,767 ns | 9,921 ns | Packrat | 1.5x |
| Calculator `1+2` | 3 bytes | 3,882 ns | 865 ns | **Bytecode** | **4.5x** |
| Calculator `1+2*3` | 5 bytes | 10,875 ns | 1,032 ns | **Bytecode** | **10.5x** |
| Calculator `(1+2)*3` | 7 bytes | 15,429 ns | 1,090 ns | **Bytecode** | **14.2x** |
| Calculator complex | 55 bytes | 54,868 ns | 1,727 ns | **Bytecode** | **31.8x** |
| URL simple | 20 bytes | 5,120 ns | 5,268 ns | Tie | - |
| URL path | 44 bytes | 8,662 ns | 7,528 ns | **Bytecode** | **1.2x** |
| Email simple | 16 bytes | 5,279 ns | 5,664 ns | Packrat | 1.1x |
| Email complex | 33 bytes | 9,290 ns | 8,559 ns | **Bytecode** | **1.1x** |
| String literal | 13 bytes | 5,688 ns | 5,559 ns | Tie | - |
| S-expr simple | 6 bytes | 4,478 ns | 3,737 ns | **Bytecode** | **1.2x** |
| S-expr nested | 17 bytes | 9,035 ns | 5,839 ns | **Bytecode** | **1.5x** |
| Balanced `()` | 4 bytes | 1,117 ns | 1,035 ns | Tie | - |
| Balanced nested | 8 bytes | 1,175 ns | 896 ns | **Bytecode** | **1.3x** |
| ISO date | 10 bytes | 4,367 ns | 5,124 ns | Packrat | 1.2x |
| IP address | 11 bytes | 12,858 ns | 13,781 ns | Packrat | 1.1x |

### Throughput (Large Input)

| Test | Input Size | Packrat | Bytecode | Winner |
|------|-----------|---------|----------|--------|
| JSON array (100 items) | 3,809 bytes | 493 MiB/s | 436 MiB/s | Packrat |

### Key Findings

1. **Simple Token Matching**: Packrat is significantly faster (2-27x)
   - Less overhead for trivial patterns
   - Direct string matching without VM dispatch

2. **Complex Expressions**: Bytecode is dramatically faster (10-32x)
   - Calculator with operator precedence
   - Expression trees with multiple operators
   - VM avoids repeated memoization overhead

3. **Nested/Recursive Patterns**: Bytecode is slightly faster (1.2-1.5x)
   - S-expressions, balanced parentheses
   - VM stack-based recursion is efficient

4. **Linear Patterns**: Performance is comparable
   - URLs, emails, dates, IP addresses
   - Both backends handle these well

### Recommendations

| Grammar Type | Recommended Backend | Reason |
|-------------|-------------------|--------|
| Simple tokens | Packrat | Lower overhead |
| Expressions with precedence | **Bytecode** | 10-30x faster |
| Nested structures | Bytecode | Slightly faster |
| Unknown/mixed | Packrat | Safer worst case |

### Automatic Selection

Use `Backend::default_for_grammar()` to automatically select:

```rust
use parsanol::portable::Backend;

let backend = Backend::default_for_grammar(&grammar);
// Returns:
// - Packrat if grammar has nested repetition (safe)
// - Bytecode for simple patterns (fast)
```

### When to Use Each Backend

**Use Packrat when:**
- Grammar has nested repetition (`(a*)*`)
- Worst-case latency matters
- Memory is not constrained
- Simplicity is preferred

**Use Bytecode when:**
- Grammar is mostly linear
- Expression parsing with precedence
- Memory is constrained
- Maximum throughput needed

### Running Benchmarks

```bash
# Run all backend benchmarks
cargo bench --no-default-features --bench backend-comparison

# Run specific benchmark
cargo bench --no-default-features --bench backend-comparison -- calculator

# Compare with baseline
cargo bench --no-default-features --bench backend-comparison -- --baseline main
```
