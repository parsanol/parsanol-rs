//! Rich Error Reporting for Parsanol
//!
//! This module provides detailed, tree-structured error messages similar to Parslet.
//!
//! # Example Output
//!
//! ```text
//! Expected at line 3, column 5:
//! `- Failed to match sequence (expression operator expression)
//!    `- Expected one of ['+', '-', '*', '/']
//!       `- Unexpected end of input
//! ```

use std::fmt;

/// Source position (line, column, byte offset)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// Start byte offset
    pub start: usize,
    /// End byte offset
    pub end: usize,
    /// Start line (1-based)
    pub start_line: usize,
    /// Start column (1-based)
    pub start_column: usize,
    /// End line (1-based)
    pub end_line: usize,
    /// End column (1-based)
    pub end_column: usize,
}

impl Span {
    /// Create a new span at a single position
    pub fn at(offset: usize, line: usize, column: usize) -> Self {
        Self {
            start: offset,
            end: offset,
            start_line: line,
            start_column: column,
            end_line: line,
            end_column: column,
        }
    }

    /// Create a span covering a range
    pub fn range(
        start: usize,
        end: usize,
        start_line: usize,
        start_column: usize,
        end_line: usize,
        end_column: usize,
    ) -> Self {
        Self {
            start,
            end,
            start_line,
            start_column,
            end_line,
            end_column,
        }
    }

    /// Merge two spans into one covering both
    pub fn merge(&self, other: &Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
            start_line: if self.start <= other.start {
                self.start_line
            } else {
                other.start_line
            },
            start_column: if self.start <= other.start {
                self.start_column
            } else {
                other.start_column
            },
            end_line: if self.end >= other.end {
                self.end_line
            } else {
                other.end_line
            },
            end_column: if self.end >= other.end {
                self.end_column
            } else {
                other.end_column
            },
        }
    }
}

impl Default for Span {
    fn default() -> Self {
        Self::at(0, 1, 1)
    }
}

/// A rich, tree-structured parse error
#[derive(Debug, Clone)]
pub struct RichError {
    /// The error message
    pub message: String,
    /// Where the error occurred
    pub span: Span,
    /// What was being parsed (e.g., "expression", "term")
    pub context: Option<String>,
    /// Child errors (causes)
    pub children: Vec<RichError>,
    /// Error severity
    pub severity: ErrorSeverity,
}

/// Error severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorSeverity {
    /// Just a note
    Note,
    /// Warning
    Warning,
    /// Error
    Error,
    /// Fatal error (cannot continue)
    Fatal,
}

impl RichError {
    /// Create a new error at a position
    pub fn at(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
            context: None,
            children: Vec::new(),
            severity: ErrorSeverity::Error,
        }
    }

    /// Create an error at a single position
    pub fn at_position(
        message: impl Into<String>,
        offset: usize,
        line: usize,
        column: usize,
    ) -> Self {
        Self::at(message, Span::at(offset, line, column))
    }

    /// Add context to the error
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }

    /// Add a child error (cause)
    pub fn with_child(mut self, child: RichError) -> Self {
        self.children.push(child);
        self
    }

    /// Set severity
    pub fn with_severity(mut self, severity: ErrorSeverity) -> Self {
        self.severity = severity;
        self
    }

    /// Create an "expected" error
    pub fn expected(expected: &str, span: Span) -> Self {
        Self::at(format!("Expected {}", expected), span)
    }

    /// Create an "unexpected" error
    pub fn unexpected(found: &str, span: Span) -> Self {
        Self::at(format!("Unexpected {}", found), span)
    }

    /// Create a "failed to match" error
    pub fn failed_to_match(what: &str, span: Span) -> Self {
        Self::at(format!("Failed to match {}", what), span)
    }

    /// Get the deepest error position
    pub fn deepest_position(&self) -> Span {
        if self.children.is_empty() {
            return self.span;
        }

        let deepest_child = self
            .children
            .iter()
            .map(|c| c.deepest_position())
            .max_by_key(|s| s.start)
            .unwrap_or(self.span);

        if deepest_child.start > self.span.start {
            deepest_child
        } else {
            self.span
        }
    }

    /// Format as ASCII tree (like Parslet)
    pub fn ascii_tree(&self) -> String {
        let mut output = String::new();
        self.ascii_tree_impl(&mut output, "", true);
        output
    }

    fn ascii_tree_impl(&self, output: &mut String, prefix: &str, last: bool) {
        let connector = if last { "`- " } else { "|- " };
        let child_prefix = if last { "   " } else { "|  " };

        output.push_str(prefix);
        output.push_str(connector);
        output.push_str(&self.message);

        if let Some(ref ctx) = self.context {
            output.push_str(&format!(" (in {})", ctx));
        }

        output.push('\n');

        for (i, child) in self.children.iter().enumerate() {
            let is_last = i == self.children.len() - 1;
            child.ascii_tree_impl(output, &format!("{}{}", prefix, child_prefix), is_last);
        }
    }

    /// Format with source code context
    pub fn format_with_source(&self, source: &str) -> String {
        let mut output = String::new();

        // Get the deepest position for context
        let pos = self.deepest_position();

        // Format header
        output.push_str(&format!(
            "Error at line {}, column {}:\n",
            pos.start_line, pos.start_column
        ));

        // Get source line
        let line_start = source[..pos.start.min(source.len())]
            .rfind('\n')
            .map(|n| n + 1)
            .unwrap_or(0);
        let line_end = source[pos.start.min(source.len())..]
            .find('\n')
            .map(|n| pos.start + n)
            .unwrap_or(source.len());

        let line = &source[line_start..line_end.min(source.len())];

        // Print line with error pointer
        output.push_str(line);
        output.push('\n');

        // Error pointer
        for _ in 0..(pos.start_column.saturating_sub(1)) {
            output.push(' ');
        }
        output.push_str("^\n");

        // Print tree
        output.push_str(&self.ascii_tree());

        output
    }
}

