//! Conformance corpus gate (parsanol-tree/v2): every case in
//! corpus/conformance runs its embedded test section on this engine and
//! must pass — zero failures. The suite pins cross-engine behavioral
//! alignment (trees, bindings, preprocess, skip injection, reject
//! shapes); see corpus/conformance/README.md for the contract and the
//! authoring rules.

use parsanol::PargArtifact;
use std::path::{Path, PathBuf};

fn conformance_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join("corpus")
        .join("conformance")
}

#[test]
fn conformance_cases_all_pass() {
    let dir = conformance_dir();
    if !dir.is_dir() {
        // Downstream builds do not package the workspace corpus; the
        // gate is enforced in parsanol-rs CI, where it always exists.
        eprintln!("skipping: corpus/conformance not present outside the workspace");
        return;
    }
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .expect("conformance dir present")
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension()?.to_str()? != "json" {
                return None;
            }
            path.file_stem()?.to_str().map(str::to_string)
        })
        .collect();
    names.sort();
    assert!(
        !names.is_empty(),
        "conformance corpus is empty — nothing pins engine behavior"
    );

    for name in &names {
        let artifact = PargArtifact::from_path(dir.join(format!("{name}.json")))
            .unwrap_or_else(|err| panic!("{name}: artifact rejected: {err}"));
        let failures = artifact.run_tests();
        assert!(
            failures.is_empty(),
            "{name}: conformance failures: {failures:?}"
        );
    }
}
