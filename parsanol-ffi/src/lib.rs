//! C-ABI cdylib for parsanol.
//!
//! A separate crate so the MRI extension never links this artifact: the
//! `ffi`-gem tier (JRuby, TruffleRuby, MRI without a binary) dlopens the
//! produced libparsanol.{so,dylib,dll} and calls the `parsanol_c_*`
//! functions below. Grammars register once into Rust-side handles;
//! parses return the shared flat-u64 batch encoding.

// ---------------------------------------------------------------------------
// Handle + batch API (for the Ruby `ffi`-gem tier and other runtimes that
// cannot load C-API extensions). Grammars register once and parse by handle;
// results cross the boundary in the shared flat-u64 batch format, decoded on
// the Ruby side by BatchDecoder. Pure portable code — no magnus.
// ---------------------------------------------------------------------------

use std::collections::HashMap;
use std::ffi::{c_char, CStr};
use std::ptr;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::Mutex;

use parsanol::ffi::shared::{collapse_ast, flatten_ast_to_u64};
use parsanol::portable::{AstArena, Grammar, PortableParser};

static C_NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);
static C_LAST_ERROR: Mutex<String> = Mutex::new(String::new());

fn c_handle_map() -> &'static Mutex<HashMap<u64, Grammar>> {
    static MAP: std::sync::OnceLock<Mutex<HashMap<u64, Grammar>>> = std::sync::OnceLock::new();
    MAP.get_or_init(|| Mutex::new(HashMap::new()))
}

fn c_set_error(msg: &str) {
    if let Ok(mut guard) = C_LAST_ERROR.lock() {
        guard.clear();
        guard.push_str(msg);
        // Keep the buffer NUL-terminated: FFI callers read a C string.
        guard.push('\0');
    }
}

/// Register a grammar JSON and get a handle (0 on failure).
///
/// # Safety
///
/// - `json` must be a valid null-terminated C string
#[no_mangle]
pub unsafe extern "C" fn parsanol_c_register(json: *const c_char) -> u64 {
    if json.is_null() {
        c_set_error("null grammar json");
        return 0;
    }
    let json_str = match CStr::from_ptr(json).to_str() {
        Ok(s) => s,
        Err(_) => {
            c_set_error("grammar json is not valid UTF-8");
            return 0;
        }
    };
    match Grammar::from_json(json_str) {
        Ok(grammar) => {
            let handle = C_NEXT_HANDLE.fetch_add(1, AtomicOrdering::Relaxed);
            if let Ok(mut map) = c_handle_map().lock() {
                map.insert(handle, grammar);
            }
            handle
        }
        Err(e) => {
            c_set_error(&format!("grammar json error: {e}"));
            0
        }
    }
}

/// Release a grammar registered with `parsanol_c_register`.
///
/// Safe to call with an unknown handle (no-op).
#[no_mangle]
pub extern "C" fn parsanol_c_release(handle: u64) {
    if let Ok(mut map) = c_handle_map().lock() {
        map.remove(&handle);
    }
}

/// Last error message from `parsanol_c_register`/`parsanol_c_parse`.
///
/// The returned pointer stays valid until the next library call and must
/// not be freed.
#[no_mangle]
pub extern "C" fn parsanol_c_last_error() -> *const c_char {
    // The string lives in a static Mutex and is never freed; callers read it
    // before issuing the next call. A NUL terminator is guaranteed.
    match C_LAST_ERROR.lock() {
        Ok(guard) => {
            if guard.is_empty() {
                // An empty String's buffer pointer is dangling, not a
                // readable C string; hand out the static NUL instead.
                c"".as_ptr() as *const c_char
            } else {
                guard.as_ptr() as *const c_char
            }
        }
        Err(_) => c"".as_ptr() as *const c_char,
    }
}

