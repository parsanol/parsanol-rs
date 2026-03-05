//! Transformation system for converting parse trees
//!
//! This module provides the `Transform` struct for rule-based transformations
//! and `TransformError` for error handling.

use std::collections::HashMap;
use std::fmt;

use super::{Bindings, HashPatternBuilder, Pattern, Value};

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
