//! Custom Atoms Example
//!
//! Demonstrates patterns for extending Parsanol with custom validation logic.
//! Shows how to create domain-specific parsers with semantic checks.
//!
//! Run with: cargo run --example custom-atoms --no-default-features

#![allow(clippy::print_literal)]

use parsanol::portable::{
    parser_dsl::{dynamic, re, seq, GrammarBuilder},
    AstArena, Grammar, PortableParser,
};

// ============================================================================
// Pattern 1: Semantic Validation (Post-Parse)
// ============================================================================

/// Reserved words that cannot be used as identifiers
const RESERVED_WORDS: &[&str] = &[
    "if", "else", "while", "for", "return", "fn", "let", "const", "true", "false", "null", "break",
    "continue", "import", "export",
];

/// Validate that an identifier is not a reserved word
pub fn validate_identifier(name: &str) -> Result<(), String> {
    if RESERVED_WORDS.contains(&name) {
        return Err(format!("'{}' is a reserved word", name));
    }
    if name.starts_with('_') && name.len() == 1 {
        return Err("Underscore alone is not a valid identifier".to_string());
    }
    if name.len() < 2 {
        return Err(format!("Identifier '{}' is too short", name));
    }
    Ok(())
}

/// Validate a port number
pub fn validate_port(port: u16) -> Result<(), String> {
    if port == 0 {
        return Err("Port 0 is not valid".to_string());
    }
    if port < 1024 {
        return Err(format!("Port {} is a privileged port (< 1024)", port));
    }
    Ok(())
}

/// Validate an email format (beyond regex)
pub fn validate_email(email: &str) -> Result<(), String> {
    let parts: Vec<&str> = email.split('@').collect();
    if parts.len() != 2 {
        return Err("Email must contain exactly one @".to_string());
    }

    let local = parts[0];
    let domain = parts[1];

    if local.is_empty() {
        return Err("Email local part cannot be empty".to_string());
    }
    if local.starts_with('.') || local.ends_with('.') {
        return Err("Email local part cannot start or end with .".to_string());
    }
    if !domain.contains('.') {
        return Err("Email domain must contain a .".to_string());
    }

    Ok(())
}

// ============================================================================
// Pattern 2: Configuration Parser with Validation
// ============================================================================

/// Configuration parser that demonstrates custom validation
pub struct ConfigParser {
    grammar: Grammar,
}

impl ConfigParser {
    pub fn new() -> Self {
        let grammar = GrammarBuilder::new()
            // Basic patterns
            .rule("identifier", re("[a-zA-Z_][a-zA-Z0-9_]*"))
            .rule("integer", re("[0-9]+"))
            .rule("email_local", re("[a-zA-Z0-9._%+-]+"))
            .rule("email_domain", re("[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}"))
            .rule(
                "email",
                seq(vec![
                    dynamic(re("[a-zA-Z0-9._%+-]+")),
                    dynamic(re("@")),
                    dynamic(re("[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}")),
                ]),
            )
            // Values
            .rule("value", re("[^\\n]+"))
            // Key-value pairs
            .rule(
                "pair",
                seq(vec![
                    dynamic(re("[a-zA-Z_][a-zA-Z0-9_]*")), // key
                    dynamic(re("[ \\t]*=[ \\t]*")),        // =
                    dynamic(re("[^\\n]+")),                // value
                ]),
            )
            // Full config (last rule is root)
            .rule("config", re("([^\\n]*\\n)*[^\\n]*"))
            .build();

        Self { grammar }
    }

    /// Parse and validate configuration
    pub fn parse(&self, input: &str) -> Result<Vec<(String, String, bool)>, String> {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&self.grammar, input, &mut arena);

        let _ast = parser
            .parse()
            .map_err(|e| format!("Parse error: {:?}", e))?;

        // Extract and validate pairs
        let mut pairs = Vec::new();

