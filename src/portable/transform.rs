//! Transformation System for Parse Trees
//!
//! This module provides tools for transforming generic parse trees into
//! typed Rust data structures, similar to Parslet's transformation system.
//!
//! # Example
//!
//! ```rust
//! use parsanol::portable::transform::*;
//!
//! // Define transform rules
//! let transform = Transform::new()
//!     .rule("int", |v| {
//!         let n = v.as_int().ok_or_else(|| TransformError::Custom("not an int".to_string()))?;
//!         Ok(Value::int(n * 2))
//!     });
//!
//! // Create a value to transform
//! let value = Value::hash(vec![("int", Value::int(21))]);
//!
//! // Apply transform
//! let result = transform.apply(&value).unwrap();
//! assert_eq!(result.as_int(), Some(42));
//! ```
//!
//! # Pattern Matching
//!
//! The transform system supports pattern matching similar to Parslet:
//!
//! ```rust,ignore
//! let transform = Transform::new()
//!     // Match a hash with specific keys
//!     .pattern(
//!         Pattern::hash()
//!             .field("left", Pattern::simple("l"))
//!             .field("op", Pattern::str("+"))
//!             .field("right", Pattern::simple("r")),
//!         |bindings| {
//!             let l = bindings.get_int("l")?;
//!             let r = bindings.get_int("r")?;
//!             Ok(Value::int(l + r))
//!         }
//!     );
//! ```

use super::arena::AstArena;
use super::ast::AstNode;
use std::collections::HashMap;
use std::fmt;

/// A value in the transformation system
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Value {
    /// Null/nil value
    #[default]
    Nil,
    /// Boolean value
    Bool(bool),
    /// Integer value
    Int(i64),
    /// Float value
    Float(f64),
    /// String value
    String(String),
    /// Array of values
    Array(Vec<Value>),
    /// Hash/object of key-value pairs
    Hash(HashMap<String, Value>),
}

impl Value {
    /// Create a nil value
    pub fn nil() -> Self {
        Value::Nil
    }

    /// Create a boolean value
    pub fn bool(b: bool) -> Self {
        Value::Bool(b)
    }

    /// Create an integer value
    pub fn int(n: i64) -> Self {
        Value::Int(n)
    }

    /// Create a float value
    pub fn float(f: f64) -> Self {
        Value::Float(f)
    }

    /// Create a string value
    pub fn string(s: impl Into<String>) -> Self {
        Value::String(s.into())
    }

    /// Create an array value
    pub fn array(items: Vec<Value>) -> Self {
        Value::Array(items)
    }

    /// Create a hash value
    pub fn hash(pairs: Vec<(impl Into<String>, Value)>) -> Self {
        let mut map = HashMap::new();
        for (k, v) in pairs {
            map.insert(k.into(), v);
        }
        Value::Hash(map)
    }

    /// Check if this is nil
    pub fn is_nil(&self) -> bool {
        matches!(self, Value::Nil)
    }

    /// Get as boolean
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Get as integer
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(n) => Some(*n),
            _ => None,
        }
    }

    /// Get as float
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Value::Float(f) => Some(*f),
            Value::Int(n) => Some(*n as f64),
            _ => None,
        }
    }

    /// Get as string slice
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// Get as string (cloned)
    pub fn to_string(&self) -> Option<String> {
        match self {
            Value::String(s) => Some(s.clone()),
            _ => None,
        }
    }

    /// Get as array
    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(arr) => Some(arr),
            _ => None,
        }
    }

    /// Get as hash
    pub fn as_hash(&self) -> Option<&HashMap<String, Value>> {
        match self {
            Value::Hash(h) => Some(h),
            _ => None,
        }
    }

    /// Get a hash value by key
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Hash(h) => h.get(key),
            _ => None,
        }
    }

    /// Get an array element by index
    pub fn get_index(&self, index: usize) -> Option<&Value> {
        match self {
            Value::Array(arr) => arr.get(index),
            _ => None,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Nil => write!(f, "nil"),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Int(n) => write!(f, "{}", n),
            Value::Float(fl) => write!(f, "{}", fl),
            Value::String(s) => write!(f, "{:?}", s),
            Value::Array(arr) => {
                write!(f, "[")?;
                for (i, v) in arr.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", v)?;
                }
                write!(f, "]")
            }
            Value::Hash(h) => {
                write!(f, "{{")?;
                for (i, (k, v)) in h.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{:?}: {}", k, v)?;
                }
                write!(f, "}}")
            }
        }
    }
}

// ============================================================================
// Pattern Matching System (Parslet-style)
// ============================================================================

/// Pattern for matching values in transformations
#[derive(Debug, Clone)]
pub enum Pattern {
    /// Match any value and bind to variable
    Simple(String),
    /// Match a specific string value
    Str(String),
    /// Match an integer value
    Int(i64),
    /// Match a boolean value
    Bool(bool),
    /// Match nil
    Nil,
    /// Match an array and bind to variable
    Sequence(String),
    /// Match a hash with specific fields
    Hash {
        /// Fields to match (name, pattern)
        fields: Vec<(String, Pattern)>,
        /// Whether to allow extra fields not in the pattern
        allow_extra: bool,
    },
    /// Match anything and bind to variable
    Subtree(String),
    /// Match any of the given patterns
    AnyOf(Vec<Pattern>),
    /// Match all of the given patterns (intersection)
    AllOf(Vec<Pattern>),
}

