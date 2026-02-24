# Code Linter Example - Rust Implementation

## How to Run

```bash
cd parsanol-rs
cargo run --example linter/basic --no-default-features
```

## Code Walkthrough

This example demonstrates how to use Parsanol's slice positions for building a code linter. The key concepts shown are:

### 1. Source Position Tracking

The linter uses `SourcePosition` to track exactly where issues occur:

```rust
use parsanol::portable::{SourcePosition, offset_to_line_col};

// Convert byte offset to line:column
let pos = offset_to_line_col(input, offset);
// pos.line, pos.column, pos.offset are all available
```

### 2. LintIssue Structure

Each lint issue contains full context for reporting:

```rust
pub struct LintIssue {
    pub severity: Severity,      // Error, Warning, Info, Hint
    pub code: String,            // e.g., "W001", "E001"
    pub message: String,         // Human-readable description
    pub position: SourcePosition, // Exact location
    pub suggestion: Option<String>, // How to fix it
}
```

### 3. Extracting Slice Positions from AST

When walking the parsed AST, we can extract the exact position of each element:

```rust
match node {
    AstNode::InputRef { offset, length } => {
        // Get the actual text
        let text = &input[*offset as usize..(*offset + *length) as usize];

        // Get line/column position
        let pos = offset_to_line_col(input, *offset as usize);

        // Now we can report issues at this exact position
    }
    // ... other node types
}
```

### 4. Pattern Matching for Lint Checks

The linter demonstrates several common lint patterns:

- **Naming Convention Check**: Detecting hyphens in keys
- **Length Checks**: Warning about very short identifiers
- **Value Validation**: Checking for trailing whitespace, boolean consistency
- **Portability Issues**: Detecting hardcoded paths

## Output Types

The linter produces a vector of `LintIssue` structs:

```rust
Vec<LintIssue>
```

Each issue can be formatted for display:

```rust
pub fn format_issues(issues: &[LintIssue], input: &str) -> String {
    // Formats as:
    // warning[W002]: Key 'database-host' uses hyphens; prefer underscores at 3:1
    //   database-host = localhost
    //   ^
    //   Suggestion: Replace hyphens with underscores: database_host
}
```

## Design Decisions

### Why Track Positions?

Position tracking enables:
1. **Precise error messages**: Point users to the exact line and column
2. **IDE integration**: LSP servers need positions for diagnostics
3. **Auto-fixes**: Knowing the position allows programmatic correction

### Severity Levels

Following IDE/linter conventions:
- **Error**: Must fix (syntax errors, type errors)
- **Warning**: Should fix (style issues, potential bugs)
- **Info**: FYI (best practices, conventions)
- **Hint**: Minor suggestions (formatting, optimizations)

## Extending the Linter

To add new lint checks:

1. Add a new check method in `check_pair()`
2. Create a unique code (e.g., "W004")
3. Choose appropriate severity
4. Provide helpful suggestion when possible

Example new check:

```rust
// Check for TODO comments
if value_str.contains("TODO") {
    issues.push(LintIssue {
        severity: Severity::Info,
        code: "I002".to_string(),
        message: "TODO found in configuration".to_string(),
        position: value_pos,
        suggestion: Some("Create an issue to track this".to_string()),
    });
}
```
