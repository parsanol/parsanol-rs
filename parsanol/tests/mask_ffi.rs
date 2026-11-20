//! The mask FFI acceptance (rs#530's runtime slice): the `parsanol_mask_*`
//! exports return the SAME llguidance wire the rust-level `MaskEmitter`
//! emits, and the C-driven generation loop reaches 100% grammar-valid
//! output on both acceptance grammars (isbn, json) — the validity scorer
//! is the artifact oracle itself, the deterministic lowest-allowed-id
//! walk the mask receipt ran.

use parsanol::constrain::{MaskEmitter, Token, TokenOracle};
use parsanol::ffi::c::{
    parsanol_c_last_error, parsanol_mask_free, parsanol_mask_open, parsanol_mask_step,
    parsanol_parg_free, ParsanolMask, PARSANOL_OK,
};
use parsanol::parg::PargArtifact;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;

/// The acceptance fixtures: the same bytes the in-crate constrain tests
/// and the mask receipt used.
fn fixture(name: &str) -> &'static str {
    match name {
        "isbn" => include_str!("constrain_fixtures/isbn.json"),
        "json" => include_str!("constrain_fixtures/json.json"),
        _ => unreachable!(),
    }
}

/// The receipt's isbn vocabulary: the tokens of an ISBN-shaped generation.
fn isbn_vocab() -> Vec<(u64, &'static str)> {
    vec![
        (1, "ISBN"),
        (2, " "),
        (3, ":"),
        (4, "0"),
        (5, "9"),
        (6, "7"),
        (7, "8"),
        (8, "-"),
        (9, "X"),
        (10, "978"),
        (11, "-0"),
    ]
}

/// The receipt's json vocabulary.
fn json_vocab() -> Vec<(u64, &'static str)> {
    vec![
        (1, "{"),
        (2, "}"),
        (3, "\""),
        (4, "a"),
        (5, ":"),
        (6, ","),
        (7, "1"),
        (8, " "),
        (9, "["),
        (10, "]"),
    ]
}

fn vocab_wire(vocab: &[(u64, &str)]) -> CString {
    let items: Vec<serde_json::Value> = vocab
        .iter()
        .map(|(id, text)| serde_json::json!({"id": id, "text": text}))
        .collect();
    CString::new(serde_json::Value::Array(items).to_string()).expect("vocab wire has no NUL")
}

fn last_error() -> String {
    unsafe { CStr::from_ptr(parsanol_c_last_error()) }
        .to_string_lossy()
        .into_owned()
}

/// One C step: the next-token bitmask bytes at `prefix` (freed here).
fn c_step(handle: *mut ParsanolMask, prefix: &str) -> Vec<u8> {
    let mut out: *mut c_char = std::ptr::null_mut();
    let mut len: usize = 0;
    let rc = unsafe {
        parsanol_mask_step(
            handle,
            prefix.as_ptr().cast::<c_char>(),
            prefix.len(),
            &mut out,
            &mut len,
        )
    };
    assert_eq!(rc, PARSANOL_OK, "step failed: {}", last_error());
    assert!(len % 4 == 0, "the wire is whole u32 words: {len}");
    let bytes = unsafe { std::slice::from_raw_parts(out.cast::<u8>(), len) }.to_vec();
    unsafe { parsanol_parg_free(out) };
    bytes
}

/// The deterministic lowest-allowed-id walk through the C ABI: the same
/// sampler the mask receipt ran. Returns (text, picked ids, parsed?).
fn c_generate(
    fixture_name: &str,
    entry: &str,
    vocab: &[(u64, &str)],
    cap: usize,
) -> (String, Vec<u64>) {
    let artifact_raw = CString::new(fixture(fixture_name)).expect("fixture has no NUL");
    let entry_c = CString::new(entry).expect("entry has no NUL");
    let vocab_c = vocab_wire(vocab);
    let handle =
        unsafe { parsanol_mask_open(artifact_raw.as_ptr(), entry_c.as_ptr(), vocab_c.as_ptr()) };
    assert!(!handle.is_null(), "open failed: {}", last_error());

    let mut prefix = String::new();
    let mut picked: Vec<u64> = Vec::new();
    let artifact = PargArtifact::from_json(fixture(fixture_name)).expect("artifact loads");
    for _ in 0..cap {
        let bytes = c_step(handle, &prefix);
        let id = (0..bytes.len() * 8)
            .find(|&bit| bytes[bit / 8] & (1 << (bit % 8)) != 0)
            .expect("the mask is non-empty") as u64;
        let text = vocab
            .iter()
            .find(|(vid, _)| *vid == id)
            .expect("the picked id is in the vocab")
            .1;
        prefix.push_str(text);
        picked.push(id);
        if artifact.parse_shape(entry, &prefix).is_ok() {
            break;
        }
    }
    unsafe { parsanol_mask_free(handle) };
    (prefix, picked)
}

