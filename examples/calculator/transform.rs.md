# Calculator (Transform-Based Approach) - Rust Implementation

## How to Run

```bash
cargo run --example calculator/transform --no-default-features
```

## Code Walkthrough

### Native Expression Type

The transform mode directly produces typed Rust enums:

```rust
#[derive(Debug, Clone)]
pub enum Expr {
    Number(i64),
    BinOp {
        left: Box<Expr>,
        op: BinOp,
        right: Box<Expr>,
    },
}

pub enum BinOp { Add, Sub, Mul, Div }
```

No serialization step - the parser outputs native Rust types directly.

### Grammar with Transform

The grammar includes transformation rules:

```rust
.rule("number", re(r#"[0-9]+"#).transform(|s| s.parse::<i64>()))
.rule("add_op", choice(vec![
    dynamic(str("+").transform(|_| BinOp::Add)),
    dynamic(str("-").transform(|_| BinOp::Sub)),
]))
```

Transforms are applied during parsing, producing typed output in a single pass.

### Evaluation

The expression can be evaluated directly:

```rust
impl Expr {
    pub fn eval(&self) -> i64 {
        match self {
            Expr::Number(n) => *n,
            Expr::BinOp { left, op, right } => {
                let l = left.eval();
                let r = right.eval();
                match op {
                    BinOp::Add => l + r,
                    BinOp::Sub => l - r,
                    BinOp::Mul => l * r,
                    BinOp::Div => l / r,
                }
            }
        }
    }
}
```

The native type enables methods like `eval()` with zero runtime overhead.

### Type-Safe Operators

Using an enum for operators provides compile-time safety:

```rust
pub enum BinOp { Add, Sub, Mul, Div }
```

No string comparison needed at runtime - pattern matching is exhaustive and optimized.

## Output Types

```rust
#[derive(Debug, Clone)]
pub enum Expr {
    Number(i64),
    BinOp { left: Box<Expr>, op: BinOp, right: Box<Expr> },
}
```

Native Rust types with full pattern matching support and zero serialization overhead.

## Design Decisions

### Why Native Types?

For pure Rust applications:
- **Zero overhead**: No serialization/deserialization
- **Type safety**: Compile-time guarantees
- **Pattern matching**: Exhaustive matching on `Expr` variants
- **Methods**: Can implement `eval()`, `optimize()`, etc.

### Comparison with Pattern Mode

| Aspect | Pattern Mode | Transform Mode |
|--------|-------------|----------------|
| Output | JSON string | Native Rust |
| Overhead | Serialization | None |
| Use case | FFI/Cross-language | Pure Rust |
| Evaluation | Requires parsing JSON | Direct method call |

Use transform mode for Rust-only applications where performance and type safety matter.
