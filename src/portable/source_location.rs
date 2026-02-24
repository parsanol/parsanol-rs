//! Source Location Utilities
//!
//! This module provides utilities for tracking and formatting source code positions.
//! It consolidates line/column calculation logic used throughout the parser.

use std::fmt;

/// A position in source code
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourcePosition {
    /// Byte offset from start of input
    pub offset: usize,
    /// Line number (1-based)
    pub line: usize,
    /// Column number (1-based, UTF-8 aware)
    pub column: usize,
}

impl SourcePosition {
    /// Create a new source position
    #[inline]
    pub fn new(offset: usize, line: usize, column: usize) -> Self {
        Self {
            offset,
            line,
            column,
        }
    }

    /// Create a position at the start of input
    #[inline]
    pub fn start() -> Self {
        Self {
            offset: 0,
            line: 1,
            column: 1,
        }
    }

    /// Calculate position from an offset in the input
    pub fn from_offset(input: &str, offset: usize) -> Self {
        let offset = offset.min(input.len());

        let mut line = 1;
        let mut column = 1;
        let mut current_offset = 0;

        for ch in input.chars() {
            if current_offset >= offset {
                break;
            }

            if ch == '\n' {
                line += 1;
                column = 1;
            } else {
                column += 1;
            }

            current_offset += ch.len_utf8();
        }

        Self {
            offset,
            line,
            column,
        }
    }

    /// Get a slice of source code around this position
    pub fn get_context<'a>(&self, input: &'a str, context_lines: usize) -> SourceContext<'a> {
        let start_offset = self.offset.saturating_sub(context_lines * 80);
        let end_offset = (self.offset + context_lines * 80).min(input.len());

        SourceContext {
            source: &input[start_offset..end_offset],
            position: *self,
            context_start_offset: start_offset,
        }
    }
}

impl fmt::Display for SourcePosition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}, column {}", self.line, self.column)
    }
}

impl Default for SourcePosition {
    fn default() -> Self {
        Self::start()
    }
}

/// A range in source code
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceSpan {
    /// Start position
    pub start: SourcePosition,
    /// End position
    pub end: SourcePosition,
}

impl SourceSpan {
    /// Create a new span
    #[inline]
    pub fn new(start: SourcePosition, end: SourcePosition) -> Self {
        Self { start, end }
    }

    /// Create a span at a single position
    #[inline]
    pub fn at(offset: usize, line: usize, column: usize) -> Self {
        let pos = SourcePosition::new(offset, line, column);
        Self {
            start: pos,
            end: pos,
        }
    }

    /// Create a span from offsets
    pub fn from_offsets(input: &str, start_offset: usize, end_offset: usize) -> Self {
        Self {
            start: SourcePosition::from_offset(input, start_offset),
            end: SourcePosition::from_offset(input, end_offset),
        }
    }

    /// Create a zero-length span at the start
    #[inline]
    pub fn start() -> Self {
        Self {
            start: SourcePosition::start(),
            end: SourcePosition::start(),
        }
    }

    /// Check if this span contains an offset
    #[inline]
    pub fn contains(&self, offset: usize) -> bool {
        offset >= self.start.offset && offset <= self.end.offset
    }

    /// Get the length of this span in bytes
    #[inline]
    pub fn len(&self) -> usize {
        self.end.offset.saturating_sub(self.start.offset)
    }

    /// Check if this is a zero-length span
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.start.offset == self.end.offset
    }

    /// Merge this span with another, returning a span that covers both
    ///
    /// The resulting span starts at the earlier start position and ends at
    /// the later end position.
    #[inline]
    pub fn merge(&self, other: &SourceSpan) -> SourceSpan {
        let start = if self.start.offset <= other.start.offset {
            self.start
        } else {
            other.start
        };
        let end = if self.end.offset >= other.end.offset {
            self.end
        } else {
            other.end
        };
        SourceSpan { start, end }
    }

    /// Check if this span overlaps with another
    #[inline]
    pub fn overlaps(&self, other: &SourceSpan) -> bool {
        self.start.offset <= other.end.offset && other.start.offset <= self.end.offset
    }

    /// Check if this span is adjacent to another (end of one is start of other)
    #[inline]
    pub fn is_adjacent(&self, other: &SourceSpan) -> bool {
        self.end.offset == other.start.offset || other.end.offset == self.start.offset
    }
}

