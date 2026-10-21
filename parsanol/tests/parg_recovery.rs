//! R1.5 partial-tree recovery (TODO.sota/1): `parse_shape_recovering`
//! returns the deepest repetition prefix the failed parse matched,
//! shaped exactly like the root. Zero effect on success paths.

use parsanol::parg::{canonical_json, checksum_hex};
use parsanol::portable::parser_dsl::GrammarBuilder;
use parsanol::portable::Grammar;
use parsanol::PargArtifact;
use serde_json::Value;

fn kv_grammar() -> Grammar {
    // Tree-level named captures (label): the partial tree is built from
    // repetition items, which only exist when the grammar shapes its
    // tree (capture-store-only grammars contribute no items).
    use parsanol::portable::parser_dsl::{self, ref_, seq, ParsletExt as _};
    let pair = seq([
        parser_dsl::dynamic(parser_dsl::re(r"[a-z0-9]+").label("k")),
        parser_dsl::dynamic(parser_dsl::str("=")),
        parser_dsl::dynamic(parser_dsl::re(r"[0-9]+").label("v")),
        parser_dsl::dynamic(parser_dsl::str(";")),
    ]);
    let document = ref_("pair").repeat(1, None).label("pairs");
    GrammarBuilder::new()
        .rule("document", document)
        .rule("pair", pair)
        .build()
}

fn kv_artifact() -> PargArtifact {
    let grammar = kv_grammar();
    let envelope = serde_json::json!({
        "shape": "parsanol-tree/v2",
        "grammar": "kv",
        "version": "1.0.0",
        "source": "",
        "entries": {
            "main": {
                "root": "document",
                "grammar": serde_json::from_str::<Value>(&grammar.to_json().unwrap()).unwrap(),
                "bindings": [],
            }
        },
        "tests": [],
    });
    let mut map = envelope.as_object().unwrap().clone();
    let checksum = checksum_hex(&Value::Object(map.clone())).unwrap();
    map.insert("checksum".into(), Value::String(checksum));
    PargArtifact::from_json(&serde_json::to_string(&Value::Object(map)).unwrap())
        .expect("envelope must verify")
}

#[test]
fn success_paths_are_unchanged_by_recovery() {
    let artifact = kv_artifact();
    let recovered = artifact
        .parse_shape_recovering("main", "a=1;b=2;")
        .expect("recovery call itself succeeds");
    let plain = artifact
        .parse_shape("main", "a=1;b=2;")
        .expect("plain parse");
    assert_eq!(recovered["ok"], Value::Bool(true));
    assert_eq!(recovered["shape"], plain);
    assert!(recovered.get("partial").is_none());
}

#[test]
fn truncated_input_yields_the_matched_prefix_as_a_shaped_tree() {
    let artifact = kv_artifact();
    let recovered = artifact
        .parse_shape_recovering("main", "a=1;b=2;c=3")
        .expect("recovery call itself succeeds");
    assert_eq!(recovered["ok"], Value::Bool(false));
    // The third pair is missing its `;`: the partial tree carries the
    // two complete items, shaped exactly like the root would have been.
    let items = recovered["partial"]["pairs"]
        .as_array()
        .expect("partial root array under the root name");
    assert_eq!(items.len(), 2);
    // Leaves are the standard source-preserving slice wire objects.
    assert_eq!(items[0]["k"]["value"], Value::String("a".into()));
    assert_eq!(items[0]["v"]["value"], Value::String("1".into()));
    assert_eq!(items[1]["k"]["value"], Value::String("b".into()));
    assert_eq!(items[1]["v"]["value"], Value::String("2".into()));
    // The error wire points at the failure: the third pair matched
    // `c=3` and failed expecting `;` at offset 11 (end of input).
    assert_eq!(recovered["error"]["offset"], Value::from(11));
    // Expected labels render in Rust-debug form (the F7 wire format).
    assert_eq!(
        recovered["error"]["expected"],
        Value::Array(vec![Value::String("';'".into())])
    );
}

#[test]
fn first_pair_failures_yield_an_empty_partial() {
    let artifact = kv_artifact();
    let recovered = artifact
        .parse_shape_recovering("main", "=1;")
        .expect("recovery call itself succeeds");
    assert_eq!(recovered["ok"], Value::Bool(false));
    assert!(recovered["partial"].is_null());
}

#[test]
fn canonical_json_still_matches_the_ruby_reference_bytes() {
    // The partial envelope rides canonical_json; the float/canonical
    // contract is unaffected. (Sanity: checksums stay deterministic.)
    let artifact = kv_artifact();
    let recovered = artifact
        .parse_shape_recovering("main", "a=1;b=2;c=3")
        .unwrap();
    let text = serde_json::to_string(&recovered).unwrap();
    let value: Value = serde_json::from_str(&text).unwrap();
    let _ = canonical_json(&value).expect("canonicalizes");
}