        for line in input.lines() {
            let trimmed = line.trim();

            // Skip empty lines and comments
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            // Parse key-value pair
            if let Some((key, value)) = trimmed.split_once('=') {
                let key = key.trim();
                let value = value.trim();

                // Validate key
                let key_valid = validate_identifier(key).is_ok();

                // Validate value based on key name
                let value_valid = if key.contains("port") {
                    value
                        .parse::<u16>()
                        .ok()
                        .is_some_and(|p| validate_port(p).is_ok())
                } else if key.contains("email") || key.contains("mail") {
                    validate_email(value).is_ok()
                } else {
                    true
                };

                pairs.push((key.to_string(), value.to_string(), key_valid && value_valid));
            }
        }

        Ok(pairs)
    }
}

impl Default for ConfigParser {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Pattern 3: Type-Safe Value Extraction
// ============================================================================

/// Extract and validate a port number from a string
pub fn extract_port(value: &str) -> Result<u16, String> {
    let port: u16 = value
        .parse()
        .map_err(|_| format!("'{}' is not a valid port", value))?;
    validate_port(port)?;
    Ok(port)
}

/// Extract and validate an email from a string
pub fn extract_email(value: &str) -> Result<String, String> {
    validate_email(value)?;
    Ok(value.to_string())
}

/// Extract and validate an identifier from a string
pub fn extract_identifier(value: &str) -> Result<String, String> {
    validate_identifier(value)?;
    Ok(value.to_string())
}

// ============================================================================
// Main: Demonstrate Patterns
// ============================================================================

fn main() {
    println!("Custom Atoms Example");
    println!("{}\n", "=".repeat(60));

    // Pattern 1: Semantic validation
    println!("Pattern 1: Semantic Validation");
    println!("{}", "-".repeat(60));

    println!("\nIdentifier validation:");
    for name in &["valid_name", "if", "_", "myVar123", "x"] {
        match validate_identifier(name) {
            Ok(()) => println!("  ✓ '{}' is valid", name),
            Err(e) => println!("  ✗ '{}': {}", name, e),
        }
    }

    println!("\nPort validation:");
    for port in &[80, 443, 8080, 0] {
        match validate_port(*port) {
            Ok(()) => println!("  ✓ Port {} is valid", port),
            Err(e) => println!("  ✗ Port {}: {}", port, e),
        }
    }

    println!("\nEmail validation:");
    for email in &[
        "user@example.com",
        "invalid",
        "@example.com",
        "user@",
        "user.name@example.org",
    ] {
        match validate_email(email) {
            Ok(()) => println!("  ✓ '{}' is valid", email),
            Err(e) => println!("  ✗ '{}': {}", email, e),
        }
    }

    // Pattern 2: Configuration parser
    println!("\n{}", "-".repeat(60));
    println!("Pattern 2: Configuration Parser with Validation");
    println!("{}", "-".repeat(60));

    let input = r#"# Configuration with various value types
server_port = 8080
admin_email = admin@example.com
debug_mode = true
_invalid = value
return = something
privileged_port = 80
"#;

    println!("\nInput:");
    println!("{}\n", input);

    let parser = ConfigParser::new();
    match parser.parse(input) {
        Ok(pairs) => {
            println!("Parsed configuration:");
            for (key, value, valid) in pairs {
                let status = if valid { "✓" } else { "✗" };
                println!("  {} {} = {}", status, key, value);
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
        }
    }

    // Pattern 3: Type-safe extraction
    println!("\n{}", "-".repeat(60));
    println!("Pattern 3: Type-Safe Value Extraction");
    println!("{}", "-".repeat(60));

    println!("\nExtract port:");
    match extract_port("8080") {
        Ok(port) => println!("  ✓ Extracted port: {}", port),
        Err(e) => println!("  ✗ {}", e),
    }
    match extract_port("80") {
        Ok(port) => println!("  ✓ Extracted port: {}", port),
        Err(e) => println!("  ✗ {}", e),
    }

    println!("\nExtract email:");
    match extract_email("user@example.com") {
        Ok(email) => println!("  ✓ Extracted email: {}", email),
        Err(e) => println!("  ✗ {}", e),
    }

    println!("\nExtract identifier:");
    match extract_identifier("my_variable") {
        Ok(id) => println!("  ✓ Extracted identifier: {}", id),
        Err(e) => println!("  ✗ {}", e),
    }
    match extract_identifier("if") {
        Ok(id) => println!("  ✓ Extracted identifier: {}", id),
        Err(e) => println!("  ✗ {}", e),
    }
}
