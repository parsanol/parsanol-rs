//! Email Address Parser Example
//!
//! This example demonstrates parsing and sanitizing email addresses.
//! Handles both standard format and obfuscated formats (at/dot).
//! Based on the Parslet email_parser.rb example.
//!
//! Run with: cargo run --example email_parser --no-default-features

#![allow(clippy::print_literal)]

use parsanol::portable::{
    parser_dsl::{choice, dynamic, re, seq, str, GrammarBuilder},
    AstArena, Grammar, PortableParser,
};

/// Build an email grammar
fn build_email_grammar() -> Grammar {
    GrammarBuilder::new()
        // Space characters
        .rule("space", re("[ \\t\\n\\r]+"))
        // Word: alphanumeric sequence
        .rule("word", re("[a-zA-Z0-9]+"))
        // At symbol: @ or "at" or "AT" (with optional dashes)
        .rule(
            "at",
            choice(vec![
                dynamic(str("@")),
                dynamic(seq(vec![
                    dynamic(re("[_-]?")),
                    dynamic(choice(vec![dynamic(str("at")), dynamic(str("AT"))])),
                    dynamic(re("[_-]?")),
                ])),
            ]),
        )
        // Dot: . or "dot" or "DOT" (with optional dashes)
        .rule(
            "dot",
            choice(vec![
                dynamic(str(".")),
                dynamic(seq(vec![
                    dynamic(re("[_-]?")),
                    dynamic(choice(vec![dynamic(str("dot")), dynamic(str("DOT"))])),
                    dynamic(re("[_-]?")),
                ])),
            ]),
        )
        // Username: word followed by optional (dot word) sequences
        .rule(
            "username",
            seq(vec![
                dynamic(re("[a-zA-Z0-9]+")),
                dynamic(re("(?:[ \\t]*[._-][ \\t]*[a-zA-Z0-9]+)*")),
            ]),
        )
        // Domain: word followed by (dot word) sequences
        .rule(
            "domain",
            seq(vec![
                dynamic(re("[a-zA-Z0-9]+")),
                dynamic(re("(?:[ \\t]*[._-][ \\t]*[a-zA-Z0-9]+)+")),
            ]),
        )
        // Full email: username @ domain
        .rule(
            "email",
            seq(vec![
                dynamic(re("[a-zA-Z0-9]+(?:[ \\t]*[._-][ \\t]*[a-zA-Z0-9]+)*")),
                dynamic(re("[ \\t]*")),
                dynamic(choice(vec![
                    dynamic(str("@")),
                    dynamic(re("[_-]?(?:at|AT)[_-]?")),
                ])),
                dynamic(re("[ \\t]*")),
                dynamic(re("[a-zA-Z0-9]+(?:[ \\t]*[._-][ \\t]*[a-zA-Z0-9]+)+")),
            ]),
        )
        .build()
}

/// Parsed email components
#[derive(Debug, Clone)]
pub struct EmailParts {
    pub username: String,
    pub domain: String,
}

impl std::fmt::Display for EmailParts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{}", self.username, self.domain)
    }
}

/// Parse an email address
pub fn parse_email(input: &str) -> Result<EmailParts, String> {
    let input = input.trim();
    let grammar = build_email_grammar();
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    parser
        .parse()
        .map_err(|e| format!("Parse error: {:?}", e))?;

    // Manual parsing for the actual extraction
    // Handle both standard @ and obfuscated "at" formats
    let lower = input.to_lowercase();

    // Find the separator (@, at, AT, etc.)
    let (username_part, domain_part) = if let Some(pos) = input.find('@') {
        (&input[..pos], &input[pos + 1..])
    } else if let Some(pos) = lower.find(" at ") {
        (&input[..pos], &input[pos + 4..])
    } else if let Some(pos) = lower.find("-at-") {
        (&input[..pos], &input[pos + 4..])
    } else if let Some(pos) = lower.find("_at_") {
        (&input[..pos], &input[pos + 4..])
    } else if let Some(pos) = lower.find(" at") {
        (&input[..pos], &input[pos + 3..])
    } else if let Some(pos) = lower.find("at ") {
        (&input[..pos], &input[pos + 3..])
    } else {
        return Err("No email separator (@ or 'at') found".to_string());
    };

    // Clean up: remove spaces and convert dot variations
    let username = sanitize_part(username_part);
    let domain = sanitize_part(domain_part);

    Ok(EmailParts { username, domain })
}

/// Sanitize a part of the email (username or domain)
fn sanitize_part(part: &str) -> String {
    let lower = part.to_lowercase();
    let mut result = String::new();
    let chars: Vec<char> = lower.chars().collect();

    let mut i = 0;
    while i < chars.len() {
        // Check for "dot"
        if i + 3 <= chars.len() && &lower[i..i + 3] == "dot" {
            // Check if surrounded by valid delimiters
            let before_ok = i == 0 || !chars[i - 1].is_alphanumeric();
            let after_ok = i + 3 >= chars.len() || !chars[i + 3].is_alphanumeric();
            if before_ok && after_ok {
                result.push('.');
                i += 3;
                continue;
            }
        }
        // Skip spaces, underscores, dashes (but keep dots)
        let c = chars[i];
        if c == ' ' || c == '_' || c == '-' {
            // Skip
        } else {
            result.push(c);
        }
        i += 1;
    }

    result
}

fn main() {
    println!("Email Address Parser Example");
    println!("============================\n");

    let emails = [
        // Standard format
        "user@example.com",
        "john.doe@example.com",
        "a.b.c.d@gmail.com",
        // With spaces
        "user @ example.com",
        "john . doe @ example . com",
        // Obfuscated format
        "user at example dot com",
        "john-doe-at-example-dot-com",
        "john_doe_at_example_dot_com",
        "user AT example DOT com",
        // Mixed
        "first.last at gmail.com",
        "test-user@example dot org",
    ];

    println!("{:<40} | {}", "Input", "Parsed Email");
    println!("{}", "-".repeat(70));

    for email in emails {
        match parse_email(email) {
            Ok(parts) => {
                println!("{:<40} | {}", email, parts);
            }
            Err(e) => {
                println!("{:<40} | ERROR: {}", email, e);
            }
        }
    }

    // Demonstrate sanitization
    println!("\nSanitization Examples:");
    println!("-----------------------");
    let examples = [
        ("user at example dot com", "user@example.com"),
        ("john.doe @ example . com", "john.doe@example.com"),
        ("test-user-at-gmail-dot-com", "testuser@gmail.com"),
    ];

    for (input, expected) in examples {
        if let Ok(parts) = parse_email(input) {
            let result = format!("{}@{}", parts.username, parts.domain);
            let status = if result == expected { "✓" } else { "✗" };
            println!(
                "  {} {} -> {} (expected: {})",
                status, input, result, expected
            );
        }
    }
}
