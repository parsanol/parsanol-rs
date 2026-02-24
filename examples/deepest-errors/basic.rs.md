# Deepest Error Reporting - Rust Implementation

## How to Run

```bash
cargo run --example deepest-errors/basic --no-default-features
```

## Code Walkthrough

### The Deepest Failure Concept

PEG parsers try alternatives in order. The deepest failure is where parsing progressed furthest before failing:

```text
Input: "abc123xyz"
Rule tries: letter -> succeeds for "abc"
            digit -> fails at position 3
            letter -> fails at position 3

Deepest: position 6 (after "123") where digit succeeded longest
```

This gives better errors than "expected something at position 0".

### Error Position Extraction

```rust
fn find_deepest_error(input: &str, grammar: &Grammar) -> (usize, String) {
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(grammar, input, &mut arena);

    match parser.parse() {
        Ok(_) => (0, "Success".to_string()),
        Err(e) => {
            let pos = e.to_string().find("at position")
                .map(|p| /* extract position */)
                .unwrap_or(0);
            (pos, e.to_string())
        }
    }
}
```

Error messages include position information that can be extracted.

### Position Formatting

Show the error location with context:

```rust
fn format_error(input: &str, position: usize) -> String {
    let lines: Vec<&str> = input.lines().collect();
    let mut current_pos = 0;

    for (i, line) in lines.iter().enumerate() {
        result.push_str(&format!("{:02} {}\n", i + 1, line));

        let line_end = current_pos + line.len();
        if position >= current_pos && position <= line_end {
            let col = position - current_pos;
            result.push_str(&format!("   {}^\n", " ".repeat(col)));
        }
        current_pos = line_end + 1;
    }
    result
}
```

Convert byte position to line/column for display.

### Grammar for Testing

The example uses a realistic block-structured grammar:

```rust
builder = builder.rule("define_block", seq(vec![
    dynamic(str("define")),
    dynamic(ref_("space")),
    dynamic(ref_("identifier")),
    dynamic(str("()")),
    dynamic(ref_("body_full")),
    dynamic(str("end")),
]));
```

This has enough complexity to demonstrate error scenarios.

### Test Cases

The example tests both valid and invalid inputs:

```rust
let test_cases = [
    ("define f()\n  @res.name()\nend", true),  // valid
    ("define f()\n  @res.name(\nend", false),  // unclosed paren
    ("define f()\n  @res.name()\n", false),    // missing end
];
```

Each demonstrates a different error location.

## Output Types

```
Input:
01 define f()
02   @res.name(
03 end

Error at position 25
   ^

Error: Expected ')' at position 25
```

The position, formatted context, and error message help users fix their input.

## Design Decisions

### Why Deepest Failure?

PEG parsers naturally track the furthest point reached. Using this for error reporting provides the most specific error location.

### Why Byte Position?

Internal tracking uses byte offsets. Conversion to line/column happens only for display.

### Why Context Display?

Showing surrounding lines helps users understand where the error occurred in their input.

### Why Not All Failures?

Reporting every failure point would be overwhelming. The deepest single point is usually the most helpful.
