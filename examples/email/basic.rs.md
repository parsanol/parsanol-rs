# Email Address Parser - Rust Implementation

## How to Run

```bash
cargo run --example email/basic --no-default-features
```

## Code Walkthrough

### Local Part Parsing

The local part (before `@`) has specific character rules:

```rust
.rule("local", re(r#"[a-zA-Z0-9._%+-]+"#))
```

Allowed characters include letters, digits, and `._%+-`. The local part cannot start or end with a dot, and cannot have consecutive dots.

### Domain Parsing

The domain follows standard DNS rules:

```rust
.rule("domain", seq(vec![
    dynamic(re(r#"[a-zA-Z0-9-]+"#)),
    dynamic(seq(vec![dynamic(str(".")), dynamic(re(r#"[a-zA-Z0-9-]+"#))]).many()),
]))
```

Domains have at least two labels separated by dots, with the TLD being alphabetic only.

### Separator Detection

The `@` symbol separates local part from domain:

```rust
.rule("separator", str("@"))
```

Some parsers also handle obfuscated formats like "at" or "[at]" for spam-resistant forms.

### Validation Logic

Post-parse validation checks semantic rules:

```rust
fn validate_email(email: &EmailParts) -> Result<(), String> {
    if email.local.is_empty() { return Err("Empty local part"); }
    if email.local.starts_with('.') || email.local.ends_with('.') {
        return Err("Local part cannot start/end with dot");
    }
    if email.domain.split('.').any(|p| p.is_empty()) {
        return Err("Invalid domain");
    }
    Ok(())
}
```

The grammar validates syntax; this validates semantics.

## Output Types

```rust
pub struct EmailParts {
    pub local: String,
    pub domain: String,
}

pub enum Email {
    Valid(EmailParts),
    Invalid(String),  // Error message
}
```

The output distinguishes valid emails from invalid ones with helpful error messages.

## Design Decisions

### Why Not Full RFC 5322 Compliance?

Full RFC 5322 allows many edge cases (quoted strings, comments, IP addresses) that are rarely used in practice. This parser focuses on the 99% use case while remaining extensible.

### Handling Obfuscated Formats

The parser can optionally handle "user at example dot com" by preprocessing:

```rust
fn normalize_email(input: &str) -> String {
    input
        .replace(" at ", "@")
        .replace(" AT ", "@")
        .replace(" dot ", ".")
        .replace(" DOT ", ".")
}
```

This is useful for processing user-submitted data from forms.