impl Pattern {
    /// Create a simple pattern that matches any leaf value
    pub fn simple(var: impl Into<String>) -> Self {
        Pattern::Simple(var.into())
    }

    /// Create a pattern that matches a specific string
    pub fn str(s: impl Into<String>) -> Self {
        Pattern::Str(s.into())
    }

    /// Create a pattern that matches an integer
    pub fn int(n: i64) -> Self {
        Pattern::Int(n)
    }

    /// Create a pattern that matches a boolean
    pub fn bool(b: bool) -> Self {
        Pattern::Bool(b)
    }

    /// Create a pattern that matches nil
    pub fn nil() -> Self {
        Pattern::Nil
    }

    /// Create a pattern that matches an array
    pub fn sequence(var: impl Into<String>) -> Self {
        Pattern::Sequence(var.into())
    }

    /// Create a pattern that matches anything
    pub fn subtree(var: impl Into<String>) -> Self {
        Pattern::Subtree(var.into())
    }

    /// Create a hash pattern builder
    pub fn hash() -> HashPatternBuilder {
        HashPatternBuilder::new()
    }

    /// Try to match this pattern against a value
    pub fn match_value(&self, value: &Value) -> Option<Bindings> {
        match self {
            Pattern::Simple(var) => {
                // Simple patterns match leaf values (not arrays or hashes)
                match value {
                    Value::Array(_) | Value::Hash(_) => None,
                    _ => {
                        let mut bindings = Bindings::new();
                        bindings.insert(var.clone(), value.clone());
                        Some(bindings)
                    }
                }
            }
            Pattern::Str(expected) => match value.as_str() {
                Some(s) if s == expected => Some(Bindings::new()),
                _ => None,
            },
            Pattern::Int(expected) => match value.as_int() {
                Some(n) if n == *expected => Some(Bindings::new()),
                _ => None,
            },
            Pattern::Bool(expected) => match value.as_bool() {
                Some(b) if b == *expected => Some(Bindings::new()),
                _ => None,
            },
            Pattern::Nil => {
                if value.is_nil() {
                    Some(Bindings::new())
                } else {
                    None
                }
            }
            Pattern::Sequence(var) => match value.as_array() {
                Some(arr) => {
                    let mut bindings = Bindings::new();
                    bindings.insert(var.clone(), Value::Array(arr.to_vec()));
                    Some(bindings)
                }
                _ => None,
            },
            Pattern::Subtree(var) => {
                let mut bindings = Bindings::new();
                bindings.insert(var.clone(), value.clone());
                Some(bindings)
            }
            Pattern::Hash {
                fields,
                allow_extra,
            } => {
                match value.as_hash() {
                    Some(hash) => {
                        let mut bindings = Bindings::new();
                        for (field_name, field_pattern) in fields {
                            match hash.get(field_name) {
                                Some(field_value) => {
                                    let field_bindings = field_pattern.match_value(field_value)?;
                                    bindings.merge(field_bindings)?;
                                }
                                None => return None,
                            }
                        }
                        // Check if there are extra fields not in the pattern
                        if !*allow_extra {
                            for key in hash.keys() {
                                if !fields.iter().any(|(f, _)| f == key) {
                                    return None;
                                }
                            }
                        }
                        Some(bindings)
                    }
                    _ => None,
                }
            }
            Pattern::AnyOf(patterns) => {
                for pattern in patterns {
                    if let Some(bindings) = pattern.match_value(value) {
                        return Some(bindings);
                    }
                }
                None
            }
            Pattern::AllOf(patterns) => {
                let mut combined = Bindings::new();
                for pattern in patterns {
                    match pattern.match_value(value) {
                        Some(bindings) => combined.merge(bindings)?,
                        None => return None,
                    }
                }
                Some(combined)
            }
        }
    }
}

/// Builder for hash patterns
#[derive(Debug, Clone)]
pub struct HashPatternBuilder {
    fields: Vec<(String, Pattern)>,
    allow_extra: bool,
}

impl HashPatternBuilder {
    /// Create a new hash pattern builder
    pub fn new() -> Self {
        Self {
            fields: Vec::new(),
            allow_extra: true,
        }
    }

    /// Add a field with a simple pattern
    pub fn field(mut self, name: impl Into<String>, var: impl Into<String>) -> Self {
        self.fields.push((name.into(), Pattern::simple(var)));
        self
    }

    /// Add a field with a specific pattern
    pub fn field_pattern(mut self, name: impl Into<String>, pattern: Pattern) -> Self {
        self.fields.push((name.into(), pattern));
        self
    }

    /// Require exact field match (no extra fields allowed)
    pub fn strict(mut self) -> Self {
        self.allow_extra = false;
        self
    }