impl fmt::Display for SourceSpan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.start.line == self.end.line {
            write!(
                f,
                "line {}, columns {}-{}",
                self.start.line, self.start.column, self.end.column
            )
        } else {
            write!(
                f,
                "line {}, column {} to line {}, column {}",
                self.start.line, self.start.column, self.end.line, self.end.column
            )
        }
    }
}

impl Default for SourceSpan {
    fn default() -> Self {
        Self::start()
    }
}

/// Context around a source position
#[derive(Debug, Clone)]
pub struct SourceContext<'a> {
    /// The source code snippet
    pub source: &'a str,
    /// The position in question
    pub position: SourcePosition,
    /// Offset where context starts
    pub context_start_offset: usize,
}

impl<'a> SourceContext<'a> {
    /// Format the context with an underline
    pub fn format_with_underline(&self) -> String {
        let mut result = String::new();

        // Calculate relative position within context
        let relative_offset = self.position.offset.saturating_sub(self.context_start_offset);

        // Find the line containing the position
        let mut line_start = 0;
        let mut line_end = self.source.len();

        for (i, ch) in self.source.char_indices() {
            if ch == '\n' && i < relative_offset {
                line_start = i + 1;
            }
            if ch == '\n' && i >= relative_offset {
                line_end = i;
                break;
            }
            let _ = i + ch.len_utf8(); // Track offset (unused but kept for clarity)
        }

        let line_content = &self.source[line_start..line_end];
        result.push_str(line_content);
        result.push('\n');

        // Add underline
        let underline_pos = relative_offset.saturating_sub(line_start);
        for _ in 0..underline_pos {
            result.push(' ');
        }
        result.push('^');

        result
    }
}

/// Convert a byte offset to line and column numbers
///
/// This is the primary utility function for position calculation.
/// Line and column numbers are 1-based.
#[inline]
pub fn offset_to_line_col(input: &str, offset: usize) -> (usize, usize) {
    let pos = SourcePosition::from_offset(input, offset);
    (pos.line, pos.column)
}

