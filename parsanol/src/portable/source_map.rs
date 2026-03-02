//! Source Map Support for AST Tracking
//!
//! This module provides utilities for tracking original source positions
//! through parsing and transformations. This is essential for:
//! - IDE integration (hover tooltips, go-to-definition)
//! - Debugging (breakpoints at source positions)
//! - Error reporting with precise locations
//!
//! # Overview
//!
//! The core type is [`SourceMapped<T>`], which wraps any value with its
//! original source location information:
//!
//! ```rust,ignore
//! use parsanol::portable::source_map::SourceMapped;
//! use parsanol::portable::source_location::SourceSpan;
//!
//! // A value with its source location
//! let mapped = SourceMapped {
//!     value: 42,
//!     span: SourceSpan::at(10, 2, 5),
//! };
//!
//! // Access the value and span
//! assert_eq!(*mapped, 42);
//! assert_eq!(mapped.span().start.line, 2);
//! ```

use super::source_location::SourceSpan;
use std::ops::{Deref, DerefMut};

/// A value wrapped with its original source location
///
/// This type preserves source position information through transformations,
/// enabling features like IDE integration and precise error reporting.
///
/// # Example
///
/// ```
/// use parsanol::portable::source_map::SourceMapped;
/// use parsanol::portable::source_location::SourceSpan;
///
/// let mapped = SourceMapped::new(
///     42,
///     SourceSpan::at(10, 2, 5)
/// );
///
/// assert_eq!(*mapped, 42);
/// assert_eq!(mapped.span().start.line, 2);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceMapped<T> {
    /// The wrapped value
    value: T,
    /// The source location
    span: SourceSpan,
}

impl<T> SourceMapped<T> {
    /// Create a new source-mapped value
    #[inline]
    pub fn new(value: T, span: SourceSpan) -> Self {
        Self { value, span }
    }

    /// Create a source-mapped value with a zero-length span
    #[inline]
    pub fn at(value: T, offset: usize, line: usize, column: usize) -> Self {
        Self {
            value,
            span: SourceSpan::at(offset, line, column),
        }
    }

    /// Get a reference to the inner value
    #[inline]
    pub fn inner(&self) -> &T {
        &self.value
    }

    /// Get a mutable reference to the inner value
    #[inline]
    pub fn inner_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// Unwrap to get the inner value
    #[inline]
    pub fn into_inner(self) -> T {
        self.value
    }

    /// Get the source span
    #[inline]
    pub fn span(&self) -> &SourceSpan {
        &self.span
    }

    /// Map the inner value while preserving the span
    #[inline]
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> SourceMapped<U> {
        SourceMapped {
            value: f(self.value),
            span: self.span,
        }
    }

    /// Map the inner value with a fallible function
    #[inline]
    pub fn try_map<U, E, F: FnOnce(T) -> Result<U, E>>(self, f: F) -> Result<SourceMapped<U>, E> {
        Ok(SourceMapped {
            value: f(self.value)?,
            span: self.span,
        })
    }

    /// Change the span
    #[inline]
    pub fn with_span(mut self, span: SourceSpan) -> Self {
        self.span = span;
        self
    }

    /// Combine with another source-mapped value
    ///
    /// The resulting span will cover both inputs.
    #[inline]
    pub fn combine<U, F: FnOnce(T, U) -> V, V>(
        self,
        other: SourceMapped<U>,
        f: F,
    ) -> SourceMapped<V> {
        SourceMapped {
            value: f(self.value, other.value),
            span: self.span.merge(&other.span),
        }
    }
}

impl<T> Deref for SourceMapped<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> DerefMut for SourceMapped<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

impl<T: Default> Default for SourceMapped<T> {
    fn default() -> Self {
        Self {
            value: T::default(),
            span: SourceSpan::default(),
        }
    }
}