/// Parse `input` (NUL-free UTF-8, NUL-terminated) with a registered handle
/// and write the flat-u64 batch encoding into `out`.
///
/// Returns:
/// - `> 0`: number of u64 cells written (parse succeeded)
/// - `0`: parse failed; see `parsanol_c_last_error` (empty on clean failure)
/// - `< 0`: `-needed` — `out` is too small; retry with `cap >= needed`
///
/// # Safety
///
/// - `handle` must come from `parsanol_c_register` and not be released
/// - `input` must be a valid null-terminated C string
/// - `out` must be valid for writes of `cap` u64 cells
#[no_mangle]
pub unsafe extern "C" fn parsanol_c_parse(
    handle: u64,
    input: *const c_char,
    out: *mut u64,
    cap: usize,
) -> isize {
    use flatten_ast_to_u64;

    let grammar = {
        let map = match c_handle_map().lock() {
            Ok(m) => m,
            Err(_) => {
                c_set_error("internal: handle map poisoned");
                return 0;
            }
        };
        match map.get(&handle) {
            Some(g) => g.clone(),
            None => {
                c_set_error("unknown grammar handle");
                return 0;
            }
        }
    };

    let input_str = if input.is_null() {
        ""
    } else {
        match CStr::from_ptr(input).to_str() {
            Ok(s) => s,
            Err(_) => {
                c_set_error("input is not valid UTF-8");
                return 0;
            }
        }
    };

    let mut arena = AstArena::for_input(input_str.len());
    arena.set_input(input_str.to_string());
    let mut parser = PortableParser::new(&grammar, input_str, &mut arena);
    let ast = match parser.parse() {
        Ok(ast) => ast,
        Err(e) => {
            c_set_error(&format!("{e}"));
            return 0;
        }
    };

    // Same pipeline as the extension tier: collapse adjacent input refs
    // (semantically neutral, shrinks the flat encoding), then flatten
    // the RAW tagged tree — NOT to_parslet_compatible's pre-fold. The
    // Ruby-side AstTransformer must see the same tagged shapes on every
    // tier, or the backends build different trees for one grammar.
    let collapsed = collapse_ast(&ast, &mut arena);
    let mut flat: Vec<u64> = Vec::new();
    flatten_ast_to_u64(&collapsed, &arena, input_str, &mut flat);

    if flat.len() > cap {
        return -(flat.len() as isize);
    }
    if !flat.is_empty() {
        ptr::copy_nonoverlapping(flat.as_ptr(), out, flat.len());
    }
    c_set_error("");
    flat.len() as isize
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn test_c_register_parse_batch() {
        // str("ab").as(:greeting): flat batch must decode to {greeting: "ab"}.
        let json = CString::new(
            r#"{"root": 1, "atoms": [{"Str": {"pattern": "ab"}}, {"Named": {"name": "greeting", "atom": 0}}]}"#,
        )
        .unwrap();
        let handle = unsafe { parsanol_c_register(json.as_ptr()) };
        assert_ne!(handle, 0, "register failed: last_error={:?}", unsafe {
            CStr::from_ptr(parsanol_c_last_error()).to_string_lossy()
        });

        let input = CString::new("ab").unwrap();
        let mut buf = [0u64; 64];
        let written =
            unsafe { parsanol_c_parse(handle, input.as_ptr(), buf.as_mut_ptr(), buf.len()) };
        assert!(written > 0, "parse failed: last_error={:?}", unsafe {
            CStr::from_ptr(parsanol_c_last_error()).to_string_lossy()
        });

        // Buffer-too-small reports the needed size.
        let needed = unsafe { parsanol_c_parse(handle, input.as_ptr(), buf.as_mut_ptr(), 1) };
        assert_eq!(needed, -written);

        // A failing input reports 0 with a clean error.
        let bad = CString::new("zz").unwrap();
        let failed = unsafe { parsanol_c_parse(handle, bad.as_ptr(), buf.as_mut_ptr(), buf.len()) };
        assert_eq!(failed, 0);

        parsanol_c_release(handle);
    }

    #[test]
    fn last_error_is_always_readable() {
        assert!(!parsanol_c_last_error().is_null());
    }
}
