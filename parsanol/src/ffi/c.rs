//! C ABI for Parsanol
//!
//! This module provides a stable C ABI for using parsanol from other languages
//! like Python, C++, or any language that can call C functions.
//!
//! # Overview
//!
//! The C ABI provides opaque handles to grammars and parsers, along with
//! functions for creating, using, and destroying them.
//!
//! # Example (C)
//!
//! ```c
//! #include <parsanol.h>
//!
//! int main() {
//!     // Create a grammar from JSON
//!     const char* json = "{\"root\": 0, \"atoms\": [{\"Str\": \"hello\"}]}";
//!     ParsanolGrammar* grammar = parsanol_grammar_new(json);
//!
//!     // Parse some input
//!     const char* input = "hello world";
//!     char* output = NULL;
//!     int result = parsanol_parse(grammar, input, &output);
//!
//!     if (result == 0 && output != NULL) {
//!         printf("Parse result: %s\n", output);
//!         parsanol_string_free(output);
//!     }
//!
//!     // Clean up
//!     parsanol_grammar_free(grammar);
//!     return 0;
//! }
//! ```
//!
//! # Thread Safety
//!
//! All functions in this module are thread-safe. Different threads can
//! safely use different grammars simultaneously.
//!
//! # Memory Management
//!
//! The caller is responsible for freeing all resources returned by the API:
//! - Use `parsanol_grammar_free()` to free grammars
//! - Use `parsanol_string_free()` to free strings
//! - Use `parsanol_result_free()` to free parse results

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_ulong};
use std::ptr;

use crate::portable::Grammar;

// ============================================================================
// Opaque Types
// ============================================================================

/// Opaque handle to a grammar
pub struct ParsanolGrammar {
    grammar: Grammar,
}

/// Opaque handle to a parse result
pub struct ParsanolResult {
    success: bool,
    end_pos: usize,
    error_message: Option<CString>,
    ast_json: Option<CString>,
}

// ============================================================================
// Error Codes
// ============================================================================

/// Success
pub const PARSANOL_OK: c_int = 0;
/// Null pointer passed
pub const PARSANOL_ERROR_NULL_POINTER: c_int = -1;
/// Invalid JSON
pub const PARSANOL_ERROR_INVALID_JSON: c_int = -2;
/// Parse failed
pub const PARSANOL_ERROR_PARSE_FAILED: c_int = -3;
/// Out of memory
pub const PARSANOL_ERROR_OUT_OF_MEMORY: c_int = -4;
/// Invalid grammar
pub const PARSANOL_ERROR_INVALID_GRAMMAR: c_int = -5;

// ============================================================================
// Grammar Functions
// ============================================================================

/// Create a new grammar from JSON
///
/// The JSON format is:
/// ```json
/// {
///     "root": 0,
///     "atoms": [
///         {"Str": {"pattern": "hello"}},
///         {"Re": {"pattern": "[0-9]+"}},
///         {"Sequence": {"atoms": [0, 1]}},
///         {"Alternative": {"atoms": [0, 1]}},
///         {"Repetition": {"atom": 0, "min": 0, "max": null}},
///         {"Named": {"name": "value", "atom": 0}},
///         {"Entity": {"atom": 0}},
///         {"Lookahead": {"atom": 0, "positive": true}},
///         {"Cut": null},
///         {"Ignore": {"atom": 0}},
///         {"Custom": {"id": 100}}
///     ]
/// }
/// ```
///
/// # Safety
///
/// - `json` must be a valid null-terminated C string
/// - The returned pointer must be freed with `parsanol_grammar_free`
#[no_mangle]
pub unsafe extern "C" fn parsanol_grammar_new(json: *const c_char) -> *mut ParsanolGrammar {
    if json.is_null() {
        return ptr::null_mut();
    }

    let json_str = match CStr::from_ptr(json).to_str() {
        Ok(s) => s,
        Err(_) => return ptr::null_mut(),
    };

    let grammar = match Grammar::from_json(json_str) {
        Ok(g) => g,
        Err(_) => return ptr::null_mut(),
    };

    let boxed = Box::new(ParsanolGrammar { grammar });
    Box::into_raw(boxed)
}