    /// Build the pattern
    pub fn build(self) -> Pattern {
        Pattern::Hash {
            fields: self.fields,
            allow_extra: self.allow_extra,
        }
    }
}

impl Default for HashPatternBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Variable bindings from pattern matching
#[derive(Debug, Clone)]
pub struct Bindings {
    values: HashMap<String, Value>,
}

impl Bindings {
    /// Create empty bindings
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
        }
    }

    /// Insert a binding
    pub fn insert(&mut self, name: String, value: Value) {
        self.values.insert(name, value);
    }

    /// Get a binding as a Value
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.values.get(name)
    }

    /// Get an integer binding
    pub fn get_int(&self, name: &str) -> Result<i64, TransformError> {
        self.values
            .get(name)
            .and_then(|v| v.as_int())
            .ok_or_else(|| TransformError::MissingField(name.to_string()))
    }

    /// Get a float binding
    pub fn get_float(&self, name: &str) -> Result<f64, TransformError> {
        self.values
            .get(name)
            .and_then(|v| v.as_float())
            .ok_or_else(|| TransformError::MissingField(name.to_string()))
    }

    /// Get a string binding
    pub fn get_string(&self, name: &str) -> Result<&str, TransformError> {
        self.values
            .get(name)
            .and_then(|v| v.as_str())
            .ok_or_else(|| TransformError::MissingField(name.to_string()))
    }

    /// Get a bool binding
    pub fn get_bool(&self, name: &str) -> Result<bool, TransformError> {
        self.values
            .get(name)
            .and_then(|v| v.as_bool())
            .ok_or_else(|| TransformError::MissingField(name.to_string()))
    }

    /// Get an array binding
    pub fn get_array(&self, name: &str) -> Result<&[Value], TransformError> {
        self.values
            .get(name)
            .and_then(|v| v.as_array())
            .ok_or_else(|| TransformError::MissingField(name.to_string()))
    }

    /// Merge bindings, checking for conflicts
    pub fn merge(&mut self, other: Bindings) -> Option<()> {
        for (name, value) in other.values {
            if let Some(existing) = self.values.get(&name) {
                // Check constraint: same variable must have same value
                if existing != &value {
                    return None;
                }
            } else {
                self.values.insert(name, value);
            }
        }
        Some(())
    }
}

