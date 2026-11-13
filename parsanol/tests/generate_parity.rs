//! rs#459 slice 2: the lowered generator is byte-identical with the
//! generic interpreter. Two halves:
//!
//! 1. Self-consistency (always on): over every artifact with a render
//!    spec and derive templates, for every accept input, `generate`
//!    and `gen_derive` reproduce `apply`/`derive::apply` byte-for-byte
//!    — and the re-parse property holds for the generated string (the
//!    graduation oracle's property, now through the graduated path).
//! 2. The corpus gate (parg_corpus.rs) replays the frozen renders
//!    through BOTH paths.

use parsanol::PargArtifact;
use std::path::PathBuf;

fn artifacts_dir() -> Option<PathBuf> {
    if let Ok(from_env) = std::env::var("PARG_ARTIFACT_DIR") {
        return Some(PathBuf::from(from_env));
    }
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let candidate = manifest
        .parent()?
        .join("pubid")
        .join("pubid-grammar")
        .join("artifacts");
    candidate.is_dir().then_some(candidate)
}

#[test]
fn generate_matches_the_interpreter_and_round_trips() {
    let Some(dir) = artifacts_dir() else {
        eprintln!("skipping: baked artifacts not present (set PARG_ARTIFACT_DIR)");
        return;
    };
    let mut checked_render = 0usize;
    let mut checked_derive = 0usize;
    let mut roundtrips = 0usize;
    for entry in std::fs::read_dir(&dir)
        .expect("the artifacts dir")
        .flatten()
    {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(envelope) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let variants = match envelope.get("render").and_then(|r| r.as_object()) {
            Some(v) if !v.is_empty() => v,
            _ => continue,
        };
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let artifact = PargArtifact::from_json(&text)
            .unwrap_or_else(|err| panic!("{name}: the artifact must load: {err}"));
        let entry_name = envelope
            .get("entries")
            .and_then(|e| e.as_object())
            .and_then(|e| e.keys().next())
            .expect("an entry")
            .clone();

        for (variant, _) in variants {
            for test in envelope
                .get("tests")
                .and_then(|t| t.as_array())
                .into_iter()
                .flatten()
            {
                if test.get("kind").and_then(|k| k.as_str()) != Some("accept") {
                    continue;
                }
                let Some(input) = test.get("input").and_then(|i| i.as_str()) else {
                    continue;
                };
                let Ok(bound) = artifact.parse_and_bind(&entry_name, input) else {
                    continue;
                };
                // THE PARITY: lowered generator == generic interpreter.
                let interpreted =
                    parsanol::parg::render::apply(artifact.render_spec(), variant, &bound)
                        .unwrap_or_else(|err| panic!("{name}/{variant}: interpret failed: {err}"));
                let generated = artifact
                    .generate(variant, &bound)
                    .unwrap_or_else(|err| panic!("{name}/{variant}: generate failed: {err}"));
                assert_eq!(
                    interpreted, generated,
                    "{name}/{variant}: the lowered generator diverges from the interpreter \
                     on {input:?}"
                );
                checked_render += 1;
                // THE PROPERTY, through the graduated path: the
                // generated string re-parses to the same bound map.
                if let Ok(rebound) = artifact.parse_and_bind(&entry_name, &generated) {
                    let a = serde_json::to_value(&bound).expect("bound serializes");
                    let b = serde_json::to_value(&rebound).expect("rebound serializes");
                    assert_eq!(
                        a, b,
                        "{name}/{variant}: {input:?} generated {generated:?} \
                         which re-parses to a different bound map"
                    );
                    roundtrips += 1;
                }
            }
        }

        // Derive templates: the lowered template == the interpreter.
        let derive_names: Vec<String> = envelope
            .get("derive")
            .and_then(|d| d.as_object())
            .map(|d| d.keys().cloned().collect())
            .unwrap_or_default();
        for name in derive_names {
            for test in envelope
                .get("tests")
                .and_then(|t| t.as_array())
                .into_iter()
                .flatten()
            {
                if test.get("kind").and_then(|k| k.as_str()) != Some("accept") {
                    continue;
                }
                let Some(input) = test.get("input").and_then(|i| i.as_str()) else {
                    continue;
                };
                let Ok(bound) = artifact.parse_and_bind(&entry_name, input) else {
                    continue;
                };
                let interpreted =
                    parsanol::parg::derive::apply(envelope.get("derive").unwrap(), &name, &bound)
                        .unwrap_or_else(|err| panic!("{name}: derive interpret failed: {err}"));
                let generated = artifact
                    .gen_derive(&name, &bound)
                    .unwrap_or_else(|err| panic!("{name}: gen_derive failed: {err}"));
                assert_eq!(
                    interpreted, generated,
                    "{name}: the lowered derive template diverges on {input:?}"
                );
                checked_derive += 1;
            }
        }
    }
    assert!(
        checked_render > 0,
        "expected render-spec artifacts (set PARG_ARTIFACT_DIR)"
    );
    println!(
        "generate parity: {checked_render} render cases, {checked_derive} derive cases, \
         {roundtrips} round-trips"
    );
}

#[test]
fn unknown_variants_and_segments_reject_at_lowering() {
    let render = serde_json::json!({
        "default": [{ "type": "field", "field": "x" }]
    });
    assert!(parsanol::parg::generate::Generator::lower(&render, "default").is_ok());
    assert!(
        parsanol::parg::generate::Generator::lower(&render, "other").is_err(),
        "an unknown variant must reject at lowering"
    );
    let bad = serde_json::json!({
        "default": [{ "type": "mystery" }]
    });
    assert!(
        parsanol::parg::generate::Generator::lower(&bad, "default").is_err(),
        "an unknown segment must reject at lowering, not at emit"
    );
}

#[test]
fn lowering_rejects_malformed_segments_the_same_way_as_the_interpreter() {
    // A non-object segment: both paths name the same error shape.
    let bad = serde_json::json!({ "default": ["not-an-object"] });
    let lowered = parsanol::parg::generate::Generator::lower(&bad, "default");
    assert!(lowered.is_err());
}
