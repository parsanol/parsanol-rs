//! rs#334: the VM keeps a repetition iteration's partial prefix on
//! failure. `top = pq or pq` with `and = not *( pq "AND" pq not )`:
//! for "a AND" the star's second iteration consumes " AND" and then
//! fails inside `not`; the whole iteration must roll back (the Ruby
//! atoms do), leaving "AND" unconsumed — the parse must fail.

use parsanol::PargArtifact;

#[test]
fn rep_iteration_rolls_back_on_failure() {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures_bool334.json")).expect("artifact");
    let artifact = PargArtifact::from_json(&text).expect("artifact loads");
    assert!(
        artifact.parse_shape("main", "a AND").is_err(),
        "\"a AND\" must not parse (the star's failing iteration must roll back)"
    );
    assert!(artifact.parse_shape("main", "a AND b").is_ok());
    assert!(artifact.parse_shape("main", "a OR b").is_ok());
    assert!(artifact.parse_shape("main", "(a AND b) OR c").is_ok());
}