impl Default for Bindings {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert an AstNode to a Value
pub fn ast_to_value(node: &AstNode, arena: &AstArena, input: &str) -> Value {
    match node {
        AstNode::Nil => Value::Nil,
        AstNode::Bool(b) => Value::Bool(*b),
        AstNode::Int(n) => Value::Int(*n),
        AstNode::Float(f) => Value::Float(f.to_bits() as f64), // Approximate
        AstNode::StringRef { pool_index } => {
            let s = arena.get_string(*pool_index as usize);
            Value::String(s.to_string())
        }
        AstNode::InputRef { offset, length } => {
            let start = *offset as usize;
            let end = start + *length as usize;
            let s = &input[start..end.min(input.len())];
            Value::String(s.to_string())
        }
        AstNode::Array { pool_index, length } => {
            let items = arena.get_array(*pool_index as usize, *length as usize);
            let values: Vec<Value> = items
                .iter()
                .map(|i| ast_to_value(i, arena, input))
                .collect();
            Value::Array(values)
        }
        AstNode::Hash { pool_index, length } => {
            let pairs = arena.get_hash_items(*pool_index as usize, *length as usize);
            let mut map = HashMap::new();
            for (k, v) in pairs {
                map.insert(k.clone(), ast_to_value(&v, arena, input));
            }
            Value::Hash(map)
        }
    }
}

/// Get the source span for an AST node, if available
///
/// Returns None for leaf nodes without position info (Nil, Bool, Int, Float)
pub fn ast_node_span(node: &AstNode, input: &str) -> Option<super::source_location::SourceSpan> {
    match node {
        AstNode::InputRef { offset, length } => {
            let start = *offset as usize;
            Some(super::source_location::SourceSpan::from_offsets(
                input,
                start,
                start + *length as usize,
            ))
        }
        AstNode::StringRef { pool_index: _ } => {
            // StringRef references arena interned strings, which don't have positions
            // Could be enhanced to track position at intern time
            None
        }
        AstNode::Array {
            pool_index: _,
            length: _,
        } => {
            // Could compute span from children - for now, return None
            None
        }
        AstNode::Hash {
            pool_index: _,
            length: _,
        } => {
            // Could compute span from children - for now, return None
            None
        }
        AstNode::Nil | AstNode::Bool(_) | AstNode::Int(_) | AstNode::Float(_) => None,
    }
}

/// Convert an AstNode to a Value with source span information
///
/// This is useful for tracking original source positions through transformations,
/// enabling features like IDE integration and precise error reporting.
pub fn ast_to_value_with_span(
    node: &AstNode,
    arena: &AstArena,
    input: &str,
) -> super::source_map::SourceMapped<Value> {
    let value = ast_to_value(node, arena, input);
    let span =
        ast_node_span(node, input).unwrap_or_else(|| super::source_location::SourceSpan::start());
    super::source_map::SourceMapped::new(value, span)
}

/// A transformation rule
type TransformFn = Box<dyn Fn(&Value) -> Result<Value, TransformError> + Send + Sync>;

/// A pattern action function type
type PatternAction = Box<dyn Fn(&Bindings) -> Result<Value, TransformError> + Send + Sync>;

/// A pattern-based transformation rule
struct PatternRule {
    pattern: Pattern,
    action: PatternAction,
}

/// Error during transformation
#[derive(Debug, Clone)]
pub enum TransformError {
    /// Rule not found
    RuleNotFound(String),
    /// Type mismatch
    TypeMismatch {
        /// Expected type name
        expected: String,
        /// Actual type name found
        actual: String,
    },
    /// Missing field
    MissingField(String),
    /// Pattern didn't match
    PatternMismatch(String),
    /// Custom error
    Custom(String),
}

impl fmt::Display for TransformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransformError::RuleNotFound(name) => write!(f, "Transform rule not found: {}", name),
            TransformError::TypeMismatch { expected, actual } => {
                write!(f, "Type mismatch: expected {}, got {}", expected, actual)
            }
            TransformError::MissingField(field) => write!(f, "Missing field: {}", field),
            TransformError::PatternMismatch(desc) => write!(f, "Pattern did not match: {}", desc),
            TransformError::Custom(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for TransformError {}

/// A transformation system for converting parse trees
pub struct Transform {
    /// Rules indexed by name/pattern (for simple rule-based transforms)
    rules: HashMap<String, TransformFn>,
    /// Pattern-based rules (for more complex matching)
    pattern_rules: Vec<PatternRule>,
    /// Default transform for unknown patterns
    default: Option<TransformFn>,
    /// Indexed patterns for faster dispatch
    /// Key is the first field name of hash patterns, or "__simple__" for simple patterns
    hash_pattern_index: HashMap<String, Vec<usize>>,
    /// Index of non-hash patterns (simple, sequence, subtree, etc.)
    non_hash_patterns: Vec<usize>,
}

impl Transform {
    /// Create a new empty transform
    pub fn new() -> Self {
        Self {
            rules: HashMap::new(),
            pattern_rules: Vec::new(),
            default: None,
            hash_pattern_index: HashMap::new(),
            non_hash_patterns: Vec::new(),
        }
    }

    /// Add a transformation rule (simple key-based)
    pub fn rule<F>(mut self, name: &str, f: F) -> Self
    where
        F: Fn(&Value) -> Result<Value, TransformError> + Send + Sync + 'static,
    {
        self.rules.insert(name.to_string(), Box::new(f));
        self
    }

    /// Add a pattern-based transformation rule
    pub fn pattern<F>(mut self, pattern: Pattern, f: F) -> Self
    where
        F: Fn(&Bindings) -> Result<Value, TransformError> + Send + Sync + 'static,
    {
        let idx = self.pattern_rules.len();
        self.pattern_rules.push(PatternRule {
            pattern: pattern.clone(),
            action: Box::new(f),
        });

        // Index the pattern for faster dispatch
        self.index_pattern(idx, &pattern);
        self
    }

    /// Index a pattern for faster lookup
    fn index_pattern(&mut self, idx: usize, pattern: &Pattern) {
        match pattern {
            Pattern::Hash { fields, .. } => {
                // Index by first field name
                if let Some((first_field, _)) = fields.first() {
                    self.hash_pattern_index
                        .entry(first_field.clone())
                        .or_default()
                        .push(idx);
                } else {
                    // Empty hash pattern - add to non-hash
                    self.non_hash_patterns.push(idx);
                }
            }
            _ => {
                // All other patterns go to non-hash index
                self.non_hash_patterns.push(idx);
            }
        }
    }

    /// Add a hash pattern rule with a builder pattern
    pub fn hash_rule<F>(self, builder: HashPatternBuilder, f: F) -> Self
    where
        F: Fn(&Bindings) -> Result<Value, TransformError> + Send + Sync + 'static,
    {
        self.pattern(builder.build(), f)
    }

    /// Set the default transform for unknown patterns
    pub fn default_rule<F>(mut self, f: F) -> Self
    where
        F: Fn(&Value) -> Result<Value, TransformError> + Send + Sync + 'static,
    {
        self.default = Some(Box::new(f));
        self
    }

    /// Apply the transform to a value
    pub fn apply(&self, value: &Value) -> Result<Value, TransformError> {
        // Use indexed pattern matching for faster dispatch
        match value {
            Value::Hash(h) => {
                // Try hash-specific patterns first (indexed by first field name)
                if let Some(first_key) = h.keys().next() {
                    if let Some(indices) = self.hash_pattern_index.get(first_key) {
                        for &idx in indices {
                            let rule = &self.pattern_rules[idx];
                            if let Some(bindings) = rule.pattern.match_value(value) {
                                return (rule.action)(&bindings);
                            }
                        }
                    }
                }

                // Try non-hash patterns (simple, subtree, any_of, etc.)
                for &idx in &self.non_hash_patterns {
                    let rule = &self.pattern_rules[idx];
                    if let Some(bindings) = rule.pattern.match_value(value) {
                        return (rule.action)(&bindings);
                    }
                }

                // Check if this hash has a recognizable pattern
                // For single-key hashes, use the key as the rule name
                if h.len() == 1 {
                    // SAFETY: We checked h.len() == 1, so there's exactly one element
                    let (key, inner) = h.iter().next().expect("hash with len==1 must have element");
                    if let Some(rule) = self.rules.get(key) {
                        // First transform the inner value
                        let transformed_inner = self.apply(inner)?;
                        // Then apply the rule
                        return rule(&transformed_inner);
                    }
                }

                // Recursively transform hash values
                let mut result = HashMap::new();
                for (k, v) in h {
                    result.insert(k.clone(), self.apply(v)?);
                }
                Ok(Value::Hash(result))
            }
            Value::Array(arr) => {
                // Try non-hash patterns for arrays
                for &idx in &self.non_hash_patterns {
                    let rule = &self.pattern_rules[idx];
                    if let Some(bindings) = rule.pattern.match_value(value) {
                        return (rule.action)(&bindings);
                    }
                }

                // Recursively transform array elements
                let result: Result<Vec<Value>, TransformError> =
                    arr.iter().map(|v| self.apply(v)).collect();
                Ok(Value::Array(result?))
            }
            _ => {
                // For leaf values, try non-hash patterns first
                for &idx in &self.non_hash_patterns {
                    let rule = &self.pattern_rules[idx];
                    if let Some(bindings) = rule.pattern.match_value(value) {
                        return (rule.action)(&bindings);
                    }
                }

                // Try default transform or return as-is
                if let Some(default) = &self.default {
                    default(value)
                } else {
                    Ok(value.clone())
                }
            }
        }
    }

    /// Check if a rule exists
    pub fn has_rule(&self, name: &str) -> bool {
        self.rules.contains_key(name)
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::new()
    }
}

/// Typed transform for converting to specific types
#[allow(clippy::type_complexity)]
pub struct TypedTransform<T> {
    /// The transformation function
    transform: Box<dyn Fn(&Value) -> Result<T, TransformError> + Send + Sync>,
}

impl<T: 'static> TypedTransform<T> {
    /// Create a new typed transform
    pub fn new<F>(f: F) -> Self
    where
        F: Fn(&Value) -> Result<T, TransformError> + Send + Sync + 'static,
    {
        Self {
            transform: Box::new(f),
        }
    }

    /// Apply the transform
    pub fn apply(&self, value: &Value) -> Result<T, TransformError> {
        (self.transform)(value)
    }
}

/// Extract an integer from a value
pub fn extract_int(value: &Value) -> Result<i64, TransformError> {
    value.as_int().ok_or_else(|| TransformError::TypeMismatch {
        expected: "int".to_string(),
        actual: format!("{:?}", value),
    })
}

/// Extract a string from a value
pub fn extract_string(value: &Value) -> Result<String, TransformError> {
    value
        .to_string()
        .ok_or_else(|| TransformError::TypeMismatch {
            expected: "string".to_string(),
            actual: format!("{:?}", value),
        })
}

/// Extract a hash field
pub fn extract_field<'a>(value: &'a Value, field: &str) -> Result<&'a Value, TransformError> {
    value
        .get(field)
        .ok_or_else(|| TransformError::MissingField(field.to_string()))
}

