//! RubyBuilder - streaming builder wrapper for Ruby callbacks

use crate::portable::ast::ParseError;
use crate::portable::streaming_builder::{BuildError, BuildResult, StreamingBuilder};
use magnus::{value::ReprValue, IntoValue, Ruby, Value};

/// Ruby callback wrapper for streaming builder
///
/// This wraps a Ruby object that implements the builder callback protocol.
/// The Ruby object should respond to methods like `on_string`, `on_int`, etc.
///
/// # Ruby Interface
///
/// ```ruby
/// class MyBuilder
///   def on_named_start(name); end
///   def on_named_end(name); end
///   def on_string(value, offset, length); end
///   def on_int(value); end
///   def on_float(value); end
///   def on_bool(value); end
///   def on_nil; end
///   def on_array_start(expected_len); end
///   def on_array_element(index); end
///   def on_array_end(actual_len); end
///   def on_hash_start(expected_len); end
///   def on_hash_key(key); end
///   def on_hash_value(key); end
///   def on_hash_end(actual_len); end
///   def on_start(input); end
///   def on_success; end
///   def on_error(message); end
///   def finish; end
/// end
/// ```
pub struct RubyBuilder {
    /// The Ruby object implementing callbacks
    callback: Value,
}

impl RubyBuilder {
    /// Create a new Ruby builder wrapper
    ///
    /// # Arguments
    /// * `callback` - Ruby object with callback methods
    pub fn new(callback: Value) -> Self {
        Self { callback }
    }

    /// Call a method on the Ruby callback object
    fn call_method(&self, method: &str, args: &[Value]) -> BuildResult<()> {
        let _ruby = Ruby::get().map_err(|e| BuildError::Custom {
            message: format!("Ruby not available: {}", e),
        })?;

        if args.is_empty() {
            self.callback
                .funcall::<&str, (), Value>(method, ())
                .map(|_| ())
                .map_err(|e| BuildError::Custom {
                    message: format!("Ruby callback error: {}", e),
                })
        } else {
            self.callback
                .funcall::<&str, &[Value], Value>(method, args)
                .map(|_| ())
                .map_err(|e| BuildError::Custom {
                    message: format!("Ruby callback error: {}", e),
                })
        }
    }
}

impl StreamingBuilder for RubyBuilder {
    type Output = Value;

    fn on_named_start(&mut self, name: &str) -> BuildResult<()> {
        let ruby = Ruby::get().map_err(|e| BuildError::Custom {
            message: format!("Ruby not available: {}", e),
        })?;
        let name_val: Value = ruby.str_new(name).as_value();
        self.call_method("on_named_start", &[name_val])
    }

    fn on_named_end(&mut self, name: &str) -> BuildResult<()> {
        let ruby = Ruby::get().map_err(|e| BuildError::Custom {
            message: format!("Ruby not available: {}", e),
        })?;
        let name_val: Value = ruby.str_new(name).as_value();
        self.call_method("on_named_end", &[name_val])
    }

    fn on_string(&mut self, value: &str, offset: usize, length: usize) -> BuildResult<()> {
        let ruby = Ruby::get().map_err(|e| BuildError::Custom {
            message: format!("Ruby not available: {}", e),
        })?;
        let value_val: Value = ruby.str_new(value).as_value();
        let offset_val: Value = ruby.integer_from_i64(offset as i64).as_value();
        let length_val: Value = ruby.integer_from_i64(length as i64).as_value();
        self.call_method("on_string", &[value_val, offset_val, length_val])
    }

    fn on_int(&mut self, value: i64) -> BuildResult<()> {
        let ruby = Ruby::get().map_err(|e| BuildError::Custom {
            message: format!("Ruby not available: {}", e),
        })?;
        let value_val: Value = ruby.integer_from_i64(value).as_value();
        self.call_method("on_int", &[value_val])
    }

    fn on_float(&mut self, value: f64) -> BuildResult<()> {
        let ruby = Ruby::get().map_err(|e| BuildError::Custom {
            message: format!("Ruby not available: {}", e),
        })?;
        let value_val: Value = ruby.float_from_f64(value).as_value();
        self.call_method("on_float", &[value_val])
    }