#[test]
fn c_mask_wire_matches_the_rust_emitter() {
    for (name, entry, vocab, prefixes) in [
        (
            "isbn",
            "identifier",
            isbn_vocab(),
            vec!["", "978", "978-", "ISBN 0", "978-0-306-40615-", "X"],
        ),
        (
            "json",
            "main",
            json_vocab(),
            vec!["", "{", "{\"", "{\"a\"", "{\"a\":1}", "{\"a\"}"],
        ),
    ] {
        let artifact = PargArtifact::from_json(fixture(name)).expect("artifact loads");
        let oracle = TokenOracle::new(
            &artifact,
            entry,
            vocab
                .iter()
                .map(|(id, text)| Token {
                    id: *id,
                    text: (*text).to_string(),
                })
                .collect(),
        );
        let emitter = MaskEmitter::new(oracle);

        let raw = CString::new(fixture(name)).expect("no NUL");
        let entry_c = CString::new(entry).expect("no NUL");
        let vocab_c = vocab_wire(&vocab);
        let handle =
            unsafe { parsanol_mask_open(raw.as_ptr(), entry_c.as_ptr(), vocab_c.as_ptr()) };
        assert!(!handle.is_null(), "{name}: open failed: {}", last_error());

        for prefix in &prefixes {
            let wire = c_step(handle, prefix);
            let expected = emitter.mask_for(prefix).to_wire();
            assert_eq!(
                wire, expected,
                "{name}: the C wire diverges from MaskEmitter at {prefix:?}"
            );
        }

        // The drained prefix: an all-zero buffer of the same shape.
        let drained = c_step(handle, if name == "isbn" { "X" } else { "{\"a\"}" });
        assert!(
            drained.iter().all(|&b| b == 0),
            "{name}: the drained prefix must mask everything out"
        );
        assert_eq!(drained.len(), emitter.mask_for("").to_wire().len());

        unsafe { parsanol_mask_free(handle) };
    }
}

#[test]
fn c_mask_open_defaults_and_terminal_vocabulary() {
    let raw = CString::new(fixture("isbn")).expect("no NUL");

    // NULL entry + NULL vocabulary: the artifact's default entry and its
    // terminal vocabulary with ids 1..n. The ISBN literal is a terminal,
    // so the start mask is non-empty and sized by the terminal count.
    let handle = unsafe { parsanol_mask_open(raw.as_ptr(), std::ptr::null(), std::ptr::null()) };
    assert!(!handle.is_null(), "default open failed: {}", last_error());
    let artifact = PargArtifact::from_json(fixture("isbn")).expect("artifact loads");
    let terminals = artifact.terminal_vocabulary().len();
    assert!(terminals > 0, "the isbn artifact has terminals");
    let wire = c_step(handle, "");
    assert_eq!(wire.len(), terminals.div_ceil(32) * 4);
    assert!(
        wire.iter().any(|&b| b != 0),
        "the ISBN literal starts the identifier"
    );
    unsafe { parsanol_mask_free(handle) };

    // NULL entry on an envelope without a default would fall back to the
    // first entry; this fixture HAS a default, so both spellings agree.
    let entry_c = CString::new("identifier").expect("no NUL");
    let explicit = unsafe { parsanol_mask_open(raw.as_ptr(), entry_c.as_ptr(), std::ptr::null()) };
    assert!(!explicit.is_null());
    unsafe { parsanol_mask_free(explicit) };
}

