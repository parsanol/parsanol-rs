//! C-ABI cdylib for parsanol.
//!
//! A separate crate so the MRI extension never links this artifact: the
//! `ffi`-gem tier (JRuby, TruffleRuby, MRI without a binary) dlopens the
//! produced libparsanol.{so,dylib,dll} and calls the `parsanol_c_*`
//! exports below, which delegate to `parsanol::ffi::c`. Grammars
//! register once into Rust-side handles; parses return the shared
//! flat-u64 batch encoding.

use std::ffi::c_char;

use parsanol::ffi::c::{
    parsanol_c_last_error as c_last_error, parsanol_c_parse as c_parse,
    parsanol_c_register as c_register, parsanol_c_release as c_release,
};

/// # Safety
/// - `json` must be a valid null-terminated C string
#[no_mangle]
pub unsafe extern "C" fn parsanol_c_register(json: *const c_char) -> u64 {
    unsafe { c_register(json) }
}

/// # Safety
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
    unsafe { c_parse(handle, input, out, cap) }
}

/// Release a grammar registered with `parsanol_c_register`.
#[no_mangle]
pub extern "C" fn parsanol_c_release(handle: u64) {
    c_release(handle)
}

/// Last error message from `parsanol_c_register`/`parsanol_c_parse`.
///
/// The returned pointer stays valid until the next library call and must
/// not be freed.
#[no_mangle]
pub extern "C" fn parsanol_c_last_error() -> *const c_char {
    c_last_error()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::{CStr, CString};

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