/// Create a new grammar from JSON and return an error code
///
/// This is similar to `parsanol_grammar_new` but provides error details.
///
/// # Safety
///
/// - `json` must be a valid null-terminated C string
/// - `grammar_out` must be a valid pointer to a `ParsanolGrammar*`
/// - The returned pointer in `grammar_out` must be freed with `parsanol_grammar_free`
#[no_mangle]
pub unsafe extern "C" fn parsanol_grammar_new_with_error(
    json: *const c_char,
    grammar_out: *mut *mut ParsanolGrammar,
) -> c_int {
    if json.is_null() || grammar_out.is_null() {
        return PARSANOL_ERROR_NULL_POINTER;
    }

    let json_str = match CStr::from_ptr(json).to_str() {
        Ok(s) => s,
        Err(_) => return PARSANOL_ERROR_INVALID_JSON,
    };

    let grammar = match Grammar::from_json(json_str) {
        Ok(g) => g,
        Err(_) => return PARSANOL_ERROR_INVALID_JSON,
    };

    let boxed = Box::new(ParsanolGrammar { grammar });
    *grammar_out = Box::into_raw(boxed);
    PARSANOL_OK
}

/// Free a grammar
///
/// # Safety
///
/// - `grammar` must be a valid pointer returned by `parsanol_grammar_new`
/// - The pointer must not be used after this call
#[no_mangle]
pub unsafe extern "C" fn parsanol_grammar_free(grammar: *mut ParsanolGrammar) {
    if !grammar.is_null() {
        let _ = Box::from_raw(grammar);
    }
}

/// Get the number of atoms in a grammar
///
/// # Safety
///
/// - `grammar` must be a valid pointer returned by `parsanol_grammar_new`
#[no_mangle]
pub unsafe extern "C" fn parsanol_grammar_atom_count(grammar: *const ParsanolGrammar) -> c_ulong {
    if grammar.is_null() {
        return 0;
    }

    let grammar = &*grammar;
    grammar.grammar.atoms.len() as c_ulong
}

/// Get the root atom index
///
/// # Safety
///
/// - `grammar` must be a valid pointer returned by `parsanol_grammar_new`
#[no_mangle]
pub unsafe extern "C" fn parsanol_grammar_root(grammar: *const ParsanolGrammar) -> c_ulong {
    if grammar.is_null() {
        return 0;
    }

    let grammar = &*grammar;
    grammar.grammar.root as c_ulong
}

// ============================================================================
// Parsing Functions
// ============================================================================

/// Parse input using a grammar
///
/// # Safety
///
/// - `grammar` must be a valid pointer returned by `parsanol_grammar_new`
/// - `input` must be a valid null-terminated C string
/// - `result_out` must be a valid pointer to a `ParsanolResult*`
/// - The returned pointer in `result_out` must be freed with `parsanol_result_free`
#[no_mangle]
pub unsafe extern "C" fn parsanol_parse(
    grammar: *const ParsanolGrammar,
    input: *const c_char,
    result_out: *mut *mut ParsanolResult,
) -> c_int {
    if grammar.is_null() || input.is_null() || result_out.is_null() {
        return PARSANOL_ERROR_NULL_POINTER;
    }

    let grammar_ref = &*grammar;
    let input_str = match CStr::from_ptr(input).to_str() {
        Ok(s) => s,
        Err(_) => return PARSANOL_ERROR_PARSE_FAILED,
    };

    let parse_result = grammar_ref.grammar.parse_with_pos(input_str);

    let result = match parse_result {
        Ok(ast_result) => {
            // Serialize AST to JSON
            let ast_json = serde_json::to_string(&ast_result.value).ok();
            let ast_cstring = ast_json.and_then(|j| CString::new(j).ok());

            ParsanolResult {
                success: true,
                end_pos: ast_result.end_pos,
                error_message: None,
                ast_json: ast_cstring,
            }
        }
        Err(e) => {
            let error_msg = format!("{}", e);
            let error_cstring = CString::new(error_msg).ok();

            ParsanolResult {
                success: false,
                end_pos: 0,
                error_message: error_cstring,
                ast_json: None,
            }
        }
    };

    let boxed = Box::new(result);
    *result_out = Box::into_raw(boxed);
    PARSANOL_OK
}