// ============================================================================
// Zero-Copy Direct Transform (Item 2.3)
// ============================================================================

/// Trait for directly transforming AstNode to a typed value without intermediate allocation.
///
/// This provides a zero-copy alternative to the standard Transform system.
/// Instead of: `AstNode -> Value -> YourType`, this goes directly: `AstNode -> YourType`.
///
/// # Example
///
/// ```rust
/// use parsanol::portable::{AstNode, AstArena, DirectTransform, TransformError};
///
/// // Define your type
/// #[derive(Debug, PartialEq)]
/// struct Number(i64);
///
/// // Implement DirectTransform
/// impl DirectTransform for Number {
///     fn from_ast(node: &AstNode, _arena: &AstArena, input: &str) -> Result<Self, TransformError> {
///         match node {
///             AstNode::Int(n) => Ok(Number(*n)),
///             AstNode::InputRef { offset, length } => {
///                 let s = &input[*offset as usize..(*offset + *length) as usize];
///                 s.parse().map(Number).map_err(|_| TransformError::Custom("not a number".into()))
///             }
///             _ => Err(TransformError::TypeMismatch { expected: "number".to_string(), actual: "other".to_string() })
///         }
///     }
/// }
/// ```
pub trait DirectTransform: Sized {
    /// Transform an AstNode directly to this type.
    ///
    /// # Arguments
    /// * `node` - The AST node to transform
    /// * `arena` - The arena containing string/array/hash data
    /// * `input` - The original input string (for InputRef nodes)
    ///
    /// # Returns
    /// The transformed value or an error
    fn from_ast(node: &AstNode, arena: &AstArena, input: &str) -> Result<Self, TransformError>;
}

