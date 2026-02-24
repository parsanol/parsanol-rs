# Balanced Parentheses Parser - Rust Implementation

## How to Run

```bash
cargo run --example balanced-parens/basic --no-default-features
```

## Code Walkthrough

### Recursive Grammar Definition

The grammar is self-referential - parentheses contain balanced content:

```rust
.rule("balanced", choice(vec![
    dynamic(str("")),  // Empty - balanced
    dynamic(seq(vec![  // content
        dynamic(str("(")),
        dynamic(balanced),  // Recursive reference
        dynamic(str(")")),
        dynamic(balanced),
    ])),
]))
```

A balanced string is either empty, or `(` + balanced + `)` + balanced.

### Multiple Delimiter Types

Support for `()`, `[]`, and `{}`:

```rust
.rule("paren_group", seq(vec![dynamic(str("(")), dynamic(content), dynamic(str(")"))]))
.rule("bracket_group", seq(vec![dynamic(str("[")), dynamic(content), dynamic(str("]"))]))
.rule("brace_group", seq(vec![dynamic(str("{")), dynamic(content), dynamic(str("}"))]))
```

Each delimiter type has its own rule, but they share the same content definition.

### Validation Algorithm

Post-parse validation checks matching delimiters:

```rust
fn validate_matching(input: &str) -> Result<(), String> {
    let mut stack: Vec<(char, usize)> = Vec::new();
    for (i, c) in input.chars().enumerate() {
        match c {
            '(' | '[' | '{' => stack.push((c, i)),
            ')' | ']' | '}' => {
                let expected = match c {
                    ')' => '(',
                    ']' => '[',
                    '}' => '{',
                    _ => return Err(format!("Unexpected closing '{}' at {}", c, i)),
                };
                match stack.pop() {
                    Some((open, pos)) if open == expected => continue,
                    Some((open, pos)) => return Err(format!("Mismatch: '{}' at {} vs '{}' at {}", open, pos, c, i)),
                    None => return Err(format!("Unmatched '{}' at {}", c, i)),
                }
            }
            _ => {}
        }
    }
    if !stack.is_empty() {
        return Err(format!("Unclosed '{}' at {}", stack[0].0, stack[0].1));
    }
    Ok(())
}
```

A stack-based approach ensures each opener has a matching closer of the same type.

### Depth Tracking

Track nesting depth for analysis:

```rust
fn calculate_depth(input: &str) -> usize {
    let mut max_depth = 0;
    let mut current_depth = 0;
    for c in input.chars() {
        match c {
            '(' | '[' | '{' => {
                current_depth += 1;
                max_depth = max_depth.max(current_depth);
            }
            ')' | ']' | '}' => current_depth -= 1,
            _ => {}
        }
    }
    max_depth
}
```

Maximum depth helps identify deeply nested structures.

## Output Types

```rust
pub enum ParenNode {
    Empty,
    Balanced { content: Box<ParenNode>, rest: Box<ParenNode> },
    Text(String),
}

pub struct BalancedResult {
    pub is_valid: bool,
    pub depth: usize,
    pub error: Option<String>,
}
```

The tree structure preserves the parsing, while `BalancedResult` provides validation summary.

## Design Decisions

### Why Recursive Grammar?

Recursive grammars naturally express nested structures. The grammar mirrors the recursive definition: balanced = empty | `(` balanced `)` balanced.

### Why Post-Parse Validation?

The grammar validates structure, but ensuring matching delimiters requires context (the stack). This is cleaner in post-processing than encoding in the grammar.