/// A collection of source-mapped values
///
/// Useful for tracking spans across multiple items (e.g., array elements).
#[derive(Debug, Clone, Default)]
pub struct SourceMapCollection<T> {
    /// The items with their spans
    items: Vec<SourceMapped<T>>,
}

impl<T> SourceMapCollection<T> {
    /// Create an empty collection
    #[inline]
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Create a collection with capacity
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            items: Vec::with_capacity(capacity),
        }
    }

    /// Add an item
    #[inline]
    pub fn push(&mut self, item: SourceMapped<T>) {
        self.items.push(item);
    }

    /// Add an item with a span
    #[inline]
    pub fn push_with_span(&mut self, value: T, span: SourceSpan) {
        self.items.push(SourceMapped::new(value, span));
    }

    /// Get the number of items
    #[inline]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Check if empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Get an item by index
    #[inline]
    pub fn get(&self, index: usize) -> Option<&SourceMapped<T>> {
        self.items.get(index)
    }

    /// Iterate over items
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &SourceMapped<T>> {
        self.items.iter()
    }

    /// Iterate over values (without spans)
    #[inline]
    pub fn iter_values(&self) -> impl Iterator<Item = &T> {
        self.items.iter().map(|m| m.inner())
    }

    /// Get the combined span of all items
    pub fn combined_span(&self) -> Option<SourceSpan> {
        if self.items.is_empty() {
            return None;
        }

        let first = &self.items[0];
        let mut combined = first.span;

        for item in &self.items[1..] {
            combined = combined.merge(&item.span);
        }

        Some(combined)
    }

    /// Convert to a vector of values (discarding spans)
    #[inline]
    pub fn into_values(self) -> Vec<T> {
        self.items.into_iter().map(|m| m.into_inner()).collect()
    }

    /// Convert to a vector of source-mapped values
    #[inline]
    pub fn into_items(self) -> Vec<SourceMapped<T>> {
        self.items
    }
}

impl<T> IntoIterator for SourceMapCollection<T> {
    type Item = SourceMapped<T>;
    type IntoIter = std::vec::IntoIter<SourceMapped<T>>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl<'a, T> IntoIterator for &'a SourceMapCollection<T> {
    type Item = &'a SourceMapped<T>;
    type IntoIter = std::slice::Iter<'a, SourceMapped<T>>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}

/// Builder for creating source-mapped values
///
/// Useful when building values incrementally.
#[derive(Debug, Clone)]
pub struct SourceMapBuilder {
    /// The source input
    source: String,
}

impl SourceMapBuilder {
    /// Create a new builder with the source input
    #[inline]
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
        }
    }

    /// Create a source-mapped value from an offset and length
    pub fn mapped<T>(&self, value: T, offset: usize, length: usize) -> SourceMapped<T> {
        let span = SourceSpan::from_offsets(&self.source, offset, offset + length);
        SourceMapped::new(value, span)
    }

    /// Create a source-mapped value at a single position
    pub fn at<T>(&self, value: T, offset: usize) -> SourceMapped<T> {
        use super::source_location::SourcePosition;
        let pos = SourcePosition::from_offset(&self.source, offset);
        SourceMapped::at(value, offset, pos.line, pos.column)
    }

