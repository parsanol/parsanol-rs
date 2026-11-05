//! plurimath#540 per-target replay: the raw-tree rows of a staged
//! plurimath-testsuite generation replay identically on the Rust engine.
//! Gated on PARG_ARTIFACT (latexmath's artifacts/latex.json) and
//! STAGE_PAYLOADS (a directory of a generation's raw-*.yaml payloads
//! converted to raw-*.json).

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
    let report = replay::replay_stage(&artifact, &payloads);
    assert!(!report.rows.is_empty(), "no raw-tree rows found");
    assert_eq!(report.differ(), 0, "{}", report.format());
}

#[test]
fn a_changed_payload_is_reported_as_differing() {
    let Some((artifact, payloads)) = gated() else {
        return;
    };
    let source = replay::payload_files(&payloads)
        .into_iter()
        .find(|f| {
            std::fs::read_to_string(f)
                .unwrap()
                .contains("\"byte_offset\":1,")
        })
        .expect("a payload with a slice at byte 1");
    let text = std::fs::read_to_string(&source).unwrap();
    let changed = text.replacen("\"byte_offset\":1,", "\"byte_offset\":99,", 1);
    let copy = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("raw-changed.json");
    std::fs::write(&copy, changed).unwrap();
    let report = replay::replay_stage(&artifact, &copy);
    assert_eq!(report.differ(), 1, "{}", report.format());
}