impl fmt::Display for RichError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Error at line {}, column {}: {}",
            self.span.start_line, self.span.start_column, self.message
        )
    }
}

impl std::error::Error for RichError {}

/// Error builder for constructing rich errors
pub struct ErrorBuilder {
    message: String,
    span: Span,
    context: Option<String>,
    children: Vec<RichError>,
    severity: ErrorSeverity,
}

impl ErrorBuilder {
    /// Create a new error builder
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            span: Span::default(),
            context: None,
            children: Vec::new(),
            severity: ErrorSeverity::Error,
        }
    }

    /// Set the span
    pub fn span(mut self, span: Span) -> Self {
        self.span = span;
        self
    }

    /// Set position
    pub fn at(mut self, offset: usize, line: usize, column: usize) -> Self {
        self.span = Span::at(offset, line, column);
        self
    }

    /// Set context
    pub fn context(mut self, ctx: impl Into<String>) -> Self {
        self.context = Some(ctx.into());
        self
    }

    /// Add a child error
    pub fn child(mut self, child: RichError) -> Self {
        self.children.push(child);
        self
    }

    /// Set severity
    pub fn severity(mut self, severity: ErrorSeverity) -> Self {
        self.severity = severity;
        self
    }

    /// Build the error
    pub fn build(self) -> RichError {
        RichError {
            message: self.message,
            span: self.span,
            context: self.context,
            children: self.children,
            severity: self.severity,
        }
    }
}

/// Convert byte offset to line/column
pub fn offset_to_line_col(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    let mut pos = 0;

    for ch in source.chars() {
        if pos >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
        pos += ch.len_utf8();
    }

    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_merge() {
        let a = Span::at(10, 2, 5);
        let b = Span::at(20, 3, 10);
        let merged = a.merge(&b);
        assert_eq!(merged.start, 10);
        assert_eq!(merged.end, 20);
    }

    #[test]
    fn test_rich_error_ascii_tree() {
        let error = ErrorBuilder::new("Failed to parse expression")
            .at(10, 2, 5)
            .context("expression")
            .child(
                ErrorBuilder::new("Expected '+' or '-'")
                    .at(10, 2, 5)
                    .build(),
            )
            .build();

        let tree = error.ascii_tree();
        assert!(tree.contains("Failed to parse expression"));
        assert!(tree.contains("Expected"));
    }

    #[test]
    fn test_deepest_position() {
        let parent = ErrorBuilder::new("Parent")
            .at(10, 2, 5)
            .child(ErrorBuilder::new("Child 1").at(20, 3, 10).build())
            .child(ErrorBuilder::new("Child 2").at(30, 4, 15).build())
            .build();

        let deepest = parent.deepest_position();
        assert_eq!(deepest.start, 30);
    }

    #[test]
    fn test_format_with_source() {
        let source = "hello world\nthis is a test\nmore text";
        let error = ErrorBuilder::new("Unexpected token").at(15, 2, 5).build();

        let formatted = error.format_with_source(source);
        assert!(formatted.contains("line 2, column 5"));
        assert!(formatted.contains("Unexpected token"));
    }
}
