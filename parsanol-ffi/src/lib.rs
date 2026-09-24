//! C-ABI cdylib for parsanol.
//!
//! A separate crate so the MRI extension never links this artifact: the
//! `ffi`-gem tier (JRuby, TruffleRuby, MRI without a binary) dlopens the
//! produced libparsanol.{so,dylib,dll}. The `#[no_mangle] parsanol_c_*`
//! exports come from the parsanol rlib itself — linking it here is what
//! publishes them; no wrappers, no duplicated symbols. Grammars register
//! once into Rust-side handles; parses return the shared flat-u64 batch
//! encoding.

// Force the dependency into the link even under aggressive stripping.
#[allow(unused_imports)]
use parsanol::ffi::c::{
    parsanol_c_last_error, parsanol_c_parse, parsanol_c_parse_len, parsanol_c_register,
    parsanol_c_release,
};

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

        // Binary-safe entry point must be exported too (compile-time
        // link check): the same parse through the explicit-length call.
        let written_len = unsafe {
            parsanol_c_parse_len(
                handle,
                input.as_ptr(),
                input.as_bytes().len(),
                buf.as_mut_ptr(),
                buf.len(),
            )
        };
        assert_eq!(written_len, written, "len-parse failed: last_error={:?}", unsafe {
            CStr::from_ptr(parsanol_c_last_error()).to_string_lossy()
        });

        parsanol_c_release(handle);
    }

    #[test]
    fn last_error_is_always_readable() {
        assert!(!parsanol_c_last_error().is_null());
    }
}
