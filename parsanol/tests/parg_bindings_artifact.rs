//! The declared per-target binding artifact (parsanol-ruby#432): the
//! Ruby-emitted fixture replays byte-for-byte — the rs loader verifies
//! each target artifact, applies the declared records through the same
//! apply core as the embedded ones, and re-emits payloads identical to
//! the reference compiler's.

use parsanol::parg::bindings_artifact::{bindings_envelope, PargBindingsArtifact, BINDING_TARGETS};
use parsanol::PargArtifact;
use serde_json::Value;

fn fixture() -> Value {
    let text = include_str!("fixtures/parg_binding_artifacts.json");
    serde_json::from_str(text).expect("fixture parses")
}

#[test]
fn ruby_emitted_artifacts_verify_and_replay() {
    for case in fixture()["cases"].as_array().unwrap() {
        let envelope_text = serde_json::to_string(&case["envelope"]).unwrap();
        let artifact = PargArtifact::from_json(&envelope_text)
            .unwrap_or_else(|err| panic!("{}: envelope rejected: {err}", case["grammar"]));
        let entry = case["entry"].as_str().unwrap();
        let shape = &case["shape"];
        for target in BINDING_TARGETS {
            let declared =
                PargBindingsArtifact::from_value(case["artifacts"][target].clone(), Some(target))
                    .unwrap_or_else(|err| panic!("{}[{target}]: rejected: {err}", case["grammar"]));
            assert_eq!(declared.target(), target);
            assert_eq!(
                declared.source_hash(),
                artifact.source_hash().unwrap_or(""),
                "{}[{target}]: source hash must pin the envelope (or both predate #209)",
                case["grammar"]
            );
            let bound = declared
                .apply(&artifact, entry, shape)
                .unwrap_or_else(|err| panic!("{}[{target}]: apply failed: {err}", case["grammar"]));
            assert_eq!(
                &bound, &case["bound"],
                "declared bindings diverge from the recorded bound map for {} [{target}] / {:?}",
                case["grammar"], case["input"]
            );
        }
    }
}

#[test]
fn rs_emission_matches_the_ruby_reference_byte_for_byte() {
    for case in fixture()["cases"].as_array().unwrap() {
        for target in BINDING_TARGETS {
            let emitted = bindings_envelope(&case["envelope"], target)
                .unwrap_or_else(|err| panic!("emission failed for {target}: {err}"));
            assert_eq!(
                &emitted, &case["artifacts"][target],
                "rs-emitted {target} artifact diverges from the Ruby reference for {}",
                case["grammar"]
            );
        }
    }
}

#[test]
fn declared_application_matches_embedded_records() {
    for case in fixture()["cases"].as_array().unwrap() {
        let envelope_text = serde_json::to_string(&case["envelope"]).unwrap();
        let artifact = PargArtifact::from_json(&envelope_text).unwrap();
        let entry = case["entry"].as_str().unwrap();
        let shape = &case["shape"];
        let embedded = artifact.apply_bindings(entry, shape).unwrap();
        assert_eq!(
            &embedded, &case["bound"],
            "{}: embedded path moved",
            case["grammar"]
        );
    }
}

#[test]
fn tampered_records_fail_the_checksum() {
    let fixture = fixture();
    // Pick a case whose entry actually declares records (some C7
    // grammars bind nothing on their entry).
    let case = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| {
            case["artifacts"]["rs"]["entries"]
                .as_object()
                .unwrap()
                .values()
                .any(|slot| {
                    slot["bindings"]
                        .as_array()
                        .is_some_and(|records| !records.is_empty())
                })
        })
        .expect("a case with declared records");
    let mut payload = case["artifacts"]["rs"].clone();
    let object = payload.as_object_mut().unwrap();
    let entries = object.get_mut("entries").unwrap().as_object_mut().unwrap();
    let entry = entries
        .values_mut()
        .find(|slot| {
            slot["bindings"]
                .as_array()
                .is_some_and(|records| !records.is_empty())
        })
        .unwrap()
        .as_object_mut()
        .unwrap();
    let records = entry.get_mut("bindings").unwrap().as_array_mut().unwrap();
    let record = records[0].as_object_mut().unwrap();
    record.insert("path".to_string(), Value::String("smuggled".to_string()));

    let err = PargBindingsArtifact::from_value(payload, None).unwrap_err();
    assert!(
        err.to_string().contains("checksum mismatch"),
        "expected a checksum mismatch, got: {err}"
    );
}

#[test]
fn target_pin_rejects_a_foreign_artifact() {
    let case = &fixture()["cases"][0];
    let err =
        PargBindingsArtifact::from_value(case["artifacts"]["ts"].clone(), Some("rs")).unwrap_err();
    assert!(
        err.to_string().contains(r#"targets "ts", not "rs""#),
        "expected a target mismatch, got: {err}"
    );
}

#[test]
fn pairing_rejects_a_foreign_grammar_hash() {
    let case = &fixture()["cases"][0];
    // A self-consistent artifact whose source hash pins a DIFFERENT
    // grammar text: verification passes, pairing must refuse.
    let mut payload = case["artifacts"]["python"].clone();
    payload.as_object_mut().unwrap().insert(
        "grammar_source_hash".to_string(),
        Value::String("0".repeat(64)),
    );
    let mut stripped = payload.clone();
    stripped.as_object_mut().unwrap().remove("checksum");
    let checksum = parsanol::parg::checksum_hex(&stripped).unwrap();
    payload
        .as_object_mut()
        .unwrap()
        .insert("checksum".to_string(), Value::String(checksum));
    let declared = PargBindingsArtifact::from_value(payload, None).unwrap();
    let envelope_text = serde_json::to_string(&case["envelope"]).unwrap();
    let artifact = PargArtifact::from_json(&envelope_text).unwrap();
    let err = declared
        .apply(&artifact, case["entry"].as_str().unwrap(), &case["shape"])
        .unwrap_err();
    assert!(
        err.to_string().contains("source_hash"),
        "expected a pairing mismatch, got: {err}"
    );
}
