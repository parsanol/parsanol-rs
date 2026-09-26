//! C ABI (F7 mapping): the shaped parse result and the structured failure
//! wire exposed as C strings, over the same portable engine the artifact
//! APIs use. Callers own returned pointers and release them with
//! `parsanol_pg_free`.
//!
//!   char* parsanol_pg_parse(const char* grammar_json, const char* input);
//!       -> shaped-tree JSON, or NULL on failure (see `parsanol_pg_error`)
//!   char* parsanol_pg_error(const char* grammar_json, const char* input);
//!       -> {"offset":N,"ranked":[[pos,[labels]]...]} or NULL on success
//!   void  parsanol_pg_free(char* ptr);

use std::ffi::{c_char, CStr, CString};

use crate::portable::{AstArena, Grammar, PortableParser};

enum Outcome {
    Shape(String),
    Failed { offset: usize, ranked: Vec<(usize, Vec<String>)> },
}

fn execute(grammar_json: &str, input: &str) -> Result<Outcome, String> {
    let grammar: Grammar =
        Grammar::from_json(grammar_json).map_err(|e| e.to_string())?;
    let mut arena = AstArena::for_input(input.len().max(1 << 12));
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    match parser.parse() {
        Ok(raw) => {
            let shaped =
                crate::portable::parslet_transform::to_parslet_compatible(&raw, &mut arena, input);
            let value = crate::pg::suite::ast_to_value(&shaped, &arena, input);
            serde_json::to_string(&value)
                .map(Outcome::Shape)
                .map_err(|e| e.to_string())
        }
        Err(_) => {
            let offset = parser.failure_wire().map_or(0, |(o, _)| o);
            let ranked = parser
                .failure_ranks()
                .iter()
                .map(|(p, labels)| (*p, labels.clone()))
                .collect();
            Ok(Outcome::Failed { offset, ranked })
        }
    }
}

fn run(grammar_json: *const c_char, input: *const c_char) -> Result<Outcome, String> {
    let grammar_json = unsafe { CStr::from_ptr(grammar_json) }
        .to_str()
        .map_err(|e| e.to_string())?;
    let input = unsafe { CStr::from_ptr(input) }.to_str().map_err(|e| e.to_string())?;
    execute(grammar_json, input)
}

fn to_c(string: String) -> *mut c_char {
    CString::new(string).map(|s| s.into_raw()).unwrap_or_else(|_| std::ptr::null_mut())
}

/// # Safety
/// Arguments must be valid C strings. Returns shaped-tree JSON (owned;
/// free with `parsanol_pg_free`) or NULL when parsing fails.
#[no_mangle]
pub unsafe extern "C" fn parsanol_pg_parse(grammar_json: *const c_char, input: *const c_char) -> *mut c_char {
    match run(grammar_json, input) {
        Ok(Outcome::Shape(json)) => to_c(json),
        _ => std::ptr::null_mut(),
    }
}

/// # Safety
/// Arguments must be valid C strings. Returns the failure-wire JSON
/// (owned; free with `parsanol_pg_free`) or NULL when parsing succeeds.
#[no_mangle]
pub unsafe extern "C" fn parsanol_pg_error(grammar_json: *const c_char, input: *const c_char) -> *mut c_char {
    match run(grammar_json, input) {
        Ok(Outcome::Failed { offset, ranked }) => to_c(
            serde_json::json!({ "offset": offset, "ranked": ranked }).to_string(),
        ),
        _ => std::ptr::null_mut(),
    }
}

/// # Safety
/// Frees a pointer returned by `parsanol_pg_parse` / `parsanol_pg_error`.
#[no_mangle]
pub unsafe extern "C" fn parsanol_pg_free(ptr: *mut c_char) {
    if !ptr.is_null() {
        drop(CString::from_raw(ptr));
    }
}
