//! Error tracking and reporting for the bytecode VM
//!
//! This module provides detailed error information by tracking
//! what was expected at the furthest failure position.
//!
//! # Architecture
//!
//! - `ErrorContext` - tracks what was expected at a failure point
//! - `ErrorTracker` - collects failure contexts during execution
//! - `ErrorReporter` - builds RichError from tracked data
//!
//! # Design Principles
//!
//! - **OOP**: Each struct has a single responsibility
//! - **MECE**: All error scenarios are covered distinctly
//! - **Separation of Concerns**: Tracking is separate from reporting
//! - **Open/Closed**: New error types can be added without modification

use super::instruction::Instruction;
use super::program::Program;
use crate::portable::error::{ErrorSeverity, RichError};
use crate::portable::source_location::{offset_to_line_col, SourceSpan};

/// What was expected at a failure point
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expected {
    /// Expected a specific character
    Char(char),
    /// Expected a character from a set
    CharSet(String),
    /// Expected a literal string
    String(String),
    /// Expected a regex pattern
    Regex(String),
    /// Expected any character
    Any(usize),
    /// Expected end of input
    EndOfInput,
    /// Expected a labeled pattern
    Label(String),
    /// Unknown expectation
    Unknown,
}

impl std::fmt::Display for Expected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Expected::Char(c) => write!(f, "'{}'", c.escape_default()),
            Expected::CharSet(s) => write!(f, "one of {}", s),
            Expected::String(s) => write!(f, "{:?}", s),
            Expected::Regex(p) => write!(f, "pattern {:?}", p),
            Expected::Any(n) => write!(f, "any {} character{}", n, if *n == 1 { "" } else { "s" }),
            Expected::EndOfInput => write!(f, "end of input"),
            Expected::Label(l) => write!(f, "{}", l),
            Expected::Unknown => write!(f, "something"),
        }
    }
}

/// Context at a failure point
#[derive(Debug, Clone)]
pub struct ErrorContext {
    /// Position where the failure occurred
    pub position: usize,
    /// What was expected
    pub expected: Expected,
    /// The instruction that failed
    pub instruction_ip: usize,
}

impl ErrorContext {
    /// Create a new error context
    #[inline]
    pub fn new(position: usize, expected: Expected, instruction_ip: usize) -> Self {
        Self {
            position,
            expected,
            instruction_ip,
        }
    }
}

/// Tracks error contexts during execution
#[derive(Debug, Clone, Default)]
pub struct ErrorTracker {
    /// Furthest failure position seen
    pub furthest_position: usize,
    /// All failures at the furthest position
    pub contexts: Vec<ErrorContext>,
}

impl ErrorTracker {
    /// Create a new error tracker
    #[inline]
    pub fn new() -> Self {
        Self {
            furthest_position: 0,
            contexts: Vec::new(),
        }
    }

    /// Record a failure at the given position
    #[inline]
    pub fn record_failure(&mut self, position: usize, expected: Expected, instruction_ip: usize) {
        if position > self.furthest_position {
            // New furthest position - clear old contexts
            self.furthest_position = position;
            self.contexts.clear();
            self.contexts
                .push(ErrorContext::new(position, expected, instruction_ip));
        } else if position == self.furthest_position && !self.contexts.is_empty() {
            // Same furthest position - add context if not duplicate
            let is_duplicate = self
                .contexts
                .iter()
                .any(|ctx| ctx.position == position && ctx.expected == expected);
            if !is_duplicate {
                self.contexts
                    .push(ErrorContext::new(position, expected, instruction_ip));
            }
        }
    }

    /// Check if any failures were recorded
    #[inline]
    pub fn has_failures(&self) -> bool {
        !self.contexts.is_empty()
    }

    /// Get the furthest failure position
    #[inline]
    pub fn furthest_position(&self) -> usize {
        self.furthest_position
    }

    /// Get all expected items at the furthest position
    pub fn expected_items(&self) -> Vec<&Expected> {
        self.contexts.iter().map(|ctx| &ctx.expected).collect()
    }
}

/// Builds RichError from tracked failure data
pub struct ErrorReporter<'a> {
    /// The input text
    input: &'a str,
    /// The program (for instruction lookup in future enhancements)
    #[allow(dead_code)]
    program: &'a Program,
}

impl<'a> ErrorReporter<'a> {
    /// Create a new error reporter
    #[inline]
    pub fn new(input: &'a str, program: &'a Program) -> Self {
        Self { input, program }
    }

    /// Build a RichError from the error tracker
    pub fn build_error(&self, tracker: &ErrorTracker) -> RichError {
        if !tracker.has_failures() {
            return RichError::at("Parse failed", SourceSpan::at(0, 1, 1));
        }

        let pos = tracker.furthest_position();
        let (line, column) = offset_to_line_col(self.input, pos);
        let span = SourceSpan::at(pos, line, column);

        // Build expected items description
        let expected: Vec<&Expected> = tracker.expected_items();

        let message = if expected.len() == 1 {
            format!("Expected {}", expected[0])
        } else if expected.len() > 1 {
            let items: Vec<String> = expected.iter().map(|e| e.to_string()).collect();
            format!("Expected one of {}", items.join(", "))
        } else {
            "Parse failed".to_string()
        };

        // Create the root error
        let mut error = RichError::at(&message, span);

        // Add context about what was found
        if let Some(found) = self.get_char_at(pos) {
            error = error.with_child(
                RichError::unexpected(&format!("'{}'", found.escape_default()), span)
                    .with_severity(ErrorSeverity::Note),
            );
        } else {
            error = error.with_child(
                RichError::unexpected("end of input", span).with_severity(ErrorSeverity::Note),
            );
        }

        error
    }