/// Parse input and return only success/failure
///
/// This is a simpler API for cases where you don't need the AST.
///
/// # Safety
///
/// - `grammar` must be a valid pointer returned by `parsanol_grammar_new`
/// - `input` must be a valid null-terminated C string
#[no_mangle]
pub unsafe extern "C" fn parsanol_parse_simple(
    grammar: *const ParsanolGrammar,
    input: *const c_char,
) -> c_int {
    if grammar.is_null() || input.is_null() {
        return PARSANOL_ERROR_NULL_POINTER;
    }

    let grammar_ref = &*grammar;
    let input_str = match CStr::from_ptr(input).to_str() {
        Ok(s) => s,
        Err(_) => return PARSANOL_ERROR_PARSE_FAILED,
    };

    match grammar_ref.grammar.parse(input_str) {
        Ok(_) => PARSANOL_OK,
        Err(_) => PARSANOL_ERROR_PARSE_FAILED,
    }
}

/// Parse input and return end position
///
/// Returns the position after the matched content, or -1 on failure.
///
/// # Safety
///
/// - `grammar` must be a valid pointer returned by `parsanol_grammar_new`
/// - `input` must be a valid null-terminated C string
#[no_mangle]
pub unsafe extern "C" fn parsanol_parse_end_pos(
    grammar: *const ParsanolGrammar,
    input: *const c_char,
) -> c_int {
    if grammar.is_null() || input.is_null() {
        return PARSANOL_ERROR_NULL_POINTER;
    }

    let grammar_ref = &*grammar;
    let input_str = match CStr::from_ptr(input).to_str() {
        Ok(s) => s,
        Err(_) => return PARSANOL_ERROR_PARSE_FAILED,
    };

    match grammar_ref.grammar.parse_with_pos(input_str) {
        Ok(result) => result.end_pos as c_int,
        Err(_) => PARSANOL_ERROR_PARSE_FAILED,
    }
}

// ============================================================================
// Result Functions
// ============================================================================

/// Check if a parse result was successful
///
/// # Safety
///
/// - `result` must be a valid pointer returned by `parsanol_parse`
#[no_mangle]
pub unsafe extern "C" fn parsanol_result_success(result: *const ParsanolResult) -> c_int {
    if result.is_null() {
        return 0;
    }

    let result = &*result;
    if result.success {
        1
    } else {
        0
    }
}

/// Get the end position from a parse result
///
/// # Safety
///
/// - `result` must be a valid pointer returned by `parsanol_parse`
#[no_mangle]
pub unsafe extern "C" fn parsanol_result_end_pos(result: *const ParsanolResult) -> c_ulong {
    if result.is_null() {
        return 0;
    }

    let result = &*result;
    result.end_pos as c_ulong
}

/// Get the error message from a parse result
///
/// Returns NULL if the parse was successful or there is no error message.
///
/// # Safety
///
/// - `result` must be a valid pointer returned by `parsanol_parse`
/// - The returned string is valid until `parsanol_result_free` is called
#[no_mangle]
pub unsafe extern "C" fn parsanol_result_error(result: *const ParsanolResult) -> *const c_char {
    if result.is_null() {
        return ptr::null();
    }

    let result = &*result;
    match &result.error_message {
        Some(msg) => msg.as_ptr(),
        None => ptr::null(),
    }
}

/// Get the AST as JSON from a parse result
///
/// Returns NULL if the parse failed or there is no AST.
///
/// # Safety
///
/// - `result` must be a valid pointer returned by `parsanol_parse`
/// - The returned string is valid until `parsanol_result_free` is called
#[no_mangle]
pub unsafe extern "C" fn parsanol_result_ast_json(result: *const ParsanolResult) -> *const c_char {
    if result.is_null() {
        return ptr::null();
    }

    let result = &*result;
    match &result.ast_json {
        Some(json) => json.as_ptr(),
        None => ptr::null(),
    }
}

/// Free a parse result
///
/// # Safety
///
/// - `result` must be a valid pointer returned by `parsanol_parse`
/// - The pointer must not be used after this call
#[no_mangle]
pub unsafe extern "C" fn parsanol_result_free(result: *mut ParsanolResult) {
    if !result.is_null() {
        let _ = Box::from_raw(result);
    }
}

// ============================================================================
// String Functions
// ============================================================================