/// Helper functions for implementing DirectTransform
pub mod direct_helpers {
    use super::{AstArena, AstNode, TransformError};

    /// Extract a string from an AstNode
    #[inline]
    pub fn extract_string<'a>(
        node: &AstNode,
        arena: &'a AstArena,
        input: &'a str,
    ) -> Result<&'a str, TransformError> {
        match node {
            AstNode::StringRef { pool_index } => Ok(arena.get_string(*pool_index as usize)),
            AstNode::InputRef { offset, length } => {
                let start = *offset as usize;
                let end = start + (*length as usize);
                if end <= input.len() {
                    Ok(&input[start..end])
                } else {
                    Err(TransformError::Custom("InputRef out of bounds".into()))
                }
            }
            _ => Err(TransformError::TypeMismatch {
                expected: "string".into(),
                actual: "other".into(),
            }),
        }
    }

    /// Extract an integer from an AstNode
    #[inline]
    pub fn extract_int(node: &AstNode) -> Result<i64, TransformError> {
        match node {
            AstNode::Int(n) => Ok(*n),
            _ => Err(TransformError::TypeMismatch {
                expected: "int".into(),
                actual: "other".into(),
            }),
        }
    }

    /// Extract a float from an AstNode
    #[inline]
    pub fn extract_float(node: &AstNode) -> Result<f64, TransformError> {
        match node {
            AstNode::Float(f) => Ok(*f),
            AstNode::Int(n) => Ok(*n as f64),
            _ => Err(TransformError::TypeMismatch {
                expected: "float".into(),
                actual: "other".into(),
            }),
        }
    }

    /// Extract a boolean from an AstNode
    #[inline]
    pub fn extract_bool(node: &AstNode) -> Result<bool, TransformError> {
        match node {
            AstNode::Bool(b) => Ok(*b),
            _ => Err(TransformError::TypeMismatch {
                expected: "bool".into(),
                actual: "other".into(),
            }),
        }
    }

    /// Extract an array from an AstNode
    #[inline]
    pub fn extract_array(node: &AstNode, arena: &AstArena) -> Result<Vec<AstNode>, TransformError> {
        match node {
            AstNode::Array { pool_index, length } => {
                Ok(arena.get_array(*pool_index as usize, *length as usize))
            }
            _ => Err(TransformError::TypeMismatch {
                expected: "array".into(),
                actual: "other".into(),
            }),
        }
    }

    /// Extract a hash field from an AstNode
    #[inline]
    pub fn extract_hash_field(
        node: &AstNode,
        arena: &AstArena,
        _input: &str,
        field: &str,
    ) -> Result<AstNode, TransformError> {
        match node {
            AstNode::Hash { pool_index, length } => {
                let pairs = arena.get_hash_items(*pool_index as usize, *length as usize);
                for (key, value) in pairs {
                    if key == field {
                        return Ok(value);
                    }
                }
                Err(TransformError::MissingField(field.to_string()))
            }
            _ => Err(TransformError::TypeMismatch {
                expected: "hash".into(),
                actual: "other".into(),
            }),
        }
    }

    /// Extract and transform a hash field using DirectTransform
    #[inline]
    pub fn extract_field_as<T: super::DirectTransform>(
        node: &AstNode,
        arena: &AstArena,
        input: &str,
        field: &str,
    ) -> Result<T, TransformError> {
        let field_node = extract_hash_field(node, arena, input, field)?;
        T::from_ast(&field_node, arena, input)
    }

    /// Transform an array of AstNodes to a Vec<T>
    #[inline]
    pub fn transform_array<T: super::DirectTransform>(
        node: &AstNode,
        arena: &AstArena,
        input: &str,
    ) -> Result<Vec<T>, TransformError> {
        let items = extract_array(node, arena)?;
        items
            .iter()
            .map(|item| T::from_ast(item, arena, input))
            .collect()
    }
}

// Implement DirectTransform for common types

impl DirectTransform for i64 {
    fn from_ast(node: &AstNode, _arena: &AstArena, _input: &str) -> Result<Self, TransformError> {
        direct_helpers::extract_int(node)
    }
}

impl DirectTransform for f64 {
    fn from_ast(node: &AstNode, _arena: &AstArena, _input: &str) -> Result<Self, TransformError> {
        direct_helpers::extract_float(node)
    }
}

impl DirectTransform for bool {
    fn from_ast(node: &AstNode, _arena: &AstArena, _input: &str) -> Result<Self, TransformError> {
        direct_helpers::extract_bool(node)
    }
}

impl DirectTransform for String {
    fn from_ast(node: &AstNode, arena: &AstArena, input: &str) -> Result<Self, TransformError> {
        direct_helpers::extract_string(node, arena, input).map(|s| s.to_string())
    }
}

impl<T: DirectTransform> DirectTransform for Option<T> {
    fn from_ast(node: &AstNode, arena: &AstArena, input: &str) -> Result<Self, TransformError> {
        match node {
            AstNode::Nil => Ok(None),
            _ => T::from_ast(node, arena, input).map(Some),
        }
    }
}

