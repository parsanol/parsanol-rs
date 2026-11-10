//! Exercise the existing causal-policy fixture through the public artifact/C ABI.
//! Grammar mirrors portable/parser/compatibility_tests.rs's successful-negative-
//! lookahead regression; UTF-8 byte offsets must not be adjusted arithmetically.
use parsanol::parg::{PargArtifact, PargError};
use parsanol::portable::parser::ParserCompatibility;
use serde_json::{json, Value};
use std::ffi::{CStr, CString};

fn artifact(prefix: &str) -> String {
    let mut value = json!({
    "shape":"parsanol-tree/v2", "grammar":"DiagnosticPolicy", "version":"1.0.0",
    "default_entry":"main", "entries":{"main":{"root":"main","grammar":{
        "root":7,"atoms":[
            {"Str":{"pattern":format!("{prefix}b")}},
            {"Str":{"pattern":"c"}}, {"Sequence":{"atoms":[0,1]}},
            {"Lookahead":{"atom":2,"positive":false}},
            {"Str":{"pattern":prefix}}, {"Str":{"pattern":"!"}},
            {"Str":{"pattern":"unused"}}, {"Sequence":{"atoms":[3,4,5]}}
        ]
    }}}});
    value["checksum"] = Value::String(parsanol::parg::checksum_hex(&value).unwrap());
    value.to_string()
}

#[test]
fn artifact_policy_preserves_strict_default_and_selects_causal_diagnostics() {
    for (prefix, input, causal) in [("a", "ab?", 1), ("α", "αb?", 2)] {
        let artifact = PargArtifact::from_json(&artifact(prefix)).unwrap();
        for (policy, expected) in [
            (ParserCompatibility::Strict, causal + 1),
            (ParserCompatibility::Parslet, causal),
        ] {
            assert!(
                matches!(artifact.parse_shape_with_compatibility("main",input,policy),
                Err(PargError::ParseWire {offset,..}) if offset == expected)
            );
        }
        assert!(matches!(artifact.parse_shape("main",input),
            Err(PargError::ParseWire {offset,..}) if offset == causal+1));
        assert!(artifact
            .parse_shape_with_compatibility(
                "main",
                &format!("{prefix}!"),
                ParserCompatibility::Parslet
            )
            .is_ok());
    }
}

#[test]
fn c_policy_returns_owned_envelopes_and_rejects_invalid_parameters() {
    use parsanol::ffi::c::{parsanol_parg_free, parsanol_parg_parse_with_compatibility};
    let artifact = CString::new(artifact("α")).unwrap();
    let entry = CString::new("main").unwrap();
    let input = CString::new("αb?").unwrap();
    for (policy, expected) in [(0, 3), (1, 2)] {
        unsafe {
            let ptr = parsanol_parg_parse_with_compatibility(
                artifact.as_ptr(),
                entry.as_ptr(),
                input.as_ptr(),
                policy,
            );
            assert!(!ptr.is_null());
            let value: Value = serde_json::from_str(CStr::from_ptr(ptr).to_str().unwrap()).unwrap();
            parsanol_parg_free(ptr);
            assert_eq!(value["ok"], false);
            assert_eq!(value["error"]["offset"], expected);
        }
    }
    unsafe {
        assert!(parsanol_parg_parse_with_compatibility(
            artifact.as_ptr(),
            entry.as_ptr(),
            input.as_ptr(),
            99
        )
        .is_null());
        assert!(parsanol_parg_parse_with_compatibility(
            std::ptr::null(),
            entry.as_ptr(),
            input.as_ptr(),
            1
        )
        .is_null());
    }
}
