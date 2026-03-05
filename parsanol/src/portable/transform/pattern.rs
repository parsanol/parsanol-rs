//! Pattern matching system for transformations
//!
//! This module provides pattern matching capabilities similar to Parslet's
//! transformation DSL, including the `Pattern` enum, `HashPatternBuilder`,
//! and `Bindings` for capturing matched values.

use std::collections::HashMap;

use super::{TransformError, Value};

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
