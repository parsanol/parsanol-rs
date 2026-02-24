//! Code Linter Example
//!
//! Demonstrates using slice positions for linting and code analysis.
//! Shows how to track exact source locations for reporting issues.
//!
//! Run with: cargo run --example linter --no-default-features

#![allow(clippy::print_literal)]

use parsanol::portable::{
    offset_to_line_col,
    parser_dsl::{re, GrammarBuilder},
    AstArena, Grammar, PortableParser,
};

/// A lint issue found in the code
#[derive(Debug, Clone)]
pub struct LintIssue {
    /// Issue severity
    pub severity: Severity,
    /// Issue code (e.g., "E001", "W001")
    pub code: String,
    /// Human-readable message
    pub message: String,
    /// Line number (1-based)
    pub line: usize,
    /// Column number (1-based)
    pub column: usize,
    /// Byte offset in input
    pub offset: usize,
    /// Optional suggestion for fixing
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Error => write!(f, "error"),
            Severity::Warning => write!(f, "warning"),
            Severity::Info => write!(f, "info"),
            Severity::Hint => write!(f, "hint"),
        }
    }
}

/// Reserved words that cannot be used as keys
const RESERVED_WORDS: &[&str] = &[
    "if", "else", "while", "for", "return", "fn", "let", "const", "true", "false", "null", "break",
    "continue", "import", "export",
];

/// Build a simple configuration grammar
/// Note: The grammar just validates that input is text. Real validation
/// happens in the linting logic using slice positions.
fn build_config_grammar() -> Grammar {
    GrammarBuilder::new()
        // Match any text including newlines (config files are just text)
        .rule("config", re("(?s).*"))
        .build()
}

/// Lint a configuration file and return issues
fn lint_config(input: &str) -> Result<Vec<LintIssue>, String> {
    // First, parse to validate syntax
    let grammar = build_config_grammar();
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    let _ast = parser
        .parse()
        .map_err(|e| format!("Parse error: {:?}", e))?;

    let mut issues = Vec::new();

    // Process each line for semantic issues
    for (line_idx, line) in input.lines().enumerate() {
        let line_start: usize = input.lines().take(line_idx).map(|l| l.len() + 1).sum();
        let trimmed = line.trim();

        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
            continue;
        }

        // Check for key-value pairs
        if let Some(eq_pos) = line.find('=') {
            let key = line[..eq_pos].trim();
            let value = line[eq_pos + 1..].trim();

            let key_offset = line_start + line.find(key).unwrap_or(0);
            let value_offset = line_start
                + eq_pos
                + 1
                + line[eq_pos + 1..]
                    .find(value.chars().next().unwrap_or(' '))
                    .unwrap_or(0);

            let (key_line, key_col) = offset_to_line_col(input, key_offset);
            let (val_line, val_col) = offset_to_line_col(input, value_offset);

            // Check 1: Reserved word as key
            if RESERVED_WORDS.contains(&key) {
                issues.push(LintIssue {
                    severity: Severity::Error,
                    code: "E001".to_string(),
                    message: format!("'{}' is a reserved word", key),
                    line: key_line,
                    column: key_col,
                    offset: key_offset,
                    suggestion: Some(format!("Rename to '{}_' or 'my_{}'", key, key)),
                });
            }

            // Check 2: Hyphens in key (prefer underscores)
            if key.contains('-') {
                issues.push(LintIssue {
                    severity: Severity::Warning,
                    code: "W001".to_string(),
                    message: format!("Key '{}' uses hyphens; prefer underscores", key),
                    line: key_line,
                    column: key_col,
                    offset: key_offset,
                    suggestion: Some(key.replace('-', "_")),
                });
            }

            // Check 3: Very short key (likely typo)
            if key.len() == 1 {
                issues.push(LintIssue {
                    severity: Severity::Info,
                    code: "I001".to_string(),
                    message: format!("Key '{}' is very short; check for typos", key),
                    line: key_line,
                    column: key_col,
                    offset: key_offset,
                    suggestion: None,
                });
            }

            // Check 4: Boolean value consistency
            let lower_value = value.to_lowercase();
            if lower_value == "yes" || lower_value == "no" {
                issues.push(LintIssue {
                    severity: Severity::Hint,
                    code: "H001".to_string(),
                    message: format!("Boolean '{}' is non-standard", value),
                    line: val_line,
                    column: val_col,
                    offset: value_offset,
                    suggestion: Some("Use 'true' or 'false'".to_string()),
                });
            }

            // Check 5: Port number range
            if key.contains("port") {
                if let Ok(port) = value.parse::<u16>() {
                    if port < 1024 {
                        issues.push(LintIssue {
                            severity: Severity::Warning,
                            code: "W002".to_string(),
                            message: format!("Port {} is a privileged port (< 1024)", port),
                            line: val_line,
                            column: val_col,
                            offset: value_offset,
                            suggestion: Some("Use a port >= 1024".to_string()),
                        });
                    }
                }
            }

            // Check 6: Hardcoded paths
            if value.starts_with("/home/") || value.starts_with("C:\\") {
                issues.push(LintIssue {
                    severity: Severity::Warning,
                    code: "W003".to_string(),
                    message: "Hardcoded path detected; may not be portable".to_string(),
                    line: val_line,
                    column: val_col,
                    offset: value_offset,
                    suggestion: Some("Use environment variables".to_string()),
                });
            }

            // Check 7: Leading/trailing whitespace in value
            if value.starts_with(' ') || value.ends_with(' ') {
                issues.push(LintIssue {
                    severity: Severity::Hint,
                    code: "H002".to_string(),
                    message: "Value has leading/trailing whitespace".to_string(),
                    line: val_line,
                    column: val_col,
                    offset: value_offset,
                    suggestion: Some("Trim or quote the value".to_string()),
                });
            }
        }
    }

    // Sort by offset
    issues.sort_by_key(|i| i.offset);

    Ok(issues)
}

