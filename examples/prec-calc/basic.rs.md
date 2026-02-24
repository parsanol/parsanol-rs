# Precedence Calculator - Rust Implementation

## How to Run

```bash
cargo run --example prec-calc/basic --no-default-features
```

## Code Walkthrough

### Grammar Construction

The grammar is built using the GrammarBuilder DSL:

```rust
let mut builder = GrammarBuilder::new();

builder = builder.rule("digit", re(r"[0-9]"));
builder = builder.rule("integer", re(r"[0-9]+"));
builder = builder.rule("identifier", re(r"[a-z][a-zA-Z0-9]*"));
```

Basic rules define atoms: digits, integers, and identifiers.

### InfixBuilder for Precedence

The InfixBuilder handles operator precedence automatically:

```rust
let expr_atom = InfixBuilder::new()
    .primary(ref_("primary"))
    .op("*", 2, Assoc::Left)
    .op("/", 2, Assoc::Left)
    .op("+", 1, Assoc::Right)
    .op("-", 1, Assoc::Right)
    .build(&mut builder);
```

Higher precedence numbers bind tighter. Multiplication (2) binds before addition (1).

### Association Types

Left and right associativity are specified:

```rust
Assoc::Left   // 10 - 3 - 2 = (10 - 3) - 2 = 5
Assoc::Right  // 2 ^ 3 ^ 2 = 2 ^ (3 ^ 2) = 512
```

Left-associative operators group left-to-right; right-associative group right-to-left.

### Expression AST

The AST represents binary operations recursively:

```rust
pub enum Expr {
    Integer(i64),
    BinaryOp {
        left: Box<Expr>,
        op: String,
        right: Box<Expr>,
    },
}
```

Box enables recursive structure; operation strings allow multiple operators.

### Variable Bindings

A simple HashMap stores variable values:

```rust
pub struct Bindings {
    vars: HashMap<String, i64>,
}
```

Assignments add to the map; lookups read from it.

### Evaluation

Recursive evaluation computes expression values:

```rust
fn eval(expr: &Expr) -> i64 {
    match expr {
        Expr::Integer(n) => *n,
        Expr::BinaryOp { left, op, right } => {
            let l = eval(left);
            let r = eval(right);
            match op.as_str() {
                "+" => l + r,
                "-" => l - r,
                "*" => l * r,
                "/" => l / r,
                _ => panic!("Unknown operator"),
            }
        }
    }
}
```

Post-order traversal evaluates children before applying operator.

## Output Types

```rust
// Expression AST
Expr::Integer(42)
Expr::BinaryOp {
    left: Box::new(Expr::Integer(10)),
    op: "*".to_string(),
    right: Box::new(Expr::Integer(5)),
}

// Bindings
Bindings {
    vars: {"a".to_string() => 7, "b".to_string() => 10}
}
```

## Design Decisions

### Why InfixBuilder Instead of Manual Rules?

InfixBuilder handles the complex pattern matching for precedence automatically. Manual recursive descent would require careful ordering of alternatives.

### Why Box for Recursive Types?

Rust requires known sizes for all types. Box provides heap allocation with a fixed-size pointer, enabling recursive enum variants.

### Why String for Operators?

Using String allows dynamic operator sets. In production, an enum would provide type safety but requires more code.
