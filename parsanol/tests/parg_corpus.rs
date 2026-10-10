//! F4 corpus gate: every frozen corpus case replays identically on the
//! Rust engine — shape, bound map, and (where the grammar declares
//! them) rendered and derived strings — against the Ruby-generated
//! references in pubid-grammar/corpora.

use parsanol::PargArtifact;
use serde_json::Value;
use std::path::{Path, PathBuf};

fn workspace_dir(var: &str, leaf: &str) -> Option<(PathBuf, bool)> {
    if let Ok(from_env) = std::env::var(var) {
        return Some((PathBuf::from(from_env), true));
    }
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    for ancestor in manifest.ancestors().skip(1) {
        let candidate = ancestor.join("pubid").join("pubid-grammar").join(leaf);
        if candidate.is_dir() {
            return Some((candidate, false));
        }
    }
    None
}

#[test]
fn every_corpus_case_replays_identically() {
    let (Some((corpora, corpora_pinned)), Some((artifacts, _))) = (
        workspace_dir("PARG_CORPUS_DIR", "corpora"),
        workspace_dir("PARG_ARTIFACT_DIR", "artifacts"),
    ) else {
        eprintln!("skipping: pubid-grammar corpora/artifacts not present (set PARG_CORPUS_DIR/PARG_ARTIFACT_DIR)");
        return;
    };
    let mut checked = 0usize;
    for dir in std::fs::read_dir(&corpora).expect("corpora dir").flatten() {
        let corpus_path = dir.path().join("corpus.json");
        if !corpus_path.is_file() {
            continue;
        }
        let name = dir.file_name().to_str().expect("utf8 name").to_string();
        let corpus: Value = serde_json::from_str(&std::fs::read_to_string(&corpus_path).unwrap())
            .unwrap_or_else(|err| panic!("{name}: corpus unreadable: {err}"));
        let artifact = PargArtifact::from_path(artifacts.join(format!("{name}.json")))
            .unwrap_or_else(|err| panic!("{name}: artifact rejected: {err}"));
        let pinned = corpus["artifact_checksum"].as_str().unwrap();
        let actual = artifact.checksum().unwrap();
        if pinned != actual {
            if corpora_pinned {
                panic!("{name}: corpus pinned to a different artifact");
            }
            // An ambient sibling checkout (not the PARG_CORPUS_DIR pin)
            // drifts independently of this repository - CI pins the
            // dirs and still fails hard. Locally, say what to do.
            eprintln!(
                "skipping {name}: the sibling pubid-grammar checkout is stale \
                 (pinned {pinned}, built {actual}); refresh the checkout or set \
                 PARG_CORPUS_DIR/PARG_ARTIFACT_DIR"
            );
            continue;
        }
        let entry = artifact
            .default_entry()
            .unwrap_or(artifact.entry_names()[0])
            .to_string();
        for case in corpus["cases"].as_array().expect("cases array") {
            let input = case["input"].as_str().expect("case input");
            let kind = case["kind"].as_str().expect("case kind");
            let ctx = || format!("{name} {kind} {input:?}");
            match kind {
                "reject" => {
                    assert!(
                        artifact.parse_shape(&entry, input).is_err(),
                        "{}: expected rejection",
                        ctx()
                    );
                }
                "accept" | "example" => {
                    let shape = artifact
                        .parse_shape(&entry, input)
                        .unwrap_or_else(|err| panic!("{}: {err}", ctx()));
                    assert_eq!(
                        &shape,
                        &case["parsanol_tree"],
                        "{}: shape diverges from Ruby",
                        ctx()
                    );
                    let bound = artifact
                        .apply_bindings(&entry, &shape)
                        .unwrap_or_else(|err| panic!("{}: {err}", ctx()));
                    assert_eq!(
                        &bound,
                        &case["bound"],
                        "{}: bound map diverges from Ruby",
                        ctx()
                    );
                    if let Some(rendered) = case.get("rendered") {
                        for (variant, expected) in rendered.as_object().expect("rendered object") {
                            let got = artifact
                                .render_string(&entry, input, variant)
                                .unwrap_or_else(|err| panic!("{}: {err}", ctx()));
                            assert_eq!(
                                got,
                                expected.as_str().expect("rendered string"),
                                "{}: render {variant:?} diverges from Ruby",
                                ctx()
                            );
                        }
                    }
                    if let Some(derived) = case.get("derived") {
                        for (derive_name, expected) in derived.as_object().expect("derived object")
                        {
                            let got = artifact
                                .derive_string(&entry, input, derive_name)
                                .unwrap_or_else(|err| panic!("{}: {err}", ctx()));
                            assert_eq!(
                                got,
                                expected.as_str().expect("derived string"),
                                "{}: derive {derive_name:?} diverges from Ruby",
                                ctx()
                            );
                        }
                    }
                }
                other => panic!("{}: unknown corpus kind {other:?}", ctx()),
            }
        }
        checked += 1;
    }
    assert!(checked >= 40, "expected the full corpus set, ran {checked}");
}