    /// Get the character at a position
    fn get_char_at(&self, pos: usize) -> Option<char> {
        if pos >= self.input.len() {
            return None;
        }

        let remaining = &self.input[pos..];
        remaining.chars().next()
    }

    /// Build a RichError with instruction context
    pub fn build_error_with_context(
        &self,
        tracker: &ErrorTracker,
        instruction_context: Option<&str>,
    ) -> RichError {
        let mut error = self.build_error(tracker);

        if let Some(context) = instruction_context {
            error = error.with_context(context);
        }

        error
    }
}

/// Helper to extract expected information from an instruction
pub fn instruction_to_expected(instruction: &Instruction, program: &Program) -> Expected {
    match instruction {
        Instruction::Char { byte } => Expected::Char(*byte as char),
        Instruction::CharSet { set_idx } => {
            if let Some(set) = program.get_char_set(*set_idx) {
                let chars: String = (0..=255)
                    .filter(|&b| set.contains(b))
                    .filter_map(|b| {
                        if (32..127).contains(&b) {
                            Some(b as char)
                        } else {
                            None
                        }
                    })
                    .collect();
                Expected::CharSet(format!("[{}]", chars))
            } else {
                Expected::Unknown
            }
        }
        Instruction::String { str_idx, .. } => {
            if let Some(s) = program.get_string(*str_idx) {
                Expected::String(s.to_string())
            } else {
                Expected::Unknown
            }
        }
        Instruction::Regex { regex_idx } => {
            if let Some(p) = program.get_regex(*regex_idx) {
                Expected::Regex(p.to_string())
            } else {
                Expected::Unknown
            }
        }
        Instruction::Any { n } => Expected::Any(*n as usize),
        Instruction::End => Expected::EndOfInput,
        Instruction::Throw { label_idx } | Instruction::ThrowRec { label_idx, .. } => {
            if let Some(label) = program.get_label(*label_idx) {
                Expected::Label(label.to_string())
            } else {
                Expected::Label("unknown error".to_string())
            }
        }
        _ => Expected::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expected_display() {
        assert_eq!(Expected::Char('a').to_string(), "'a'");
        assert_eq!(
            Expected::CharSet("[abc]".to_string()).to_string(),
            "one of [abc]"
        );
        assert_eq!(
            Expected::String("hello".to_string()).to_string(),
            "\"hello\""
        );
        assert_eq!(Expected::EndOfInput.to_string(), "end of input");
        assert_eq!(Expected::Any(1).to_string(), "any 1 character");
        assert_eq!(Expected::Any(3).to_string(), "any 3 characters");
    }

    #[test]
    fn test_error_tracker_record_failure() {
        let mut tracker = ErrorTracker::new();

        // First failure at position 5
        tracker.record_failure(5, Expected::Char('a'), 0);
        assert_eq!(tracker.furthest_position(), 5);
        assert_eq!(tracker.contexts.len(), 1);

        // Failure at same position - should add
        tracker.record_failure(5, Expected::Char('b'), 1);
        assert_eq!(tracker.contexts.len(), 2);

        // Failure at earlier position - should not add
        tracker.record_failure(3, Expected::Char('c'), 2);
        assert_eq!(tracker.contexts.len(), 2);

        // New furthest position - should clear and add
        tracker.record_failure(10, Expected::Char('d'), 3);
        assert_eq!(tracker.furthest_position(), 10);
        assert_eq!(tracker.contexts.len(), 1);
    }

    #[test]
    fn test_error_tracker_deduplication() {
        let mut tracker = ErrorTracker::new();

        tracker.record_failure(5, Expected::Char('a'), 0);
        tracker.record_failure(5, Expected::Char('a'), 1); // Duplicate
        assert_eq!(tracker.contexts.len(), 1);
    }

    #[test]
    fn test_error_reporter_basic() {
        let program = Program::new();
        let input = "hello world";
        let reporter = ErrorReporter::new(input, &program);

        let mut tracker = ErrorTracker::new();
        tracker.record_failure(6, Expected::Char('x'), 0);

        let error = reporter.build_error(&tracker);

        assert!(error.message.contains("Expected"));
        assert!(error.message.contains("'x'"));
        assert_eq!(error.span.start.offset, 6);
    }

    #[test]
    fn test_error_reporter_multiple_expected() {
        let program = Program::new();
        let input = "test";
        let reporter = ErrorReporter::new(input, &program);

        let mut tracker = ErrorTracker::new();
        tracker.record_failure(2, Expected::Char('a'), 0);
        tracker.record_failure(2, Expected::Char('b'), 1);
        tracker.record_failure(2, Expected::Char('c'), 2);

        let error = reporter.build_error(&tracker);

        assert!(error.message.contains("Expected one of"));
        assert!(error.message.contains("'a'"));
        assert!(error.message.contains("'b'"));
        assert!(error.message.contains("'c'"));
    }

    #[test]
    fn test_error_reporter_end_of_input() {
        let program = Program::new();
        let input = "test";
        let reporter = ErrorReporter::new(input, &program);

        let mut tracker = ErrorTracker::new();
        tracker.record_failure(4, Expected::Char('x'), 0);

        let error = reporter.build_error(&tracker);

        // At position 4 (end of input), should show "unexpected end of input"
        assert_eq!(error.children.len(), 1);
        assert!(error.children[0].message.contains("Unexpected"));
    }
}