/// Free a string returned by the API
///
/// # Safety
///
/// - `s` must be a pointer returned by a parsanol function that returns a string
/// - The pointer must not be used after this call
#[no_mangle]
pub unsafe extern "C" fn parsanol_string_free(s: *mut c_char) {
    if !s.is_null() {
        let _ = CString::from_raw(s);
    }
}

// ============================================================================
// Version Functions
// ============================================================================

/// Get the library version
///
/// Returns a static string like "0.1.3".
///
/// # Safety
///
/// The returned string is static and must not be freed.
#[no_mangle]
pub extern "C" fn parsanol_version() -> *const c_char {
    static VERSION: &[u8] = concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes();
    VERSION.as_ptr() as *const c_char
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn test_version() {
        let version = parsanol_version();
        let version_str = unsafe { CStr::from_ptr(version) }.to_str().unwrap();
        assert!(!version_str.is_empty());
    }

    #[test]
    fn test_grammar_new_and_free() {
        let json =
            CString::new(r#"{"root": 0, "atoms": [{"Str": {"pattern": "hello"}}]}"#).unwrap();
        let grammar = unsafe { parsanol_grammar_new(json.as_ptr()) };
        assert!(!grammar.is_null());
        unsafe { parsanol_grammar_free(grammar) };
    }

    #[test]
    fn test_grammar_new_null() {
        let grammar = unsafe { parsanol_grammar_new(ptr::null()) };
        assert!(grammar.is_null());
    }

    #[test]
    fn test_parse_simple() {
        let json =
            CString::new(r#"{"root": 0, "atoms": [{"Str": {"pattern": "hello"}}]}"#).unwrap();
        let grammar = unsafe { parsanol_grammar_new(json.as_ptr()) };
        assert!(!grammar.is_null());

        // Parse succeeds only if the entire input is consumed
        let input = CString::new("hello").unwrap();
        let result = unsafe { parsanol_parse_simple(grammar, input.as_ptr()) };
        assert_eq!(result, PARSANOL_OK);

        // This fails because " world" is not consumed
        let partial_input = CString::new("hello world").unwrap();
        let result = unsafe { parsanol_parse_simple(grammar, partial_input.as_ptr()) };
        assert_eq!(result, PARSANOL_ERROR_PARSE_FAILED);

        let bad_input = CString::new("goodbye").unwrap();
        let result = unsafe { parsanol_parse_simple(grammar, bad_input.as_ptr()) };
        assert_eq!(result, PARSANOL_ERROR_PARSE_FAILED);

        unsafe { parsanol_grammar_free(grammar) };
    }

    #[test]
    fn test_parse_with_result() {
        let json =
            CString::new(r#"{"root": 0, "atoms": [{"Str": {"pattern": "hello"}}]}"#).unwrap();
        let grammar = unsafe { parsanol_grammar_new(json.as_ptr()) };
        assert!(!grammar.is_null());

        let input = CString::new("hello world").unwrap();
        let mut result_ptr: *mut ParsanolResult = ptr::null_mut();

        let rc = unsafe { parsanol_parse(grammar, input.as_ptr(), &mut result_ptr) };
        assert_eq!(rc, PARSANOL_OK);
        assert!(!result_ptr.is_null());

        let success = unsafe { parsanol_result_success(result_ptr) };
        assert_eq!(success, 1);

        let end_pos = unsafe { parsanol_result_end_pos(result_ptr) };
        assert_eq!(end_pos, 5);

        let ast_json = unsafe { parsanol_result_ast_json(result_ptr) };
        assert!(!ast_json.is_null());

        unsafe { parsanol_result_free(result_ptr) };
        unsafe { parsanol_grammar_free(grammar) };
    }

    #[test]
    fn test_grammar_atom_count() {
        let json = CString::new(r#"{"root": 0, "atoms": [{"Str": {"pattern": "hello"}}, {"Re": {"pattern": "[0-9]+"}}]}"#).unwrap();
        let grammar = unsafe { parsanol_grammar_new(json.as_ptr()) };
        assert!(!grammar.is_null());

        let count = unsafe { parsanol_grammar_atom_count(grammar) };
        assert_eq!(count, 2);

        unsafe { parsanol_grammar_free(grammar) };
    }
}
