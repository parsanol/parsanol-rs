//! RubyObject trait for converting Rust types to Ruby objects

use magnus::{Error, Ruby, Value};

/// Trait for types that can be converted to Ruby objects
///
/// This trait enables direct Ruby object construction from Rust types,
/// providing the fastest possible FFI path (Native).
///
/// # Implementation
///
/// # Example (Manual Implementation)
///
/// ```rust,ignore
/// use parsanol::ruby_ffi::RubyObject;
/// use magnus::{Ruby, Value, Error, RClass, RObject};
///
/// pub struct Number(i64);
///
/// impl RubyObject for Number {
///     fn to_ruby(&self, ruby: &Ruby) -> Result<Value, Error> {
///         let class: RClass = ruby.class("Calculator::Number")?;
///         class.new_instance((self.0,))
///     }
/// }
/// ```
///
/// # Example (Derive Macro)
///
/// ```rust,ignore
/// use parsanol_ruby_derive::RubyObject;
///
/// #[derive(RubyObject)]
/// #[ruby_class("Calculator::Number")]
/// pub struct Number(i64);
/// ```
pub trait RubyObject: Sized {
    /// Convert this Rust value to a Ruby object
    ///
    /// # Arguments
    ///
    /// * `ruby` - The Ruby interpreter handle
    ///
    /// # Returns
    ///
    /// A Ruby Value representing this object, or an error.
    fn to_ruby(&self, ruby: &Ruby) -> Result<Value, Error>;
}

// Implement RubyObject for primitive types
impl RubyObject for i64 {
    fn to_ruby(&self, ruby: &Ruby) -> Result<Value, Error> {
        Ok(ruby.integer_from_i64(*self).as_value())
    }
}

impl RubyObject for i32 {
    fn to_ruby(&self, ruby: &Ruby) -> Result<Value, Error> {
        Ok(ruby.integer_from_i64(*self as i64).as_value())
    }
}

impl RubyObject for f64 {
    fn to_ruby(&self, ruby: &Ruby) -> Result<Value, Error> {
        Ok(ruby.float_from_f64(*self).as_value())
    }
}

impl RubyObject for bool {
    fn to_ruby(&self, ruby: &Ruby) -> Result<Value, Error> {
        Ok((*self).into_value_with(ruby))
    }
}

impl RubyObject for String {
    fn to_ruby(&self, ruby: &Ruby) -> Result<Value, Error> {
        Ok(ruby.str_new(self).as_value())
    }
}

impl RubyObject for &str {
    fn to_ruby(&self, ruby: &Ruby) -> Result<Value, Error> {
        Ok(ruby.str_new(self).as_value())
    }
}

impl<T: RubyObject> RubyObject for Option<T> {
    fn to_ruby(&self, ruby: &Ruby) -> Result<Value, Error> {
        match self {
            Some(v) => v.to_ruby(ruby),
            None => {
                // Return Ruby nil
                let nil_val: Value = ruby.eval("nil")?;
                Ok(nil_val)
            }
        }
    }
}

impl<T: RubyObject> RubyObject for Vec<T> {
    fn to_ruby(&self, ruby: &Ruby) -> Result<Value, Error> {
        let ary = ruby.ary_new_capa(self.len() as _);
        for item in self {
            ary.push(item.to_ruby(ruby)?)?;
        }
        Ok(ary.as_value())
    }
}
