# parsanol-derive

Procedural derive macros for [parsanol](https://crates.io/crates/parsanol),
providing automatic typed AST generation from parser output.

[![Crates.io](https://img.shields.io/crates/v/parsanol-derive.svg)](https://crates.io/crates/parsanol-derive)
[![Documentation](https://docs.rs/parsanol-derive/badge.svg)](https://docs.rs/parsanol-derive)
[![License](https://img.shields.io/github/license/parsanol/parsanol-rs.svg)](https://github.com/parsanol/parsanol-rs/blob/main/LICENSE)

## Overview

This crate provides the `FromAst` derive macro that automatically generates
code to convert `parsanol::portable::transform::Value` types into typed Rust
structs and enums. This eliminates boilerplate code for AST transformation.

**Note:** This crate is automatically included as a dependency of `parsanol`.
You typically don't need to depend on it directly.

## Usage

Add `parsanol` to your `Cargo.toml`:

```toml
[dependencies]
parsanol = "0.1"
```

Then use the derive macro:

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
| `#[parsanol(rule = "name")]` | Specify the grammar rule name (for documentation) |

## Variant Attributes (for enums)

| Attribute | Description |
|-----------|-------------|
| `#[parsanol(tag = "literal")]` | Match by literal tag string |
| `#[parsanol(tag_expr = expr)]` | Match by expression (for dynamic tags) |

All enum variants must have either `tag` or `tag_expr`.

## Field Attributes

| Attribute | Description |
|-----------|-------------|
| `#[parsanol(field = "name")]` | Map to different hash field name |
| `#[parsanol(default)]` | Use `Default::default()` if missing |
| `#[parsanol(default = expr)]` | Use expression if missing |

## Examples

### Struct with Named Fields

```rust
#[derive(FromAst)]
pub struct Assignment {
    #[parsanol(field = "name")]
    variable: String,
    value: Box<Expr>,
}
```

### Enum with Multiple Variants

```rust
#[derive(FromAst)]
pub enum Statement {
    #[parsanol(tag = "assignment")]
    Assignment {
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
        #[parsanol(default = Vec::new())]
        else_block: Vec<Statement>,
    },
}
```

### Single-Field Tuple Structs (Transparent)

Single-field tuple structs automatically get transparent conversion:

```rust
#[derive(FromAst)]
pub struct Identifier(pub String);

// Value::String("foo") directly converts to Identifier("foo")
```

### Multi-Field Tuple Structs

Multi-field tuple structs are converted from arrays:

```rust
#[derive(FromAst)]
pub struct Point(pub f64, pub f64);

// Value::Array([Value::Float(1.0), Value::Float(2.0)])
// converts to Point(1.0, 2.0)
```

## Error Handling

The generated `TryFrom` implementation returns `FromAstError`:

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
    Err(FromAstError::ConversionError) => {
        eprintln!("Failed to convert value");
    }
    Err(e) => eprintln!("Error: {}", e),
}
```

## How It Works

The derive macro generates a `TryFrom<Value>` implementation:

```rust
// For a struct like:
#[derive(FromAst)]
struct MyStruct {
    field1: i64,
    field2: String,
}

// The macro generates approximately:
impl TryFrom<Value> for MyStruct {
    type Error = FromAstError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let field1 = value.get_hash_field("field1")
            .ok_or(FromAstError::MissingField("field1".into()))?
            .clone()
            .try_into()
            .map_err(|_| FromAstError::ConversionError)?;
        let field2 = value.get_hash_field("field2")
            .ok_or(FromAstError::MissingField("field2".into()))?
            .clone()
            .try_into()
            .map_err(|_| FromAstError::ConversionError)?;
        Ok(MyStruct { field1, field2 })
    }
}
```

## License

MIT License - see [LICENSE](https://github.com/parsanol/parsanol-rs/blob/main/LICENSE) for details.
