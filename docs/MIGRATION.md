# Parsanol Migration Guide

This guide helps you migrate from other parser libraries to Parsanol.

## Table of Contents

1. [From Parslet (Ruby)](#from-parslet-ruby)
2. [From Nom (Rust)](#from-nom-rust)
3. [From LALRPOP (Rust)](#from-lalrpop-rust)
4. [From Pest (Rust)](#from-pest-rust)
5. [From Regex](#from-regex)
6. [Common Patterns](#common-patterns)
7. [Troubleshooting](#troubleshooting)

---

## From Parslet (Ruby)

### API Mapping

| Parslet | Parsanol-rs | Notes |
|---------|-------------|-------|
| `str('foo')` | `Atom::Str { pattern: "foo" }` | Direct string matching |
| `match('[0-9]')` | `Atom::Re { pattern: "[0-9]" }` | Regex character class |
| `any` | `Atom::Re { pattern: "." }` | Any single character |
| `>>` (sequence) | `Atom::Sequence { atoms: [...] }` | Sequential matching |
| `\|` (alternative) | `Atom::Alternative { atoms: [...] }` | Ordered choice |
| `.repeat(n, m)` | `Atom::Repetition { atom, min: n, max: m }` | Bounded repetition |
| `.maybe` | `Atom::Repetition { atom, min: 0, max: 1 }` | Optional |
| `.as(:name)` | `Atom::Named { name, atom }` | Capture with label |
| `.absent?` | `Atom::Lookahead { atom, positive: false }` | Negative lookahead |
| `.present?` | `Atom::Lookahead { atom, positive: true }` | Positive lookahead |

### Basic Example

**Parslet (Ruby):**
```ruby
require 'parslet'

class MyParser < Parslet::Parser
  rule(:number) { match('[0-9]').repeat(1) }
  rule(:operator) { str('+') | str('-') }
  rule(:expression) { number.as(:left) >> operator.as(:op) >> number.as(:right) }
  root(:expression)
end

parser = MyParser.new
result = parser.parse("1+2")
```

**Parsanol-rs (Rust):**
```rust
use parsanol::portable::{Grammar, Atom, AstArena, PortableParser};

fn build_grammar() -> Grammar {
    let mut g = Grammar::new();

    // number = [0-9]+
    let number = g.add_atom(Atom::Re {
        pattern: "[0-9]+".to_string()
    });

    // operator = '+' | '-'
    let plus = g.add_atom(Atom::Str { pattern: "+".to_string() });
    let minus = g.add_atom(Atom::Str { pattern: "-".to_string() });
    let operator = g.add_atom(Atom::Alternative {
        atoms: vec![plus, minus]
    });

    // expression = number operator number
    let named_left = g.add_atom(Atom::Named {
        name: "left".to_string(),
        atom: number
    });
    let named_op = g.add_atom(Atom::Named {
        name: "op".to_string(),
        atom: operator
    });
    let named_right = g.add_atom(Atom::Named {
        name: "right".to_string(),
        atom: number
    });

    let expr = g.add_atom(Atom::Sequence {
        atoms: vec![named_left, named_op, named_right]
    });

    g.root = expr;
    g
}

fn main() {
    let grammar = build_grammar();
    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, "1+2", &mut arena);
    let result = parser.parse();
}
```

**Using DSL (Recommended):**
```rust
use parsanol::portable::parser_dsl::{GrammarBuilder, str, re, any};

let grammar = GrammarBuilder::new()
    .rule("number", re("[0-9]+"))
    .rule("operator", str("+").or(str("-")))
    .rule("expression",
        re("[0-9]+").as_named("left")
        .then(str("+").or(str("-")).as_named("op"))
        .then(re("[0-9]+").as_named("right"))
    )
    .root("expression")
    .build();

let result = grammar.parse("1+2")?;
```

### Transform Comparison

**Parslet (Ruby):**
```ruby
class MyTransform < Parslet::Transform
  rule(number: simple(:n)) { Integer(n) }
  rule(left: simple(:l), op: simple('+'), right: simple(:r)) { l + r }
  rule(left: simple(:l), op: simple('-'), right: simple(:r)) { l - r }
end

tree = parser.parse("1+2")
result = MyTransform.new.apply(tree)
```

**Parsanol-rs (Rust):**
```rust
use parsanol::portable::transform::{DirectTransform, Value};

let mut transform = DirectTransform::new();

// Transform number strings to integers
transform.add_rule("number", |node, arena| {
    let s = arena.get_input_ref(node)?;
    Ok(Value::Integer(s.parse()?))
});

// Transform expression
transform.add_rule("expression", |node, arena| {
    let hash = arena.get_hash(node)?;
    let left: i64 = extract_int(&hash["left"], arena)?;
    let right: i64 = extract_int(&hash["right"], arena)?;
    let op = extract_string(&hash["op"], arena)?;

    let result = match op {
        "+" => left + right,
        "-" => left - right,
        _ => return Err(TransformError::UnknownOperator(op)),
    };

    Ok(Value::Integer(result))
});

let tree = parser.parse()?;
let result = transform.apply(&tree, &arena)?;
```

### Key Differences

1. **No Dynamic Parser Definition**: Parsanol grammars are built programmatically, not via subclassing
2. **Explicit Arena**: Memory management is explicit via `AstArena`
3. **Typed Results**: Rust's type system provides compile-time guarantees
4. **No Symbol Syntax**: Use `.as_named("name")` instead of `.as(:name)`

---

## From Nom (Rust)

### Philosophy Difference

- **Nom**: Parser combinators, each parser is a function
- **Parsanol**: Grammar-based, define grammar structure first

### API Mapping

| Nom | Parsanol | Notes |
|-----|----------|-------|
| `tag("foo")` | `Atom::Str { pattern: "foo" }` | Literal match |
| `regex("[0-9]+")` | `Atom::Re { pattern: "[0-9]+" }` | Regex match |
| `a >> b` | `Atom::Sequence { atoms: [a, b] }` | Sequence |
| `a.or(b)` | `Atom::Alternative { atoms: [a, b] }` | Choice |
| `a.many()` | `Atom::Repetition { atom: a, min: 0, max: None }` | Zero or more |
| `a.many1()` | `Atom::Repetition { atom: a, min: 1, max: None }` | One or more |
| `a.opt()` | `Atom::Repetition { atom: a, min: 0, max: 1 }` | Optional |
| `peek(a)` | `Atom::Lookahead { atom: a, positive: true }` | Lookahead |

### Example

**Nom:**
```rust
use nom::{
    IResult,
    bytes::complete::tag,
    sequence::tuple,
    combinator::map,
};

fn parse_pair(input: &str) -> IResult<&str, (&str, &str)> {
    let (input, (key, _, value)) = tuple((
        tag("key"),
        tag("="),
        tag("value")
    ))(input)?;
    Ok((input, (key, value)))
}
```

**Parsanol:**
```rust
use parsanol::portable::parser_dsl::{GrammarBuilder, str};

let grammar = GrammarBuilder::new()
    .rule("pair",
        str("key").as_named("key")
        .then(str("="))
        .then(str("value").as_named("value"))
    )
    .root("pair")
    .build();

let result = grammar.parse("key=value")?;
```

### Key Differences

1. **Error Handling**: Parsanol uses `Result<_, ParseError>`, not `IResult`
2. **Backtracking**: Parsanol always backtracks; no `cut` operator like Nom's `preceded`
3. **Output Type**: Parsanol produces generic AST, not typed output
4. **Memoization**: Parsanol caches all results (packrat), Nom doesn't by default

---

## From LALRPOP (Rust)

### Philosophy Difference

- **LALRPOP**: LR(1) parser generator, handles left recursion
- **Parsanol**: PEG parser, cannot handle left recursion directly

### Grammar Syntax

**LALRPOP:**
```
Expr: i32 = {
    <l:Expr> "+" <r:Term> => l + r,
    <l:Expr> "-" <r:Term> => l - r,
    <n:Term> => n,
}

Term: i32 = {
    <l:Term> "*" <r:Factor> => l * r,
    <l:Term> "/" <r:Factor> => l / r,
    <n:Factor> => n,
}

Factor: i32 = {
    <n:Number> => n,
    "(" <e:Expr> ")" => e,
}
```

**Parsanol (with infix support):**
```rust
use parsanol::portable::parser_dsl::{GrammarBuilder, str, re, infix};

let grammar = GrammarBuilder::new()
    .rule("number", re("[0-9]+"))
    .rule("expr", infix(
        "number",
        &[
            (str("*"), 2, Assoc::Left),
            (str("/"), 2, Assoc::Left),
            (str("+"), 1, Assoc::Left),
            (str("-"), 1, Assoc::Left),
        ]
    ))
    .root("expr")
    .build();
```

### Left Recursion

**LALRPOP handles this naturally:**
```
Expr = Expr "+" Term | Term
```

**Parsanol requires rewrite:**
```rust
// Use infix helper or rewrite as:
// Expr = Term ("+" Term)*
let grammar = GrammarBuilder::new()
    .rule("term", /* ... */)
    .rule("expr",
        rule("term").as_named("first")
        .then(
            rule("term")
            .then(str("+"))
            .repeat(0, None)
            .as_named("rest")
        )
    )
    .root("expr")
    .build();
```

---

## From Pest (Rust)

### Philosophy Difference

- **Pest**: PEG parser generator with custom syntax
- **Parsanol**: Programmatic grammar construction

### Grammar Syntax

**Pest:**
```
expr = { term ~ ("+" ~ term)* }
term = { factor ~ ("*" ~ factor)* }
factor = { number | "(" ~ expr ~ ")" }
number = { ASCII_DIGIT+ }
```

**Parsanol:**
```rust
let grammar = GrammarBuilder::new()
    .rule("number", re("[0-9]+"))
    .rule("factor",
        re("[0-9]+")
        .or(
            str("(").then(rule("expr")).then(str(")"))
        )
    )
    .rule("term",
        rule("factor").as_named("first")
        .then(
            str("*").then(rule("factor")).repeat(0, None).as_named("rest")
        )
    )
    .rule("expr",
        rule("term").as_named("first")
        .then(
            str("+").then(rule("term")).repeat(0, None).as_named("rest")
        )
    )
    .root("expr")
    .build();
```

### Key Differences

1. **No External Files**: Parsanol grammars are Rust code, not .pest files
2. **No Macro Magic**: Pest uses `#[grammar = "..."]`, Parsanol uses builder pattern
3. **Similar Semantics**: Both are PEG parsers with ordered choice

---

## From Regex

### When to Switch

Use Parsanol when:
- Grammar has nested structures (balanced parens, nested expressions)
- You need structured output (AST), not just captures
- Grammar is complex (multiple alternatives, optional parts)
- You need better error messages

Use Regex when:
- Pattern is simple and flat
- You just need to extract substrings
- Performance is critical and pattern is known to compile efficiently

### Example

**Regex:**
```rust
let re = Regex::new(r"(\d+)([+-])(\d+)").unwrap();
if let Some(caps) = re.captures("1+2") {
    let left: i64 = caps[1].parse().unwrap();
    let op = &caps[2];
    let right: i64 = caps[3].parse().unwrap();
}
```

**Parsanol:**
```rust
let grammar = GrammarBuilder::new()
    .rule("expr",
        re("[0-9]+").as_named("left")
        .then(re("[+-]").as_named("op"))
        .then(re("[0-9]+").as_named("right"))
    )
    .root("expr")
    .build();

let result = grammar.parse("1+2")?;
// Result is structured AST with named captures
```

---

## Common Patterns

### Recursive Structures

For recursive grammars, use forward references:

```rust
let grammar = GrammarBuilder::new()
    .rule_forward("expr")  // Declare before definition
    .rule("number", re("[0-9]+"))
    .rule("factor",
        rule("number")
        .or(str("(").then(rule("expr")).then(str(")")))
    )
    .rule_define("expr",  // Define after dependencies
        rule("factor")
        .then(str("+").then(rule("factor")).repeat(0, None))
    )
    .root("expr")
    .build();
```

### Error Recovery

```rust
let grammar = GrammarBuilder::new()
    .rule("statement",
        rule("assignment")
        .or(rule("expression"))
        .or(rule("error_recovery"))  // Catch-all for error recovery
    )
    .rule("error_recovery",
        any().repeat(0, None).until(str(";"))  // Skip to next semicolon
    )
    .build();
```

### Streaming Large Files

```rust
use parsanol::portable::streaming::{StreamingParser, ChunkConfig};

let parser = StreamingParser::new(&grammar);
let config = ChunkConfig::new(64 * 1024);  // 64 KB chunks

parser.parse_stream(reader, config, |result| {
    match result {
        Ok(node) => process_node(node),
        Err(e) => handle_error(e),
    }
});
```

### Incremental Parsing

```rust
use parsanol::portable::incremental::{IncrementalParser, Edit};

let mut parser = IncrementalParser::new(&grammar);
let result = parser.parse(content)?;

// Apply edit
let edit = Edit {
    position: 10,
    deleted_length: 5,
    inserted_text: "hello",
};
parser.apply_edit(edit);

// Reparse (only affected regions)
let new_result = parser.reparse()?;
```

---

## Troubleshooting

### Common Errors

#### "Left recursion detected"

**Problem:** Grammar has direct or indirect left recursion.

**Solution:** Rewrite using repetition:
```rust
// Instead of: expr = expr "+" term | term
// Use: expr = term ("+" term)*
```

#### "Infinite loop in repetition"

**Problem:** Repetition can match empty string.

**Solution:** Ensure repetition body cannot be empty:
```rust
// Bad: .repeat(0, None) where inner can match empty
// Good: .repeat(1, None) or ensure inner always consumes
```

#### "Recursion limit exceeded"

**Problem:** Input causes deep recursion.

**Solution:** Increase limit or check for pathological input:
```rust
let config = ParserConfig::builder()
    .max_recursion_depth(10000)
    .build();
parser.parse_with_config(input, config)?;
```

#### "Memory limit exceeded"

**Problem:** Input too large for available memory.

**Solution:** Use streaming parser or increase limit:
```rust
let config = ParserConfig::builder()
    .max_memory(1_000_000_000)  // 1 GB
    .build();
```

### Performance Tips

1. **Reuse Arena**: Create once, reset between parses
2. **Reuse Grammar**: Build once, parse many inputs
3. **Use Batch Parsing**: `grammar.parse_batch(inputs)` for multiple inputs
4. **Enable Optimizations**: Build with `--release` for production
5. **Profile Cache**: Check `parser.cache_stats()` for hit rate

### Getting Help

- **Documentation**: `/docs/ARCHITECTURE.md`
- **Examples**: `/examples/` directory
- **Issues**: GitHub issue tracker
