//! Lexer functions for Ruby FFI

use magnus::{value::ReprValue, Error, IntoValue, RArray, Ruby, TryConvert, Value};

/// Create a cached lexer from token definitions
pub fn create_lexer(definitions: Value) -> Result<Value, Error> {
    let ruby = Ruby::get().unwrap();

    // Convert to Ruby array and iterate
    let ary: RArray = TryConvert::try_convert(definitions)?;
    let len = ary.len();

    let mut token_defs = Vec::new();

    for i in 0..len {
        let def: Value = ary.entry(i as isize)?;
        let hash: magnus::RHash = TryConvert::try_convert(def)?;

        let name: String = match hash.lookup("name")? {
            Some(v) => TryConvert::try_convert(v)?,
            None => return Err(Error::new(ruby.exception_arg_error(), "missing name")),
        };

        let pattern: String = match hash.lookup("pattern")? {
            Some(v) => TryConvert::try_convert(v)?,
            None => return Err(Error::new(ruby.exception_arg_error(), "missing pattern")),
        };

        let priority_val: Value = hash.lookup("priority")?;
        let priority: i32 = if priority_val.is_nil() {
            0
        } else {
            TryConvert::try_convert(priority_val).unwrap_or(0)
        };

        let ignore_val: Value = hash.lookup("ignore")?;
        let ignore: bool = if ignore_val.is_nil() {
            false
        } else {
            TryConvert::try_convert(ignore_val).unwrap_or(false)
        };

        token_defs.push(crate::generic_lexer::TokenDef {
            name,
            pattern,
            priority,
            ignore,
        });
    }

    let lexer_id = crate::generic_lexer::create_lexer(token_defs)
        .map_err(|e| Error::new(ruby.exception_runtime_error(), e))?;

    Ok(lexer_id.into_value_with(&ruby))
}

/// Tokenize input using a cached lexer
pub fn tokenize_with_lexer(lexer_id: usize, input: String) -> Result<Value, Error> {
    let ruby = Ruby::get().unwrap();

    let tokens = crate::generic_lexer::tokenize_with_lexer(lexer_id, &input)
        .map_err(|e| Error::new(ruby.exception_runtime_error(), e))?;

    let ruby_array = ruby.ary_new_capa(tokens.len() as _);

    for token in tokens {
        let hash = ruby.hash_new();
        hash.aset("type", token.token_type)?;
        hash.aset("value", token.value)?;

        let loc_hash = ruby.hash_new();
        loc_hash.aset("line", token.location.line as i64)?;
        loc_hash.aset("column", token.location.column as i64)?;
        loc_hash.aset("offset", token.location.offset as i64)?;
        hash.aset("location", loc_hash)?;

        ruby_array.push(hash)?;
    }

    Ok(ruby_array.into_value_with(&ruby))
}

/// Remove a cached lexer
pub fn drop_lexer(lexer_id: usize) -> Result<Value, Error> {
    let ruby = Ruby::get().unwrap();
    let removed = crate::generic_lexer::drop_lexer(lexer_id);
    Ok(removed.into_value_with(&ruby))
}