#[test]
fn c_mask_open_rejects_bad_input_with_last_error() {
    let raw = CString::new(fixture("isbn")).expect("no NUL");
    let entry_c = CString::new("identifier").expect("no NUL");
    let vocab_c = vocab_wire(&isbn_vocab());

    // NULL artifact.
    assert!(
        unsafe { parsanol_mask_open(std::ptr::null(), entry_c.as_ptr(), vocab_c.as_ptr()) }
            .is_null()
    );
    assert!(!last_error().is_empty());

    // A tampered envelope is rejected by the checksum gate.
    let tampered =
        CString::new(fixture("isbn").replace("parsanol-tree/v2", "parsanol-tree/v3")).unwrap();
    assert!(
        unsafe { parsanol_mask_open(tampered.as_ptr(), entry_c.as_ptr(), vocab_c.as_ptr()) }
            .is_null(),
        "checksum rejection"
    );

    // An unknown entry.
    let wrong = CString::new("nope").expect("no NUL");
    assert!(
        unsafe { parsanol_mask_open(raw.as_ptr(), wrong.as_ptr(), vocab_c.as_ptr()) }.is_null()
    );
    assert!(last_error().contains("unknown entry"));

    // Vocabulary wiring bugs surface, never silently mask-drain.
    for bad in [
        r#"[]"#,
        r#"[{"id": 1}]"#,
        r#"[{"text": "ISBN"}]"#,
        r#"[{"id": 1, "text": "a"}, {"id": 1, "text": "b"}]"#,
        r#"not json"#,
    ] {
        let bad_c = CString::new(bad).expect("no NUL");
        assert!(
            unsafe { parsanol_mask_open(raw.as_ptr(), entry_c.as_ptr(), bad_c.as_ptr()) }.is_null(),
            "vocabulary {bad:?} must be rejected"
        );
    }

    // Freeing NULL is a no-op.
    unsafe { parsanol_mask_free(std::ptr::null_mut()) };
}

/// The validity loop: `generations` deterministic walks per grammar, every
/// output parse-checked by the artifact oracle — the receipt's scorer.
fn assert_receipt_validity(name: &str, entry: &str, vocab: &[(u64, &str)], generations: usize) {
    let artifact = PargArtifact::from_json(fixture(name)).expect("artifact loads");
    let oracle = TokenOracle::new(
        &artifact,
        entry,
        vocab
            .iter()
            .map(|(id, text)| Token {
                id: *id,
                text: (*text).to_string(),
            })
            .collect(),
    );
    let emitter = MaskEmitter::new(oracle);
    let lowest = |mask: &parsanol::constrain::TokenBitmask| -> u64 {
        (0..mask.vocab_size as u64)
            .find(|&id| mask.is_allowed(id))
            .expect("the mask is non-empty")
    };

    let (reference_text, reference_picked, complete) = emitter
        .generate(40, lowest)
        .expect("the rust loop does not dead-end");

    let mut valid = 0usize;
    let mut tokens = 0usize;
    for _ in 0..generations {
        let (text, picked) = c_generate(name, entry, vocab, 40);
        assert!(
            artifact.parse_shape(entry, &text).is_ok(),
            "{name}: the C-driven output must parse: {text:?}"
        );
        assert_eq!(picked, reference_picked, "{name}: same walk as the receipt");
        let replay: String = picked
            .iter()
            .filter_map(|id| {
                vocab
                    .iter()
                    .find(|(vid, _)| vid == id)
                    .map(|(_, text)| (*text).to_string())
            })
            .collect();
        assert_eq!(replay, text, "{name}: the picks concatenate to the output");
        assert!(complete, "{name}: the loop reached a full parse");
        valid += 1;
        tokens += picked.len();
    }
    println!("{name}: validity {valid}/{generations} ({tokens} tokens, {reference_text:?})");
    assert_eq!(valid, generations, "{name}: validity must be 100%");
}

#[test]
fn c_mask_loop_generates_valid_isbn() {
    assert_receipt_validity("isbn", "identifier", &isbn_vocab(), 200);
}

#[test]
fn c_mask_loop_generates_valid_json() {
    assert_receipt_validity("json", "main", &json_vocab(), 200);
}
