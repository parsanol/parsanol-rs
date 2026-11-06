//! plurimath#540 per-target replay: the raw-tree rows of a staged
//! plurimath-testsuite generation replay identically on the Rust engine.
//! Gated on PARG_ARTIFACT (the stage gem's artifact, e.g. latexmath's
//! artifacts/latex.json) and STAGE_PAYLOADS (a directory of a generation's
//! *.yaml payloads converted to *.json); STAGE_LANGUAGE names the stage
//! language (default latex).

#[path = "../../examples/stage-replay/languages.rs"]
mod languages;
#[path = "../../examples/stage-replay/replay.rs"]
mod replay;

use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;

fn gated() -> Option<(PathBuf, PathBuf)> {
    match (
        std::env::var("PARG_ARTIFACT"),
        std::env::var("STAGE_PAYLOADS"),
    ) {
        (Ok(a), Ok(p)) => Some((a.into(), p.into())),
        _ => {
            eprintln!("skipping: set PARG_ARTIFACT and STAGE_PAYLOADS");
            None
        }
    }
}

fn language() -> String {
    std::env::var("STAGE_LANGUAGE").unwrap_or_else(|_| "latex".to_string())
}

#[test]
fn the_umath_preprocessing_port_matches_the_ruby_preprocessor() {
    // Umath::Preprocessor.call(input).text at umath 0302568.
    for (input, expected) in [
        ("\u{2061}", Some("&#x2061;")),
        ("a\u{2061}", Some("a&#x2061;")),
        ("\u{3b1}\u{2061}", Some("&#x3b1;&#x2061;")),
        ("\u{1d44e}", Some("&#x1d44e;")),
        (" a&b\"c\" ", Some("a&b\"c\"")),
        ("x#label", Some("x")),
        ("#", None),
        ("\u{270e}(a#b)", Some("&#x270e;(a#b)")),
        ("a\u{2af7}note\u{2af8}b", Some("ab")),
        ("\\\\x", Some("\\x")),
        ("\\u03b1", Some("&#x03b1;")),
    ] {
        assert_eq!(
            languages::umath_preprocess(input).as_deref(),
            expected,
            "{input:?}"
        );
    }
}

#[test]
fn canonical_form_matches_the_ruby_dump() {
    let shape = json!({"row_break": {"value": "é", "offset": 3, "length": 1, "line": 1, "column": 3},
                       "n": null, "items": {"tag": "repetition", "items": [true, 3]}});
    let renames = HashMap::from([("row_break".to_string(), "\\\\".to_string())]);
    let dumped = replay::dump_canonical(&shape, "aéé", &renames);
    let expected = json!({"hash": [
        [{"sym": "items"}, {"array": [{"bool": true}, {"int": "3"}]}],
        [{"sym": "n"}, {"nil": true}],
        [{"str": "\\\\"}, {"slice": "é", "byte_offset": 3, "char_offset": 2}],
    ]});
    assert!(replay::canonical_equal(&dumped, &expected), "{dumped}");
}

#[test]
fn every_staged_raw_row_replays_identically() {
    let Some((artifact, payloads)) = gated() else {
        return;
    };
    let report = replay::replay_stage(&artifact, &payloads, &language());
    assert!(!report.rows.is_empty(), "no raw-tree rows found");
    assert_eq!(report.differ(), 0, "{}", report.format());
}

#[test]
fn a_changed_payload_is_reported_as_differing() {
    let Some((artifact, payloads)) = gated() else {
        return;
    };
    // The first slice of a raw-tree payload: its byte offset moved by 99.
    const KEY: &str = "\"byte_offset\":";
    let first_slice = |text: &str| {
        let at = text.find(",\"char_offset\"")?;
        let start = text[..at].rfind(KEY)? + KEY.len();
        Some((start, at))
    };
    let (text, (start, end)) = replay::payload_files(&payloads)
        .into_iter()
        .find_map(|f| {
            let text = std::fs::read_to_string(f).unwrap();
            if !text.contains("\"boundary\":\"raw-tree\"") {
                return None;
            }
            let span = first_slice(&text)?;
            Some((text, span))
        })
        .expect("a raw-tree payload with a slice");
    let offset: u64 = text[start..end].parse().expect("a byte offset");
    let changed = format!("{}{}{}", &text[..start], offset + 99, &text[end..]);
    let copy = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("raw-changed.json");
    std::fs::write(&copy, changed).unwrap();
    let report = replay::replay_stage(&artifact, &copy, &language());
    assert_eq!(report.differ(), 1, "{}", report.format());
}
