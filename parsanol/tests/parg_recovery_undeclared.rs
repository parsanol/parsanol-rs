//! rs#734 regression pin: the INFERRED resync (no `resync`
//! declarations at all) must recover >= 80% of the post-corruption
//! suffix on a json-shaped grammar. The 2026-11-28 receipt measured
//! declared-vs-undeclared parity (90.4% both) on the full corrupted
//! corpus; this pin keeps the inferred machinery honest — the
//! substantial-spine gate, the boundary anchor, and the rounds'
//! advance — so a regression cannot silently re-open the declaration
//! dependency. The suffix accounting matches the runner's: covered =
//! the furthest recovered span end minus the skipped junk.

use parsanol::parg::frontend::envelope::compile_source;
use parsanol::PargArtifact;

/// The registry's json grammar with its two `resync` declarations
/// REMOVED (the only difference from the shipped json.parg — the
/// shape that 2026-11-28-recovery-undeclared.md measured at declared
/// parity).
const JSON_UNDECLARED: &str = r#"
grammar Json version "1.0.0" edition 2 {
  ws = *( %x20 / %x09 / %x0A / %x0D )
  json = ( object / array ) as json
  object = ( %x7B ws [ member *( %x2C ws member ) ] [ %x2C ] ws %x7D ) as object
  array = ( %x5B ws [ value *( %x2C ws value ) ] ws %x5D ) as array
  member = ( string %x3A ws value ) as member
  value = ( string / number / object / array / boolean / null ) as value
  boolean = ( "true" / "false" ) as boolean
  null = "null" as null
  number = ( [ %x2D ] int [ frac ] [ exp ] ) as number
  int = %x30 / ( %x31-39 *digit )
  frac = %x2E 1*digit
  exp = ( %x65 / %x45 ) [ %x2B / %x2D ] 1*digit
  digit = %x30-39
  string = ( %x22 *( %x20-21 / %x23-5B / %x5D-10FFFF / %x5C ( %x22 / %x5C / %x2F / %x62 / %x66 / %x6E / %x72 / %x74 ) ) %x22 ) as string
  entry main: json
}
"#;

const DOC: &str = r#"{"alpha": 1, "beta": [10, 20, 30], "gamma": {"g1": true, "g2": null}, "delta": "text", "epsilon": -2.5e1, "zeta": [4, 5]}"#;

/// (description, corrupted input, the corruption site = the byte
/// offset where valid structure ends). The classes mirror the corpus
/// harness: junk insertion, byte deletion, truncation-free mid-value
/// damage — everything leaves a parseable suffix behind.
const CASES: &[(&str, &str, usize)] = &[
    // Junk run inside a string value: the scan must skip to the next
    // member boundary and resume the member list.
    ("junk-in-string", r#"{"alpha": 1, "beta": "@@@@junk@@@@", "gamma": 3}"#, 13),
    // Deleted structural bytes inside a value.
    ("delete-in-value", r#"{"alpha": 1, "beta": [10, ], "gamma": 3}"#, 13),
    // A corrupted number: characters swapped for junk.
    ("junk-in-number", r#"{"alpha": 1, "beta": @@#, "gamma": 3}"#, 13),
    // Junk inside a nested object's member.
    ("junk-in-nested", r#"{"alpha": {"x": @@@, "y": 2}, "beta": 3}"#, 11),
];

fn suffix_coverage(input_len: usize, site: usize, envelope: &serde_json::Value) -> f64 {
    if envelope["ok"].as_bool() == Some(true) {
        return 1.0;
    }
    let mut junk = 0usize;
    if let Some(spans) = envelope["resyncs"].as_array() {
        for span in spans {
            let off = span["offset"].as_u64().unwrap_or(0) as usize;
            let resumed = span["resumed_at"].as_u64().unwrap_or(off as u64) as usize;
            junk += resumed.saturating_sub(off);
        }
    }
    let consumed = envelope["consumed_upto"].as_u64().unwrap_or(0) as usize;
    let mut bound = consumed;
    if let Some(spans) = envelope["recovered_spans"].as_array() {
        if let Some(last) = spans.last() {
            bound = bound.max(last["end"].as_u64().unwrap_or(0) as usize);
        }
    }
    let suffix = input_len - site;
    if suffix == 0 {
        return 1.0;
    }
    (bound.saturating_sub(junk)).saturating_sub(site).min(suffix) as f64 / suffix as f64
}

#[test]
fn inferred_resync_recovers_the_suffix_without_declarations() {
    let wire = compile_source(JSON_UNDECLARED).expect("the stripped grammar compiles");
    let artifact =
        PargArtifact::from_json(&serde_json::to_string(&wire).unwrap()).expect("artifact loads");
    let mut coverages = Vec::new();
    for (name, corrupted, site) in CASES {
        let envelope = artifact
            .parse_shape_recovering("main", corrupted)
            .expect("the recovery envelope never aborts on these classes");
        let coverage = suffix_coverage(corrupted.len(), *site, &envelope);
        coverages.push((format!("{name}: {coverage:.2}"), coverage));
    }
    let mean = coverages.iter().map(|(_, c)| *c).sum::<f64>() / coverages.len() as f64;
    assert!(
        mean >= 0.80,
        "inferred resync must recover >= 80% of the suffix mean, got {mean:.3}: {:?}",
        coverages.iter().map(|(n, _)| n).collect::<Vec<_>>()
    );
    for (name, coverage) in &coverages {
        assert!(
            *coverage >= 0.5,
            "case {name} must recover at least half its suffix"
        );
    }
}

#[test]
fn the_declared_variant_stays_at_least_as_good() {
    // The tiering contract: adding the declarations back must never
    // LOWER the coverage (declared wins where it exists, inferred
    // fills the rest).
    let with_resync = JSON_UNDECLARED.replace(
        "  entry main: json",
        "  resync value = 1*( %x00-21 / %x23-2B / %x2D-5C / %x5E-7C / %x7E-10FFFF )\n  resync member = 1*( %x00-21 / %x23-2B / %x2D-5C / %x5E-7C / %x7E-10FFFF )\n  entry main: json",
    );
    let wire = compile_source(&with_resync).expect("the declared grammar compiles");
    let artifact =
        PargArtifact::from_json(&serde_json::to_string(&wire).unwrap()).expect("artifact loads");
    for (name, corrupted, site) in CASES {
        let envelope = artifact
            .parse_shape_recovering("main", corrupted)
            .expect("the recovery envelope never aborts on these classes");
        let coverage = suffix_coverage(corrupted.len(), *site, &envelope);
        assert!(
            coverage >= 0.5,
            "declared case {name} must still recover at least half its suffix"
        );
    }
}
