//! Ruby module initialization

use magnus::{function, Error, Module, Ruby};

use super::lexer::{create_lexer, drop_lexer, tokenize_with_lexer};
use super::parser::{is_available, parse_batch, parse_to_ruby_objects, parse_with_builder};

/// Initialize the Ruby native extension module
#[magnus::init]
pub fn init(ruby: &Ruby) -> Result<(), Error> {
    let module = ruby.define_module("Parsanol")?;
    let native_module = module.define_module("Native")?;

    native_module.define_module_function("is_available", function!(is_available, 0))?;
    native_module.define_module_function("parse_batch", function!(parse_batch, 2))?;
    native_module.define_module_function("parse_to_ruby_objects", function!(parse_to_ruby_objects, 2))?;
    native_module.define_module_function("parse_with_builder", function!(parse_with_builder, 3))?;
    native_module.define_module_function("create_lexer", function!(create_lexer, 1))?;
    native_module.define_module_function("tokenize_with_lexer", function!(tokenize_with_lexer, 2))?;
    native_module.define_module_function("drop_lexer", function!(drop_lexer, 1))?;

    Ok(())
}
