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

#[test]
fn recursion_abort_is_not_syntax_or_recovery_captures() {
    std::thread::Builder::new().stack_size(64*1024*1024).spawn(|| {
        let mut value=json!({"shape":"parsanol-tree/v2","grammar":"Recursive","version":"1.0.0",
            "default_entry":"main","entries":{"main":{"root":"main","grammar":{"root":0,"atoms":[
                {"Entity":{"atom":1}},{"Alternative":{"atoms":[2,5]}},
                {"Sequence":{"atoms":[3,0,4]}},{"Str":{"pattern":"("}},
                {"Str":{"pattern":")"}},{"Str":{"pattern":"x"}}]}}}});
        value["checksum"]=Value::String(parsanol::parg::checksum_hex(&value).unwrap());
        let text=value.to_string();let artifact=PargArtifact::from_json(&text).unwrap();
        let nested=format!("{}x{}","(".repeat(1200),")".repeat(1200));
        for result in [artifact.parse_shape("main",&nested),artifact.parse_shape_recovering("main",&nested)] {
            assert!(matches!(result,Err(PargError::RecursionLimitExceeded { max_depth:1000,.. })));
        }
        assert!(artifact.parse_shape("main","x").is_ok());
        let text=CString::new(text).unwrap();let input=CString::new(nested).unwrap();let entry=CString::new("main").unwrap();
        unsafe {
            use parsanol::ffi::c::{parsanol_parg_parse_with_compatibility,parsanol_parg_free};
            let ptr=parsanol_parg_parse_with_compatibility(text.as_ptr(),entry.as_ptr(),input.as_ptr(),0);
            assert!(!ptr.is_null());let response:Value=serde_json::from_str(CStr::from_ptr(ptr).to_str().unwrap()).unwrap();parsanol_parg_free(ptr);
            assert_eq!(response["error"]["category"],"recursion_limit");
            assert_eq!(response["error"]["max_depth"],1000);assert!(response["error"]["offset"].is_null());
        }
    }).unwrap().join().unwrap();
}

#[test]
fn scoped_workers_join_and_caller_errors_remain_thread_local() {
    use std::sync::{Arc, Barrier};
    let barrier = Arc::new(Barrier::new(4));
    let handles: Vec<_> = (0..4)
        .map(|index| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                use parsanol::ffi::c::{
                    parsanol_c_last_error, parsanol_parg_free,
                    parsanol_parg_parse_with_compatibility,
                };
                let invalid = index % 2 == 0;
                let pointer = unsafe {
                    // Drop all borrowed input buffers immediately after the call.
                    let text = CString::new(artifact("α")).unwrap();
                    let entry = CString::new("main").unwrap();
                    let input = CString::new("αb?").unwrap();
                    parsanol_parg_parse_with_compatibility(
                        text.as_ptr(),
                        entry.as_ptr(),
                        input.as_ptr(),
                        if invalid { 99 } else { 1 },
                    )
                };
                barrier.wait();
                unsafe {
                    let last = CStr::from_ptr(parsanol_c_last_error()).to_str().unwrap();
                    if invalid {
                        assert!(pointer.is_null());
                        assert!(last.contains("unknown parsing compatibility"));
                    } else {
                        assert!(last.is_empty());
                        assert!(!pointer.is_null());
                        let response: Value =
                            serde_json::from_str(CStr::from_ptr(pointer).to_str().unwrap())
                                .unwrap();
                        parsanol_parg_free(pointer);
                        assert_eq!(response["error"]["offset"], 2);
                    }
                }
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
}

#[test]
fn vm_call_budget_is_a_typed_abort_and_does_not_poison_reuse() {
    use parsanol::portable::ast::ParseError;
    use parsanol::portable::{AstArena, Grammar, PortableParser};
    let grammar=Grammar::from_json(r#"{"root":0,"atoms":[{"Entity":{"atom":1}},{"Alternative":{"atoms":[2,5]}},{"Sequence":{"atoms":[3,0,4]}},{"Str":{"pattern":"("}},{"Str":{"pattern":")"}},{"Str":{"pattern":"x"}}]}"#).unwrap();
    let input = format!("{}x{}", "(".repeat(10), ")".repeat(10));
    for mode in [ParserCompatibility::Strict, ParserCompatibility::Parslet] {
        let mut arena = AstArena::for_input(1 << 16);
        let mut parser = PortableParser::new(&grammar, &input, &mut arena);
        parser.set_compatibility(mode).unwrap();
        parser.set_max_call_depth(16);
        let error = parser.parse().unwrap_err();
        assert!(error.is_abort());
        assert!(matches!(
            error,
            ParseError::RecursionLimitExceeded {
                depth: 17,
                max_depth: 16
            }
        ));
        let rich = parser.parse_with_rich_error().unwrap_err();
        assert_eq!(
            rich.message,
            "VM call depth exceeded: depth 17 exceeds limit of 16"
        );
        parser.set_max_call_depth(0);
        assert!(parser.parse().is_ok());
    }
}