impl<T: DirectTransform> DirectTransform for Vec<T> {
    fn from_ast(node: &AstNode, arena: &AstArena, input: &str) -> Result<Self, TransformError> {
        direct_helpers::transform_array(node, arena, input)
    }
}

// ============================================================================
// Pattern Macro
// ============================================================================

/// Declarative pattern matching macro for transforms
///
/// This macro provides a concise syntax for creating patterns similar to Parslet's
/// transformation DSL.
///
/// # Syntax
///
/// - `simple(name)` - Match any leaf value and bind to `name`
/// - `sequence(name)` - Match an array and bind to `name`
/// - `subtree(name)` - Match anything and bind to `name`
/// - `str("value")` - Match a specific string
/// - `int(42)` - Match a specific integer
/// - `bool(true)` - Match a specific boolean
/// - `nil` - Match nil
/// - `{ field1: pattern1, field2: pattern2 }` - Match a hash with fields
///
/// # Example
///
/// ```rust,ignore
/// use parsanol::portable::transform::{Pattern, pattern};
///
/// // Simple pattern
/// let p = pattern!(simple(x));
///
/// // Hash pattern with simple fields
/// let p = pattern!({
///     name: simple(n),
///     value: int(42)
/// });
///
/// // Nested pattern
/// let p = pattern!({
///     left: subtree(l),
///     op: str("+"),
///     right: subtree(r)
/// });
/// ```
#[macro_export]
macro_rules! pattern {
    // Simple pattern
    (simple($var:ident)) => {
        $crate::portable::transform::Pattern::simple(stringify!($var))
    };

    // Sequence pattern
    (sequence($var:ident)) => {
        $crate::portable::transform::Pattern::sequence(stringify!($var))
    };

    // Subtree pattern
    (subtree($var:ident)) => {
        $crate::portable::transform::Pattern::subtree(stringify!($var))
    };

    // String literal pattern
    (str($s:literal)) => {
        $crate::portable::transform::Pattern::str($s)
    };

    // Integer pattern
    (int($n:literal)) => {
        $crate::portable::transform::Pattern::int($n)
    };

    // Boolean pattern
    (bool($b:literal)) => {
        $crate::portable::transform::Pattern::bool($b)
    };

    // Nil pattern
    (nil) => {
        $crate::portable::transform::Pattern::nil()
    };

    // Hash pattern with fields
    ({ $($field:ident : $pattern:tt),* $(,)? }) => {{
        let mut builder = $crate::portable::transform::Pattern::hash();
        $(
            builder = builder.field_pattern(stringify!($field), pattern!($pattern));
        )*
        builder.build()
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_creation() {
        let v = Value::int(42);
        assert_eq!(v.as_int(), Some(42));

        let v = Value::string("hello");
        assert_eq!(v.as_str(), Some("hello"));

        let v = Value::array(vec![Value::int(1), Value::int(2)]);
        assert_eq!(v.as_array().map(|a| a.len()), Some(2));
    }

    #[test]
    fn test_value_hash() {
        let v = Value::hash(vec![
            ("name", Value::string("test")),
            ("value", Value::int(42)),
        ]);

        assert_eq!(v.get("name").and_then(|v| v.as_str()), Some("test"));
        assert_eq!(v.get("value").and_then(|v| v.as_int()), Some(42));
    }

    #[test]
    fn test_transform_identity() {
        let transform = Transform::new();
        let value = Value::int(42);
        let result = transform.apply(&value).unwrap();
        assert_eq!(result.as_int(), Some(42));
    }

    #[test]
    fn test_transform_rule() {
        let transform = Transform::new().rule("int", |v| {
            let n = v
                .as_int()
                .ok_or_else(|| TransformError::Custom("not an int".to_string()))?;
            Ok(Value::int(n * 2))
        });

        let value = Value::hash(vec![("int", Value::int(21))]);
        let result = transform.apply(&value).unwrap();
        assert_eq!(result.as_int(), Some(42));
    }

    #[test]
    fn test_extract_helpers() {
        let value = Value::hash(vec![("x", Value::int(10)), ("y", Value::string("test"))]);

        let x = extract_field(&value, "x").unwrap();
        assert_eq!(extract_int(x).unwrap(), 10);

        let y = extract_field(&value, "y").unwrap();
        assert_eq!(extract_string(y).unwrap(), "test");
    }

    // ========================================================================
    // DirectTransform Tests
    // ========================================================================

    #[test]
    fn test_direct_transform_int() {
        let arena = AstArena::new();
        let node = AstNode::Int(42);

        let result: i64 = DirectTransform::from_ast(&node, &arena, "").unwrap();
        assert_eq!(result, 42);
    }

    #[test]
    fn test_direct_transform_float() {
        let arena = AstArena::new();
        let node = AstNode::Float(3.14);

        let result: f64 = DirectTransform::from_ast(&node, &arena, "").unwrap();
        assert!((result - 3.14).abs() < 0.001);
    }

    #[test]
    fn test_direct_transform_bool() {
        let arena = AstArena::new();
        let node = AstNode::Bool(true);

        let result: bool = DirectTransform::from_ast(&node, &arena, "").unwrap();
        assert!(result);
    }

    #[test]
    fn test_direct_transform_option() {
        let arena = AstArena::new();

        // Test None case
        let nil_node = AstNode::Nil;
        let result: Option<i64> = DirectTransform::from_ast(&nil_node, &arena, "").unwrap();
        assert!(result.is_none());

        // Test Some case
        let int_node = AstNode::Int(42);
        let result: Option<i64> = DirectTransform::from_ast(&int_node, &arena, "").unwrap();
        assert_eq!(result, Some(42));
    }

    #[test]
    fn test_direct_transform_vec() {
        let mut arena = AstArena::new();

        // Create an array node
        let items = vec![AstNode::Int(1), AstNode::Int(2), AstNode::Int(3)];
        let (start, len) = arena.store_array(&items);
        let node = AstNode::Array {
            pool_index: start,
            length: len,
        };

        let result: Vec<i64> = DirectTransform::from_ast(&node, &arena, "").unwrap();
        assert_eq!(result, vec![1, 2, 3]);
    }

    #[test]
    fn test_direct_helpers_extract_string() {
        let mut arena = AstArena::new();

        // Test with StringRef
        let node = arena.intern_string("hello");
        let result = direct_helpers::extract_string(&node, &arena, "").unwrap();
        assert_eq!(result, "hello");

        // Test with InputRef
        let input = "world";
        let node = arena.input_ref(0, 5);
        let result = direct_helpers::extract_string(&node, &arena, input).unwrap();
        assert_eq!(result, "world");
    }

    #[test]
    fn test_direct_helpers_extract_hash_field() {
        let mut arena = AstArena::new();

        let pairs: Vec<(&str, AstNode)> =
            vec![("name", AstNode::Int(42)), ("value", AstNode::Int(100))];
        let (start, len) = arena.store_hash(&pairs);
        let node = AstNode::Hash {
            pool_index: start,
            length: len,
        };

        let field = direct_helpers::extract_hash_field(&node, &arena, "", "name").unwrap();
        assert_eq!(direct_helpers::extract_int(&field).unwrap(), 42);
    }

    #[test]
    fn test_ast_node_span_input_ref() {
        let input = "hello world";

        // Test InputRef at offset 0
        let node = AstNode::InputRef {
            offset: 0,
            length: 5,
        };
        let span = ast_node_span(&node, input).unwrap();
        assert_eq!(span.start.offset, 0);
        assert_eq!(span.end.offset, 5);
        assert_eq!(span.len(), 5);

        // Test InputRef at offset 6
        let node = AstNode::InputRef {
            offset: 6,
            length: 5,
        };
        let span = ast_node_span(&node, input).unwrap();
        assert_eq!(span.start.offset, 6);
        assert_eq!(span.end.offset, 11);
    }

    #[test]
    fn test_ast_node_span_no_position() {
        // Leaf nodes without position info should return None
        assert!(ast_node_span(&AstNode::Nil, "").is_none());
        assert!(ast_node_span(&AstNode::Bool(true), "").is_none());
        assert!(ast_node_span(&AstNode::Int(42), "").is_none());
        assert!(ast_node_span(&AstNode::Float(3.14), "").is_none());
    }

    #[test]
    fn test_ast_to_value_with_span() {
        let input = "hello world";
        let arena = AstArena::new();

        // Test with InputRef
        let node = AstNode::InputRef {
            offset: 0,
            length: 5,
        };
        let mapped = ast_to_value_with_span(&node, &arena, input);

        // Check the value
        assert_eq!(mapped.inner().as_str(), Some("hello"));

        // Check the span
        assert_eq!(mapped.span().start.offset, 0);
        assert_eq!(mapped.span().end.offset, 5);
    }

    #[test]
    fn test_ast_to_value_with_span_int() {
        let arena = AstArena::new();

        // Test with Int node (no span info)
        let node = AstNode::Int(42);
        let mapped = ast_to_value_with_span(&node, &arena, "");

        // Check the value
        assert_eq!(mapped.inner().as_int(), Some(42));

        // Check the span (should be default start span)
        assert_eq!(mapped.span().start.offset, 0);
        assert_eq!(mapped.span().is_empty(), true);
    }

    #[test]
    fn test_source_mapped_combine() {
        use crate::portable::source_location::SourceSpan;
        use crate::portable::source_map::SourceMapped;

        let input = "hello world";
        let span1 = SourceSpan::from_offsets(input, 0, 5);
        let span2 = SourceSpan::from_offsets(input, 6, 11);

        let mapped1 = SourceMapped::new("hello", span1);
        let mapped2 = SourceMapped::new("world", span2);

        // Combine should merge spans
        let combined = mapped1.combine(mapped2, |a, b| format!("{} {}", a, b));

        assert_eq!(*combined, "hello world");
        assert_eq!(combined.span().start.offset, 0);
        assert_eq!(combined.span().end.offset, 11);
    }
}
