//! rs#172 parity gate: grammar-declared skip (trivia injection).
//! Fixture generated from Ruby (`/tmp/gen_skip_fixture.rb`): the
//! envelope's atoms carry the injected trivia — `Ignore(Repetition(0,1,
//! skip))` before every terminal, leading/trailing skips on entries —
//! so any engine that accepts the atoms implements injection. This
//! replays the same accept/reject suite through the portable engine;
//! Ruby's runner (interpreter and native modes) is green on the same
//! envelope, so failures here are parity failures.

use parsanol::PargArtifact;
use serde_json::Value;

fn fixture_envelope() -> String {
    let text = include_str!("fixtures/parg_skip.json");
    let fixture: Value = serde_json::from_str(text).expect("fixture is valid JSON");
    serde_json::to_string(&fixture["envelope"]).unwrap()
}

#[test]
fn loads_a_skip_artifact_and_declares_its_trivia_rule() {
    let artifact = PargArtifact::from_json(&fixture_envelope()).expect("skip artifact must verify");
    let envelope: Value = serde_json::from_str(&fixture_envelope()).unwrap();
    assert_eq!(envelope["skip"], "trivia");
    assert!(artifact.entry_names().contains(&"document"));
}

#[test]
fn replays_the_ruby_accept_reject_suite() {
    let artifact = PargArtifact::from_json(&fixture_envelope()).expect("skip artifact must verify");
    let failures = artifact.run_tests();
    assert!(
        failures.is_empty(),
        "skip parity failures (Ruby is green on the same envelope): {failures:?}"
    );
}
