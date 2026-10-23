//! C-ABI differential: the recovery export returns the SAME envelope the
//! artifact-level `parse_shape_recovering` (the wasm wire) returns —
//! including the spans of declared `resync` policies.

use parsanol::parg::PargArtifact;
use std::ffi::{CStr, CString};

fn corpus_dir() -> Option<std::path::PathBuf> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()?
        .join("corpus")
        .join("conformance");
    dir.is_dir().then_some(dir)
}

fn take(ptr: *mut std::os::raw::c_char) -> String {
    assert!(!ptr.is_null());
    let text = unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned();
    unsafe { parsanol::ffi::c::parsanol_parg_free(ptr) };
    text
}

#[test]
fn c_abi_recovery_matches_the_artifact_wire() {
    let Some(dir) = corpus_dir() else {
        eprintln!("skipping: corpus not present outside the workspace");
        return;
    };

    let path = dir.join("recovery_resync_declared.json");
    let text = std::fs::read_to_string(&path).expect("artifact readable");
    let artifact = PargArtifact::from_json(&text).expect("artifact verifies");
    let entry = artifact.default_entry().expect("default entry").to_string();
    let input = "a=1;b=2;@@junk@@;c=3;d=4;";

    let reference = artifact
        .parse_shape_recovering(&entry, input)
        .expect("recovery wire");

    let envelope = CString::new(text).unwrap();
    let entry_c = CString::new(entry).unwrap();
    let input_c = CString::new(input).unwrap();

    let wire = take(unsafe {
        parsanol::ffi::c::parsanol_parg_parse_recovering(
            envelope.as_ptr(),
            entry_c.as_ptr(),
            input_c.as_ptr(),
        )
    });
    let wire: serde_json::Value = serde_json::from_str(&wire).expect("C wire is valid JSON");

    // Canonical comparison: the C ABI's envelope equals the artifact-level
    // wire (the wasm reference), canonical-form rendered.
    let expected = parsanol::parg::canonical_json(&reference).expect("canonical");
    let actual = parsanol::parg::canonical_json(&wire).expect("canonical");
    assert_eq!(
        actual, expected,
        "C recovery wire diverged from the artifact wire"
    );

    // And the declared resync span is the pinned one.
    let spans = wire["resyncs"].as_array().expect("resyncs array");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0]["skipped"], "@@junk@@;");
    assert_eq!(spans[0]["offset"], 8);
    assert_eq!(spans[0]["resumed_at"], 17);
}

#[test]
fn c_abi_null_entry_selects_the_default() {
    let Some(dir) = corpus_dir() else {
        eprintln!("skipping: corpus not present outside the workspace");
        return;
    };
    let text =
        std::fs::read_to_string(dir.join("recovery_resync_declared.json")).expect("readable");
    let envelope = CString::new(text).unwrap();
    let input = CString::new("a=1;b=2;").unwrap();

    let with_entry = CString::new("main").unwrap();
    let named = take(unsafe {
        parsanol::ffi::c::parsanol_parg_parse_recovering(
            envelope.as_ptr(),
            with_entry.as_ptr(),
            input.as_ptr(),
        )
    });
    let default = take(unsafe {
        parsanol::ffi::c::parsanol_parg_parse_recovering(
            envelope.as_ptr(),
            std::ptr::null(),
            input.as_ptr(),
        )
    });
    assert_eq!(named, default);
}