    fn on_bool(&mut self, value: bool) -> BuildResult<()> {
        let ruby = Ruby::get().map_err(|e| BuildError::Custom {
            message: format!("Ruby not available: {}", e),
        })?;
        let value_val: Value = value.into_value_with(&ruby);
        self.call_method("on_bool", &[value_val])
    }

    fn on_nil(&mut self) -> BuildResult<()> {
        self.call_method("on_nil", &[])
    }

    fn on_array_start(&mut self, expected_len: Option<usize>) -> BuildResult<()> {
        let ruby = Ruby::get().map_err(|e| BuildError::Custom {
            message: format!("Ruby not available: {}", e),
        })?;
        let len_val: Value = match expected_len {
            Some(n) => ruby.integer_from_i64(n as i64).as_value(),
            None => ruby.qnil().as_value(),
        };
        self.call_method("on_array_start", &[len_val])
    }

    fn on_array_element(&mut self, index: usize) -> BuildResult<()> {
        let ruby = Ruby::get().map_err(|e| BuildError::Custom {
            message: format!("Ruby not available: {}", e),
        })?;
        let index_val: Value = ruby.integer_from_i64(index as i64).as_value();
        self.call_method("on_array_element", &[index_val])
    }

    fn on_array_end(&mut self, actual_len: usize) -> BuildResult<()> {
        let ruby = Ruby::get().map_err(|e| BuildError::Custom {
            message: format!("Ruby not available: {}", e),
        })?;
        let len_val: Value = ruby.integer_from_i64(actual_len as i64).as_value();
        self.call_method("on_array_end", &[len_val])
    }

    fn on_hash_start(&mut self, expected_len: Option<usize>) -> BuildResult<()> {
        let ruby = Ruby::get().map_err(|e| BuildError::Custom {
            message: format!("Ruby not available: {}", e),
        })?;
        let len_val: Value = match expected_len {
            Some(n) => ruby.integer_from_i64(n as i64).as_value(),
            None => ruby.qnil().as_value(),
        };
        self.call_method("on_hash_start", &[len_val])
    }

    fn on_hash_key(&mut self, key: &str) -> BuildResult<()> {
        let ruby = Ruby::get().map_err(|e| BuildError::Custom {
            message: format!("Ruby not available: {}", e),
        })?;
        let key_val: Value = ruby.str_new(key).as_value();
        self.call_method("on_hash_key", &[key_val])
    }

    fn on_hash_value(&mut self, key: &str) -> BuildResult<()> {
        let ruby = Ruby::get().map_err(|e| BuildError::Custom {
            message: format!("Ruby not available: {}", e),
        })?;
        let key_val: Value = ruby.str_new(key).as_value();
        self.call_method("on_hash_value", &[key_val])
    }

    fn on_hash_end(&mut self, actual_len: usize) -> BuildResult<()> {
        let ruby = Ruby::get().map_err(|e| BuildError::Custom {
            message: format!("Ruby not available: {}", e),
        })?;
        let len_val: Value = ruby.integer_from_i64(actual_len as i64).as_value();
        self.call_method("on_hash_end", &[len_val])
    }

    fn on_start(&mut self, input: &str) -> BuildResult<()> {
        let ruby = Ruby::get().map_err(|e| BuildError::Custom {
            message: format!("Ruby not available: {}", e),
        })?;
        let input_val: Value = ruby.str_new(input).as_value();
        self.call_method("on_start", &[input_val])
    }

    fn on_success(&mut self) -> BuildResult<()> {
        self.call_method("on_success", &[])
    }

    fn on_error(&mut self, error: &ParseError) -> BuildResult<()> {
        let ruby = Ruby::get().map_err(|e| BuildError::Custom {
            message: format!("Ruby not available: {}", e),
        })?;
        let msg_val: Value = ruby.str_new(&error.to_string()).as_value();
        self.call_method("on_error", &[msg_val])
    }

    fn finish(&mut self) -> BuildResult<Value> {
        let _ruby = Ruby::get().map_err(|e| BuildError::Custom {
            message: format!("Ruby not available: {}", e),
        })?;

        self.callback
            .funcall::<&str, (), Value>("finish", ())
            .map_err(|e| BuildError::Custom {
                message: format!("Ruby callback error: {}", e),
            })
    }
}