/// Format lint issues for display
fn format_issues(issues: &[LintIssue], input: &str) -> String {
    let mut output = String::new();

    for issue in issues {
        // Get the line content
        let line_start = input[..issue.offset]
            .rfind('\n')
            .map(|n| n + 1)
            .unwrap_or(0);
        let line_end = input[issue.offset..]
            .find('\n')
            .map(|n| issue.offset + n)
            .unwrap_or(input.len());
        let line = &input[line_start..line_end];

        // Format: severity[code]: message at line:col
        output.push_str(&format!(
            "{}[{}]: {} at {}:{}\n",
            issue.severity, issue.code, issue.message, issue.line, issue.column
        ));

        // Show the line with caret
        output.push_str(&format!("  {}\n", line));
        output.push_str(&format!(
            "  {}^\n",
            " ".repeat(issue.column.saturating_sub(1))
        ));

        // Show suggestion if available
        if let Some(ref suggestion) = issue.suggestion {
            output.push_str(&format!("  Suggestion: {}\n", suggestion));
        }

        output.push('\n');
    }

    output
}

fn main() {
    let input = r#"# Configuration file with various issues
debug = yes
database-host = localhost
db_port = 5432
return = something
path = /home/user/data
admin_email = invalid-email
x =  value with spaces
privileged_port = 80
"#;

    println!("Code Linter Example");
    println!("{}\n", "=".repeat(60));

    println!("Input:");
    println!("{}\n", input);
    println!("{}", "-".repeat(60));

    match lint_config(input) {
        Ok(issues) => {
            if issues.is_empty() {
                println!("No issues found!");
            } else {
                println!("Found {} issue(s):\n", issues.len());
                println!("{}", format_issues(&issues, input));
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
        }
    }

    // Demonstrate position tracking
    println!("{}", "-".repeat(60));
    println!("Position Tracking Demo:");

    let test_input = "hello world\nsecond line\nthird line";
    for offset in [0, 6, 12, 23] {
        let (line, col) = offset_to_line_col(test_input, offset);
        println!("  offset {} -> line {}, column {}", offset, line, col);
    }
}
