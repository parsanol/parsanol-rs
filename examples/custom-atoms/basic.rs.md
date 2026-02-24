# Custom Atoms Example - Rust Implementation

## How to Run

```bash
cd parsanol-rs
cargo run --example custom-atoms/basic --no-default-features
```

## Code Walkthrough

This example demonstrates patterns for extending Parsanol with custom atom behavior.

### 1. Post-Parse Semantic Validation

Instead of creating custom atoms, validate after parsing:

```rust
pub struct SemanticValidator {
    reserved_words: Vec<String>,
    port_range: (u16, u16),
}

impl SemanticValidator {
    pub fn validate_identifier(&self, name: &str) -> Result<(), String> {
        if self.reserved_words.contains(&name.to_string()) {
            return Err(format!("'{}' is a reserved word", name));
        }
        Ok(())
    }
}
```

**Why this pattern:**
- Keeps grammar simple
- Separates syntax from semantics
- Easier to test validation logic independently

### 2. Grammar Builder Extensions

Create convenience methods for common patterns:

```rust
pub trait GrammarBuilderExt {
    fn quoted_string(&mut self, name: &str) -> usize;
    fn bounded_integer(&mut self, name: &str, min: i64, max: i64) -> usize;
    fn safe_identifier(&mut self, name: &str) -> usize;
}
```

### 3. Validation Patterns

#### Identifier Validation
- Check against reserved words
- Enforce naming conventions
- Validate length constraints

#### Numeric Range Validation
- Check bounds
- Validate special ranges (e.g., privileged ports)

#### Email Validation
- Format validation (beyond regex)
- Semantic checks (domain exists, etc.)

## Output Types

The example produces validated configuration pairs:

```rust
Vec<(String, String, bool)>  // (key, value, is_valid)
```

And a `SemanticValidator` that can be reused:

```rust
let validator = SemanticValidator::new();
validator.validate_identifier("my_var")?;  // Ok(())
validator.validate_port(8080)?;            // Ok(())
validator.validate_email("a@b.com")?;      // Ok(())
```

## Design Decisions

### Why Post-Parse Validation?

1. **Simplicity**: Grammar stays focused on syntax
2. **Testability**: Validation logic is independent
3. **Flexibility**: Easy to add new rules without modifying grammar
4. **Performance**: Parse once, validate many times with different rules

### When to Use Custom Atoms

Consider custom atoms when:
- You need context-dependent matching (indentation-sensitive languages)
- You need to match against external data (dictionaries, databases)
- You need specialized performance optimizations

For most cases, post-parse validation is sufficient and simpler.

## Extending the Example

To add new validation rules:

```rust
impl SemanticValidator {
    /// Validate a URL format
    pub fn validate_url(&self, url: &str) -> Result<(), String> {
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err("URL must start with http:// or https://".to_string());
        }
        Ok(())
    }

    /// Validate a hex color code
    pub fn validate_hex_color(&self, color: &str) -> Result<(), String> {
        if !color.starts_with('#') || color.len() != 7 {
            return Err("Hex color must be #RRGGBB format".to_string());
        }
        if !color[1..].chars().all(|c| c.is_ascii_hexdigit()) {
            return Err("Hex color must contain only hex digits".to_string());
        }
        Ok(())
    }
}
```
