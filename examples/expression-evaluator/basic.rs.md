# Expression Evaluator - Rust Implementation

## How to Run

```bash
cargo run --example expression-evaluator/basic --no-default-features
```

## Code Walkthrough

### Number Parsing

Numbers can be integers or floating-point:

```rust
.rule("number", re(r#"-?[0-9]+(\.[0-9]+)?"#))
```

The regex handles optional sign prefix and decimal point. Both `42` and `-3.14` are valid.

### Operator Precedence

The grammar implements standard math precedence: `* /` > `+ -`

```rust
.rule("factor", choice(vec![
    dynamic(number),
    dynamic(seq(vec![dynamic(str("(")), dynamic(expr), dynamic(str(")"))])),
]))
.rule("term", seq(vec![dynamic(factor), dynamic(term_tail).many()]))
.rule("expr", seq(vec![dynamic(term), dynamic(expr_tail).many()]))
```

Factors (numbers, parens) are combined into terms (mult/div), then terms into expressions (add/sub).

### Multiplication and Division

Terms chain factors with `*` and `/`:

```rust
.rule("term_tail", seq(vec![
    dynamic(choice(vec![dynamic(str("*")), dynamic(str("/"))])),
    dynamic(factor),
]))
```

Left associativity ensures `a * b / c` parses as `(a * b) / c`.

### Addition and Subtraction

Expressions chain terms with `+` and `-`:

```rust
.rule("expr_tail", seq(vec![
    dynamic(choice(vec![dynamic(str("+")), dynamic(str("-"))])),
    dynamic(term),
]))
```

Lower precedence means `a + b * c` parses as `a + (b * c)`.

### Evaluation

The AST is evaluated recursively:

```rust
fn evaluate(expr: &Expr) -> Result<f64, String> {
    match expr {
        Expr::Number(n) => Ok(n),
        Expr::BinOp { left, op, right } => {
            let l = evaluate(left)?;
            let r = evaluate(right)?;
            match op {
                BinOp::Add => Ok(l + r),
                BinOp::Sub => Ok(l - r),
                BinOp::Mul => Ok(l * r),
                BinOp::Div => Ok(l / r),
            }
        }
    }
}
```

Division by zero returns an error string.

## Output Types

```rust
pub enum Expr {
    Number(f64),
    BinOp {
        left: Box<Expr>,
        op: BinOp,
        right: Box<Expr>,
    },
}

pub enum BinOp { Add, Sub, Mul, Div }
```

The typed AST enables direct evaluation without runtime type checking.

## Design Decisions

### Why Separate Term and Expression?

The separation enables proper precedence handling. Terms handle `*` and `/`, expressions handle `+` and `-`. This is cleaner than explicit precedence values.

### Error Handling

Division by zero is checked at evaluation time, not parse time. The grammar accepts `/ 0` as valid syntax; the evaluator returns an error.