    /// Get the source string
    #[inline]
    pub fn source(&self) -> &str {
        &self.source
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_mapped_new() {
        let span = SourceSpan::at(10, 2, 5);
        let mapped = SourceMapped::new(42, span);

        assert_eq!(*mapped, 42);
        assert_eq!(mapped.span(), &span);
    }

    #[test]
    fn test_source_mapped_at() {
        let mapped = SourceMapped::at(42, 10, 2, 5);

        assert_eq!(*mapped, 42);
        assert_eq!(mapped.span().start.offset, 10);
        assert_eq!(mapped.span().start.line, 2);
        assert_eq!(mapped.span().start.column, 5);
    }

    #[test]
    fn test_source_mapped_map() {
        let span = SourceSpan::at(10, 2, 5);
        let mapped = SourceMapped::new(42, span);

        let doubled = mapped.map(|n| n * 2);

        assert_eq!(*doubled, 84);
        assert_eq!(doubled.span(), &span);
    }

    #[test]
    fn test_source_mapped_try_map() {
        let span = SourceSpan::at(10, 2, 5);
        let mapped = SourceMapped::new("42", span);

        let parsed: Result<SourceMapped<i32>, _> = mapped.try_map(|s| s.parse());

        let parsed = parsed.unwrap();
        assert_eq!(*parsed, 42);
        assert_eq!(parsed.span(), &span);
    }

    #[test]
    fn test_source_mapped_combine() {
        let span1 = SourceSpan::at(0, 1, 1);
        let span2 = SourceSpan::at(10, 1, 11);
        let mapped1 = SourceMapped::new(2, span1);
        let mapped2 = SourceMapped::new(3, span2);

        let combined = mapped1.combine(mapped2, |a, b| a * b);

        assert_eq!(*combined, 6);
        assert_eq!(combined.span().start.offset, 0);
        assert_eq!(combined.span().end.offset, 10);
    }

    #[test]
    fn test_source_mapped_deref() {
        let mapped = SourceMapped::new(vec![1, 2, 3], SourceSpan::default());

        assert_eq!(mapped.len(), 3);
        assert_eq!(mapped[0], 1);
    }

    #[test]
    fn test_source_map_collection() {
        let mut collection = SourceMapCollection::new();

        collection.push(SourceMapped::at(1, 0, 1, 1));
        collection.push(SourceMapped::at(2, 5, 1, 6));
        collection.push(SourceMapped::at(3, 10, 1, 11));

        assert_eq!(collection.len(), 3);
        assert!(!collection.is_empty());

        let combined = collection.combined_span().unwrap();
        assert_eq!(combined.start.offset, 0);
        assert_eq!(combined.end.offset, 10);
    }

    #[test]
    fn test_source_map_collection_iter_values() {
        let mut collection = SourceMapCollection::new();
        collection.push(SourceMapped::at(1, 0, 1, 1));
        collection.push(SourceMapped::at(2, 5, 1, 6));

        let values: Vec<_> = collection.iter_values().copied().collect();
        assert_eq!(values, vec![1, 2]);
    }

    #[test]
    fn test_source_map_collection_into_values() {
        let mut collection = SourceMapCollection::new();
        collection.push(SourceMapped::at(1, 0, 1, 1));
        collection.push(SourceMapped::at(2, 5, 1, 6));

        let values = collection.into_values();
        assert_eq!(values, vec![1, 2]);
    }

    #[test]
    fn test_source_map_builder() {
        let input = "hello world";
        let builder = SourceMapBuilder::new(input);

        let mapped = builder.mapped(42, 0, 5);

        assert_eq!(*mapped, 42);
        assert_eq!(mapped.span().start.offset, 0);
        assert_eq!(mapped.span().len(), 5);
    }

    #[test]
    fn test_source_map_builder_at() {
        let input = "hello world";
        let builder = SourceMapBuilder::new(input);

        let mapped = builder.at(42, 6);

        assert_eq!(*mapped, 42);
        assert_eq!(mapped.span().start.line, 1);
        assert_eq!(mapped.span().start.column, 7);
    }

    #[test]
    fn test_source_mapped_into_inner() {
        let mapped = SourceMapped::new(vec![1, 2, 3], SourceSpan::default());
        let inner = mapped.into_inner();

        assert_eq!(inner, vec![1, 2, 3]);
    }

    #[test]
    fn test_source_mapped_with_span() {
        let span1 = SourceSpan::at(0, 1, 1);
        let span2 = SourceSpan::at(10, 2, 5);

        let mapped = SourceMapped::new(42, span1);
        let mapped = mapped.with_span(span2);

        assert_eq!(mapped.span(), &span2);
    }
}
