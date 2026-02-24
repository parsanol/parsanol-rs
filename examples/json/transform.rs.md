# JSON Parser (Transform-Based Approach)

## Purpose

This implementation uses transform-based parsing to produce typed Rust
enums for JSON values.

## When to Use

- Pure Rust applications
- Maximum performance
- Type-safe output

## Key Concepts

1. **Typed Output**: Native `JsonValue` enum
2. **Zero-Copy**: No serialization overhead
3. **Type Safety**: Compile-time guarantees

## Running

```bash
cargo run --example json/transform --no-default-features
```

## Output

```
true => Bool(true)
42 => Number(42.0)
"hello" => String("hello")
[1,2,3] => Array([Number(1.0), Number(2.0), Number(3.0)])
{"key":"value"} => Object({"key": String("value")})
```

## Note

This uses the SAME grammar as `pattern.rs` for fair comparison.
