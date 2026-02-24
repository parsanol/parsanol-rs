# Boolean Algebra Parser - Rust Implementation

## How to Run

```bash
cargo run --example boolean-algebra/basic --no-default-features
```

## Code Walkthrough

### Variable Parsing

Boolean variables follow a simple pattern:

```rust
.rule("variable", re("var[0-9]+"))
```

Variables are named `var` followed by digits (e.g., `var1`, `var42`). This allows for easy testing with predictable names.

### Operator Precedence

The grammar implements proper precedence: NOT > AND > OR

```rust
.rule("primary", choice(vec![
    dynamic(variable),
    dynamic(seq(vec![dynamic(str("(")), dynamic(expr), dynamic(str(")"))])),
    dynamic(seq(vec![dynamic(str("NOT")), dynamic(primary)])),
]))
.rule("and_expr", seq(vec![dynamic(primary), dynamic(and_tail).many()]))
.rule("or_expr", seq(vec![dynamic(and_expr), dynamic(or_tail).many()]))
```

NOT binds tightest (prefix), then AND (infix), then OR (lowest precedence).

### AND Expression Parsing

AND expressions chain primary values:

```rust
.rule("and_tail", seq(vec![
    dynamic(str(" AND ")),
    dynamic(primary),
]))
```

The `and_tail` is repeated zero or more times, creating left-associative chains like `var1 AND var2 AND var3`.

### OR Expression Parsing

OR expressions chain AND expressions:

```rust
.rule("or_tail", seq(vec![
    dynamic(str(" OR ")),
    dynamic(and_expr),
]))
```

OR has lowest precedence, so `var1 OR var2 AND var3` parses as `var1 OR (var2 AND var3)`.

### Evaluation

The evaluation context maps variables to boolean values:

```rust
fn evaluate(expr: &BoolExpr, vars: &HashMap<String, bool>) -> bool {
    match expr {
        BoolExpr::Var(name) => vars.get(name).copied().unwrap_or(false),
        BoolExpr::And(left, right) => evaluate(left, vars) && evaluate(right, vars),
        BoolExpr::Or(left, right) => evaluate(left, vars) || evaluate(right, vars),
        BoolExpr::Not(inner) => !evaluate(inner, vars),
    }
}
```

Variables must looked up in the context HashMap, defaulting to false.

## Output Types

```rust
pub enum BoolExpr {
    Var(String),
    And(Box<BoolExpr>, Box<BoolExpr>),
    Or(Box<BoolExpr>, Box<BoolExpr>),
    Not(Box<BoolExpr>),
}
```

The recursive enum supports all boolean operations with proper nesting.

## Design Decisions

### Why Named Variables?

Using `var1`, `var2`, etc. simplifies testing. Real applications might use identifiers like `enabled`, `debug`, `production`.

### DNF Conversion

The parser can convert expressions to Disjunctive Normal Form (DNF) for optimization:

```rust
fn to_dnf(expr: BoolExpr) -> Vec<Vec<String>> {
    // Converts to sum-of-products form
}
```

DNF enables efficient evaluation and satisfiability checking.
