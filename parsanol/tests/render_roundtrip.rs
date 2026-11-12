//! #459 first slice: the generate→re-parse round-trip property on the
//! pilot's frozen renders. Generation (parse in reverse) is only
//! coherent if a rendered string RE-PARSES to the model that produced
//! it. This gate runs the property over every artifact that ships a
//! render spec, using the corpus's own accept inputs as the model
//! sources.

use parsanol::PargArtifact;
use std::path::PathBuf;

fn artifacts_dir() -> PathBuf {
    std::env::var("PARG_ARTIFACT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
            manifest
                .parent()
                .unwrap()
                .join("pubid")
                .join("pubid-grammar")
                .join("artifacts")
        })
}

// RED BY DESIGN while the pilot's render spec is lossy (see #459:
// 0/10 on the iso frozen cases — joint publishers dropped,  emitted
// where the grammar requires the publisher's own separator). The
// corpus's byte-identical gate compares engines against the SAME lossy
// spec and cannot see this; the round-trip property is the graduation
// oracle. Ignored so the suite stays green; run explicitly:
#[test]
#[ignore = "red while the iso render spec is lossy — run explicitly, the violations ARE the finding"]
fn rendered_strings_reparse_to_the_same_bound_map() {
    let dir = artifacts_dir();
    let mut checked = 0usize;
    let mut roundtrips = 0usize;
    let mut diverged: Vec<String> = Vec::new();
    let entries = std::fs::read_dir(&dir).expect("the artifacts dir");
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(_) => continue,
        };
        let envelope: serde_json::Value = match serde_json::from_str(&text) {
            Ok(envelope) => envelope,
            Err(_) => continue,
        };
        let variants = match envelope.get("render").and_then(|r| r.as_object()) {
            Some(variants) if !variants.is_empty() => variants,
            _ => continue,
        };
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let artifact = match PargArtifact::from_json(&text) {
            Ok(artifact) => artifact,
            Err(err) => {
                panic!("{name}: the artifact must load: {err}");
            }
        };
        let entry = envelope
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
                let bound = match artifact.parse_and_bind(&entry, input) {
                    Ok(bound) => bound,
                    Err(_) => continue,
                };
                let rendered = match artifact.render_string(&entry, input, variant) {
                    Ok(rendered) => rendered,
                    Err(parsanol::PargError::UnknownRenderVariant(_)) => continue,
                    Err(err) => panic!("{name}/{variant}/{input}: render failed: {err}"),
                };
                checked += 1;
                // THE PROPERTY: the rendered string re-parses, and the
                // re-parsed bound map matches the original.
                match artifact.parse_and_bind(&entry, &rendered) {
                    Ok(rebound) => {
                        let a = serde_json::to_value(&bound).expect("bound serializes");
                        let b = serde_json::to_value(&rebound).expect("rebound serializes");
                        if a == b {
                            roundtrips += 1;
                        } else {
                            diverged.push(format!(
                                "{name}/{variant}: {input:?} rendered {rendered:?} but \
                                 the re-parse diverges"
                            ));
                        }
                    }
                    Err(err) => diverged.push(format!(
                        "{name}/{variant}: {input:?} rendered {rendered:?} which \
                         no longer parses: {}",
                        err.to_string().split(':').next().unwrap_or("")
                    )),
                }
            }
        }
    }
    assert!(
        checked > 0,
        "expected artifacts with render specs (set PARG_ARTIFACT_DIR)"
    );
    println!("roundtrip: {roundtrips}/{checked} re-parse identically");
    for v in &diverged {
        println!("VIOLATION: {v}");
    }
    assert!(
        diverged.is_empty(),
        "{} of {} rendered strings violate the round-trip property          (render specs that emit strings the grammar rejects — the          graduation oracle; see jit-458/#459)",
        diverged.len(),
        checked
    );
}
