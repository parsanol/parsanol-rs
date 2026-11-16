//! Recovery spine, layer 2 (rs#539): the spine walk must pick the
//! grammar's list structure — not char-run repetitions (`1*digit`
//! inside a number rule, a string's interior run) which "match" at
//! every offset and stall the rounds byte-by-byte; member iterations
//! must consume whole members (not stop at the key); declared resyncs
//! engage for the spine's rule through the artifact's rule-atom table;
//! the rounds cover the clean suffix after the last corruption.

use parsanol::parg::frontend::envelope::compile_source;
use parsanol::PargArtifact;

const TINY: &str = r#"
grammar Tiny version "1.0.0" edition 2 {
  ws = *( %x20 / %x09 )
  tiny = ws object ws
  value = object / string / number
  object = ( "{" ws [ member *( ws "," ws member ) ] ws "}" ) as object
  member = ( string ws ":" ws value ) as member
  string = ( %x22 *( %x20-21 / %x23-5B / %x5D-10FFFF ) %x22 ) as string
  number = ( [ %x2D ] int ) as number
  int = %x30 / ( %x31-39 *digit )
  digit = %x30-39
  entry main: tiny
}
"#;

const TINY_RESYNC: &str = r#"
grammar Tiny version "1.0.0" edition 2 {
  ws = *( %x20 / %x09 )
  tiny = ws object ws
  value = object / string / number
  object = ( "{" ws [ member *( ws "," ws member ) ] ws "}" ) as object
  member = ( string ws ":" ws value ) as member
  string = ( %x22 *( %x20-21 / %x23-5B / %x5D-10FFFF ) %x22 ) as string
  number = ( [ %x2D ] int ) as number
  int = %x30 / ( %x31-39 *digit )
  digit = %x30-39
  resync member = 1*( %x00-21 / %x23-2B / %x2D-5C / %x5E-7C / %x7E-10FFFF )
  entry main: tiny
}
"#;

fn tiny_artifact(source: &str) -> PargArtifact {
    let envelope = compile_source(source).expect("the grammar compiles");
    PargArtifact::from_json(&envelope.to_string()).expect("envelope must verify")
}

/// A doc of `"k0":0,"k1":1,…` members; the corruption is a stray `@`
/// inside one member's value region.
fn members_doc(members: usize, corrupt_at: usize) -> (String, usize) {
    let mut doc = String::from("{");
    for i in 0..members {
        if i > 0 {
            doc.push(',');
        }
        doc.push_str(&format!("\"k{i}\":{i}"));
    }
    doc.push('}');
    let site = corrupt_at.min(doc.len() - 2).max(1);
    doc.replace_range(site..site + 1, "@");
    (doc, site)
}

fn recovered_spans(envelope: &serde_json::Value) -> Vec<(usize, usize)> {
    envelope["recovered_spans"]
        .as_array()
        .map(|spans| {
            spans
                .iter()
                .map(|s| {
                    (
                        s["offset"].as_u64().unwrap_or(0) as usize,
                        s["end"].as_u64().unwrap_or(0) as usize,
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn envelope_for(source: &str, doc: &str) -> serde_json::Value {
    tiny_artifact(source)
        .parse_shape_recovering("main", doc)
        .expect("recovery itself succeeds")
}

#[test]
fn char_run_rules_do_not_hijack_the_spine() {
    // The trap: `int = %x30 / ( %x31-39 *digit )` puts an unbounded
    // char-run repetition on every number. The spine must still be the
    // member list, so one corruption site costs ONE skip round, not a
    // round per digit.
    let (doc, site) = members_doc(12, 40);
    let envelope = envelope_for(TINY, &doc);
    assert_eq!(envelope["ok"], serde_json::Value::Bool(false));
    let resyncs = envelope["resyncs"].as_array().cloned().unwrap_or_default();
    assert!(
        resyncs.len() <= 2,
        "the rounds must not stall byte-by-byte on the digit runs: \
         {resyncs:?}"
    );
    let spans = recovered_spans(&envelope);
    let last = spans.last().map(|(_, end)| *end).unwrap_or(0);
    assert!(
        last >= doc.len() - 2,
        "recovery must reach the document end (site {site}): last {last} of {}",
        doc.len()
    );
}

#[test]
fn member_iterations_consume_whole_members() {
    // The probe honesty guard: an iteration is `"," member` — comma
    // plus key AND `: value`. A prefix success (`,"k5"` without the
    // value) would leave tiny spans chained at every key boundary.
    let (doc, _site) = members_doc(12, 40);
    let envelope = envelope_for(TINY, &doc);
    let spans = recovered_spans(&envelope);
    assert!(!spans.is_empty(), "recovery must produce spans");
    for (start, end) in &spans {
        let (start, end) = (*start, *end);
        let text = &doc[start.min(doc.len())..end.min(doc.len())];
        // Every recovered member span carries the `:` separator (the
        // whole member), not just the key string.
        let member_text = text.trim_start_matches(',');
        assert!(
            member_text.contains(':'),
            "recovered span {start}..{end} is a prefix member ({text:?})"
        );
    }
}

#[test]
fn non_root_resync_declaration_falls_back_and_still_recovers() {
    // `resync member` is declared on a NON-root rule. The per-rule
    // envelope resolution is parked (a rule->index table is
    // order-dependent across compilers — both emit duplicate Entity
    // copies per rule; see the mirror/envelope-members analysis), so
    // the entry carries no rule_atoms/resyncs map and the spine-rule
    // lookup falls through to the entry-level alias / the inferred
    // scan. Recovery must still cover the clean suffix either way.
    let artifact = tiny_artifact(TINY_RESYNC);
    let entry = artifact.envelope_value()["entries"]["main"].clone();
    assert!(
        entry.get("rule_atoms").is_none(),
        "the rule-atom wire is parked — a fallback regression here means          the resolution started reading a wire member that no compiler          emits"
    );
    let (doc, _site) = members_doc(12, 40);
    let envelope = artifact
        .parse_shape_recovering("main", &doc)
        .expect("recovery itself succeeds");
    let spans = recovered_spans(&envelope);
    let last = spans.last().map(|(_, end)| *end).unwrap_or(0);
    assert!(
        last >= doc.len() - 2,
        "recovery must reach the document end despite the parked wire:          last {last} of {}",
        doc.len()
    );
}

#[test]
fn clean_suffix_is_fully_covered_after_one_corruption() {
    // The suffix contract (rs#533): after the corruption site, the
    // clean remainder parses into recovered structure.
    let (doc, site) = members_doc(40, 60);
    let envelope = envelope_for(TINY, &doc);
    let spans = recovered_spans(&envelope);
    let suffix_total = doc.len() - site;
    let covered: usize = spans
        .iter()
        .map(|(start, end)| {
            let (start, end) = (*start, *end);
            end.saturating_sub(start.max(site))
        })
        .sum();
    let ratio = covered as f64 / suffix_total as f64;
    assert!(
        ratio > 0.9,
        "suffix coverage {ratio:.2} — the rounds must replay the clean \
         suffix (spans {spans:?})"
    );
}
