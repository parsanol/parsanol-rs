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
//!
//! # Error Type Conversion
//!
//! This module provides seamless conversion from [`ParseError`] (the internal
//! error type) to [`RichError`] (the user-facing error type):
//!
//! ```
//! use parsanol::portable::{ParseError, RichError};
//!
//! let parse_error = ParseError::at_position(10);
//! let rich_error: RichError = parse_error.into_rich("input text");
//! ```

use std::fmt;

// Re-export SourceSpan from source_location for use in errors
pub use super::source_location::SourceSpan as Span;

// Import ParseError for conversion
use super::ast::ParseError;

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
            .max_by_key(|s| s.start.offset)
            .unwrap_or(self.span);

        if deepest_child.start.offset > self.span.start.offset {
            deepest_child
        } else {
            self.span
        }
    }

    /// Get the deepest error (the error that occurred at the furthest position)
    pub fn deepest(&self) -> &RichError {
        if self.children.is_empty() {
            return self;
        }

        // Find the child with the deepest position
        let deepest_child = self
            .children
            .iter()
            .map(|c| (c, c.deepest_position().start.offset))
            .max_by_key(|(_, offset)| *offset);

        match deepest_child {
            Some((child, child_offset)) if child_offset > self.span.start.offset => child.deepest(),
            _ => self,
        }
    }

    /// Get all leaf errors (errors with no children)
    pub fn leaves(&self) -> Vec<&RichError> {
        if self.children.is_empty() {
            return vec![self];
        }

        self.children.iter().flat_map(|c| c.leaves()).collect()
    }

    /// Alias for ascii_tree() for compatibility
    pub fn to_tree(&self) -> String {
        self.ascii_tree()
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
            pos.start.line, pos.start.column
        ));

        // Get source line
        let line_start = source[..pos.start.offset.min(source.len())]
            .rfind('\n')
            .map(|n| n + 1)
            .unwrap_or(0);
        let line_end = source[pos.start.offset.min(source.len())..]
            .find('\n')
            .map(|n| pos.start.offset + n)
            .unwrap_or(source.len());

        let line = &source[line_start..line_end.min(source.len())];

        // Print line with error pointer
        output.push_str(line);
        output.push('\n');

        // Error pointer
        for _ in 0..(pos.start.column.saturating_sub(1)) {
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
            self.span.start.line, self.span.start.column, self.message
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

// Re-export offset_to_line_col from source_location for backward compatibility
pub use super::source_location::offset_to_line_col;

// ============================================================================
// ParseError to RichError Conversion
// ============================================================================

impl ParseError {
    /// Convert this parse error to a rich error with source context
    ///
    /// This method creates a user-friendly error message with line/column
    /// information derived from the source input.
    ///
    /// # Arguments
    ///
    /// * `input` - The source input that was being parsed
    ///
    /// # Example
    ///
    /// ```
    /// use parsanol::portable::{ParseError, RichError};
    ///
    /// let error = ParseError::at_position(5);
    /// let rich = error.into_rich("hello world");
    /// println!("{}", rich);
    /// ```
    pub fn into_rich(self, input: &str) -> RichError {
        use super::source_location::SourcePosition;

        let (position, message) = match &self {
            ParseError::Failed { position } => (*position, "Parse failed".to_string()),
            ParseError::Incomplete { expected, actual } => {
                return RichError::at(
                    format!("Parse incomplete: expected {} bytes, parsed {}", expected, actual),
                    Span::default(),
                );
            }
            ParseError::InvalidGrammar { reason } => {
                return RichError::at(format!("Invalid grammar: {}", reason), Span::default());
            }
            ParseError::Internal { message } => {
                return RichError::at(format!("Internal error: {}", message), Span::default());
            }
            ParseError::InputTooLarge {
                input_size,
                max_size,
            } => {
                return RichError::at(
                    format!(
                        "Input too large: {} bytes exceeds limit of {} bytes",
                        input_size, max_size
                    ),
                    Span::default(),
                );
            }
            ParseError::RecursionLimitExceeded { depth, max_depth } => {
                return RichError::at(
                    format!(
                        "Recursion limit exceeded: depth {} exceeds limit of {}",
                        depth, max_depth
                    ),
                    Span::default(),
                );
            }
            ParseError::TimeoutExceeded {
                elapsed_ms,
                timeout_ms,
            } => {
                return RichError::at(
                    format!(
                        "Timeout exceeded: {}ms exceeds limit of {}ms",
                        elapsed_ms, timeout_ms
                    ),
                    Span::default(),
                );
            }
            ParseError::MemoryLimitExceeded {
                used_bytes,
                max_bytes,
            } => {
                return RichError::at(
                    format!(
                        "Memory limit exceeded: {} bytes exceeds limit of {} bytes",
                        used_bytes, max_bytes
                    ),
                    Span::default(),
                );
            }
            ParseError::BuilderError { message } => {
                return RichError::at(format!("Builder error: {}", message), Span::default());
            }
        };

        // Convert byte offset to line/column
        let pos = SourcePosition::from_offset(input, position);
        let span = Span::at(position, pos.line, pos.column);

        RichError::at(message, span)
    }
}

impl From<ParseError> for RichError {
    fn from(error: ParseError) -> Self {
        error.into_rich("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_merge() {
        let a = Span::at(10, 2, 5);
        let b = Span::at(20, 3, 10);
        let merged = a.merge(&b);
        assert_eq!(merged.start.offset, 10);
        assert_eq!(merged.end.offset, 20);
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
        assert_eq!(deepest.start.offset, 30);
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
