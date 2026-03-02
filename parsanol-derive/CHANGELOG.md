# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.1](https://github.com/parsanol/parsanol-rs/compare/parsanol-derive-v0.1.0...parsanol-derive-v0.1.1) - 2026-03-02

### Fixed

- *(ci)* simplify release workflow to match lychee setup

## [0.1.0] - 2026-03-02

### Added

- `FromAst` derive macro for converting `Value` to typed Rust structures
- Support for structs with named fields
- Support for tuple structs
- Support for unit structs
- Support for enums with tagged variants
- Support for transparent conversion (single-field structs)
- Support for default values (`#[parsanol(default)]` and `#[parsanol(default = "expr")]`)
- Support for field name mapping (`#[parsanol(field = "name")]`)

### Attributes

#### Container Attributes

- `#[parsanol(rule = "name")]` - The grammar rule this type corresponds to
- `#[parsanol(transparent)]` - Directly convert without wrapper (for single-field structs)

#### Variant Attributes (for enums)

- `#[parsanol(tag = "name")]` - Match when the Value has this tag
- `#[parsanol(tag_expr = "pattern")]` - Match using a pattern

#### Field Attributes

- `#[parsanol(field = "name")]` - Extract from hash field with this name
- `#[parsanol(default)]` - Use Default if field is missing
- `#[parsanol(default = "expr")]` - Use expression if field is missing

## Usage Example

```rust
use parsanol::derive::FromAst;
use parsanol::portable::transform::Value;
use std::convert::TryInto;

#[derive(FromAst)]
struct Point {
    x: i64,
    y: i64,
}

#[derive(FromAst)]
enum Expr {
    #[parsanol(tag = "number")]
    Number(i64),

    #[parsanol(tag = "binop")]
    BinOp {
        left: Box<Expr>,
        op: String,
        right: Box<Expr>,
    },
}

// Convert from Value
let value = Value::hash(vec![
    ("x", Value::int(10)),
    ("y", Value::int(20)),
]);
let point: Point = value.try_into().unwrap();
```