/// Get the line content at a given offset
pub fn get_line_at_offset(input: &str, offset: usize) -> &str {
    let offset = offset.min(input.len());

    // Find start of line
    let line_start = if let Some(pos) = input[..offset].rfind('\n') {
        pos + 1
    } else {
        0
    };

    // Find end of line
    let line_end = if let Some(pos) = input[offset..].find('\n') {
        offset + pos
    } else {
        input.len()
    };

    &input[line_start..line_end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_position_start() {
        let pos = SourcePosition::start();
        assert_eq!(pos.offset, 0);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 1);
    }

    #[test]
    fn test_source_position_from_offset_start() {
        let input = "hello world";
        let pos = SourcePosition::from_offset(input, 0);
        assert_eq!(pos.offset, 0);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 1);
    }

    #[test]
    fn test_source_position_from_offset_middle() {
        let input = "hello world";
        let pos = SourcePosition::from_offset(input, 6);
        assert_eq!(pos.offset, 6);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 7);
    }

    #[test]
    fn test_source_position_from_offset_newline() {
        let input = "hello\nworld";
        let pos = SourcePosition::from_offset(input, 6);
        assert_eq!(pos.offset, 6);
        assert_eq!(pos.line, 2);
        assert_eq!(pos.column, 1);
    }

    #[test]
    fn test_source_position_from_offset_after_newline() {
        let input = "hello\nworld";
        let pos = SourcePosition::from_offset(input, 8);
        assert_eq!(pos.offset, 8);
        assert_eq!(pos.line, 2);
        assert_eq!(pos.column, 3);
    }

    #[test]
    fn test_source_position_from_offset_multibyte() {
        let input = "hello 世界";
        let pos = SourcePosition::from_offset(input, 6); // After space, before 世
        assert_eq!(pos.offset, 6);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 7);
    }

    #[test]
    fn test_source_position_from_offset_beyond_end() {
        let input = "hello";
        let pos = SourcePosition::from_offset(input, 100);
        assert_eq!(pos.offset, 5); // Clamped to input length
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 6);
    }

    #[test]
    fn test_source_span_contains() {
        let span = SourceSpan::from_offsets("hello world", 2, 7);
        assert!(span.contains(2));
        assert!(span.contains(5));
        assert!(span.contains(7));
        assert!(!span.contains(1));
        assert!(!span.contains(8));
    }

    #[test]
    fn test_source_span_len() {
        let span = SourceSpan::from_offsets("hello world", 2, 7);
        assert_eq!(span.len(), 5);
    }

    #[test]
    fn test_offset_to_line_col() {
        let input = "line1\nline2\nline3";
        assert_eq!(offset_to_line_col(input, 0), (1, 1));
        assert_eq!(offset_to_line_col(input, 5), (1, 6));
        assert_eq!(offset_to_line_col(input, 6), (2, 1));
        assert_eq!(offset_to_line_col(input, 11), (2, 6));
        assert_eq!(offset_to_line_col(input, 12), (3, 1));
    }

    #[test]
    fn test_get_line_at_offset() {
        let input = "line1\nline2\nline3";
        assert_eq!(get_line_at_offset(input, 0), "line1");
        assert_eq!(get_line_at_offset(input, 5), "line1");
        assert_eq!(get_line_at_offset(input, 6), "line2");
        assert_eq!(get_line_at_offset(input, 10), "line2");
        assert_eq!(get_line_at_offset(input, 12), "line3");
    }

    #[test]
    fn test_source_position_display() {
        let pos = SourcePosition::new(10, 3, 5);
        assert_eq!(format!("{}", pos), "line 3, column 5");
    }

    #[test]
    fn test_source_span_display_same_line() {
        let span = SourceSpan::from_offsets("hello", 1, 4);
        assert_eq!(format!("{}", span), "line 1, columns 2-5");
    }

    #[test]
    fn test_source_span_display_different_lines() {
        let start = SourcePosition::new(0, 1, 1);
        let end = SourcePosition::new(10, 2, 5);
        let span = SourceSpan::new(start, end);
        assert_eq!(format!("{}", span), "line 1, column 1 to line 2, column 5");
    }

    #[test]
    fn test_source_context_format() {
        let input = "hello world\nfoo bar";
        let pos = SourcePosition::from_offset(input, 12);
        let context = pos.get_context(input, 1);

        let formatted = context.format_with_underline();
        assert!(formatted.contains("foo bar"));
        assert!(formatted.contains("^"));
    }

    #[test]
    fn test_source_span_merge() {
        let input = "hello world";
        let span1 = SourceSpan::from_offsets(input, 0, 5);
        let span2 = SourceSpan::from_offsets(input, 6, 11);
        let merged = span1.merge(&span2);
        assert_eq!(merged.start.offset, 0);
        assert_eq!(merged.end.offset, 11);
    }

    #[test]
    fn test_source_span_merge_overlapping() {
        let input = "hello world";
        let span1 = SourceSpan::from_offsets(input, 0, 5);
        let span2 = SourceSpan::from_offsets(input, 3, 8);
        let merged = span1.merge(&span2);
        assert_eq!(merged.start.offset, 0);
        assert_eq!(merged.end.offset, 8);
    }

    #[test]
    fn test_source_span_overlaps() {
        let input = "hello world";
        let span1 = SourceSpan::from_offsets(input, 0, 5);
        let span2 = SourceSpan::from_offsets(input, 3, 8);
        let span3 = SourceSpan::from_offsets(input, 6, 11);

        assert!(span1.overlaps(&span2));
        assert!(span2.overlaps(&span1));
        assert!(span2.overlaps(&span3));
        assert!(!span1.overlaps(&span3));
        assert!(!span3.overlaps(&span1));
    }

    #[test]
    fn test_source_span_is_adjacent() {
        let input = "hello world";
        let span1 = SourceSpan::from_offsets(input, 0, 5);
        let span2 = SourceSpan::from_offsets(input, 5, 11);
        let span3 = SourceSpan::from_offsets(input, 6, 11);

        assert!(span1.is_adjacent(&span2));
        assert!(span2.is_adjacent(&span1));
        assert!(!span1.is_adjacent(&span3));
    }
}
