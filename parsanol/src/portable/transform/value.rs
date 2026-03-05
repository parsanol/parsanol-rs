//! Value type for the transformation system
//!
//! This module provides the `Value` enum which represents values in the
//! transformation system, similar to Parslet's transform values.

use std::collections::HashMap;
use std::fmt;

// Re-export FromAstError for TryFrom implementations
use crate::derive::FromAstError;

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

    /// Get a hash field by key (alias for get, returns reference)
    pub fn get_hash_field(&self, key: &str) -> Option<&Value> {
        self.get(key)
    }

    /// Get the tag from a hash value (looks for "tag" key)
    ///
    /// This is used by the derive macro for enum variant matching.
    pub fn get_tag(&self) -> Option<&str> {
        match self {
            Value::Hash(h) => h.get("tag").and_then(|v| v.as_str()),
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

    /// Get the type name of this value for error messages
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "nil",
            Value::Bool(_) => "bool",
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Hash(_) => "hash",
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
// TryFrom implementations for Value conversion
// ============================================================================

impl TryFrom<Value> for i64 {
    type Error = FromAstError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::Int(n) => Ok(n),
            Value::Float(f) => Ok(f as i64),
            _ => Err(FromAstError::TypeMismatch {
                expected: "int",
                actual: value.type_name(),
            }),
        }
    }
}

impl TryFrom<Value> for i32 {
    type Error = FromAstError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        i64::try_from(value).map(|n| n as i32)
    }
}

impl TryFrom<Value> for usize {
    type Error = FromAstError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        i64::try_from(value).and_then(|n| {
            if n >= 0 {
                Ok(n as usize)
            } else {
                Err(FromAstError::ConversionError)
            }
        })
    }
}

impl TryFrom<Value> for f64 {
    type Error = FromAstError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::Float(f) => Ok(f),
            Value::Int(n) => Ok(n as f64),
            _ => Err(FromAstError::TypeMismatch {
                expected: "float",
                actual: value.type_name(),
            }),
        }
    }
}

impl TryFrom<Value> for bool {
    type Error = FromAstError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::Bool(b) => Ok(b),
            _ => Err(FromAstError::TypeMismatch {
                expected: "bool",
                actual: value.type_name(),
            }),
        }
    }
}

impl TryFrom<Value> for String {
    type Error = FromAstError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::String(s) => Ok(s),
            Value::Nil => Ok(String::new()),
            _ => Err(FromAstError::TypeMismatch {
                expected: "string",
                actual: value.type_name(),
            }),
        }
    }
}

impl TryFrom<Value> for () {
    type Error = FromAstError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::Nil => Ok(()),
            _ => Err(FromAstError::TypeMismatch {
                expected: "nil",
                actual: value.type_name(),
            }),
        }
    }
}

impl<T: TryFrom<Value, Error = FromAstError>> TryFrom<Value> for Option<T> {
    type Error = FromAstError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::Nil => Ok(None),
            v => T::try_from(v).map(Some),
        }
    }
}

impl<T: TryFrom<Value, Error = FromAstError>> TryFrom<Value> for Vec<T> {
    type Error = FromAstError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::Array(arr) => arr.into_iter().map(T::try_from).collect(),
            _ => Err(FromAstError::TypeMismatch {
                expected: "array",
                actual: value.type_name(),
            }),
        }
    }
}
