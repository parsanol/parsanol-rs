# parsanol-ruby-derive

Proc macro crate for deriving `RubyObject` trait in parsanol-rs.

## Purpose

This crate provides the `#[derive(RubyObject)]` macro for automatically implementing the `RubyObject` trait, which enables direct Ruby object construction from Rust types (Native).

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
parsanol-ruby-derive = "0.1"
```

## Example

```rust
use parsanol_ruby_derive::RubyObject;

#[derive(Debug, Clone, RubyObject)]
#[ruby_class("Calculator::Expr")]
pub enum Expr {
    #[ruby_variant("number")]
    Number(i64),

    #[ruby_variant("binop")]
    BinOp {
        left: Box<Expr>,
        op: String,
        right: Box<Expr>,
    },
}
```

## Attributes

### Container Attributes

- `#[ruby_class("MyModule::MyClass")]` - Specify the Ruby class name

### Variant Attributes

- `#[ruby_variant("variant_name")]` - Specify the Ruby variant/class name for this variant

### Field Attributes

- `#[ruby_attr("@field_name")]` - Specify the Ruby instance variable name

## Generated Code

The macro generates an implementation of `RubyObject` that:

1. Looks up the Ruby class by name
2. Creates a new instance
3. Sets instance variables for struct fields
4. Returns the Ruby object

## License

MIT License
