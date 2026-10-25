//! rs#298 slice 4a: the compiler parity gate. Every conformance
//! artifact is a RUBY-compiled envelope; this test parses the same
//! `.parg` source with the Rust parser, compiles it with the Rust
//! compiler, and requires the atom stream and root to match the
//! Ruby-built grammar byte-for-byte (JSON equality). Drift is a
//! release blocker for both compilers.
//!
//! Slice-4b scope is merged: trivia injection (the skip declaration
//! fixtures), left-recursion tier rewriting, use-import merging, and
//! table expressions all compile to the Ruby wire.

use std::collections::BTreeMap;

use parsanol::parg::frontend::parser;
use parsanol::parg::frontend::{compile, imports};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join("corpus")
        .join("conformance")
}

/// Fixtures that compile through no default entry (support grammars)
/// are skipped at their artifact check below.
const SKIPPED: &[(&str, &str)] = &[];

#[test]
fn rust_compiler_matches_the_ruby_atom_streams() {
    let dir = corpus_dir();
    assert!(dir.is_dir(), "conformance corpus missing");

    let mut compared = 0usize;
    let mut skipped = 0usize;
    let mut entries = std::fs::read_dir(&dir)
        .expect("corpus dir")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect::<Vec<_>>();
    entries.sort();

    for path in &entries {
        let name = path.file_stem().unwrap().to_str().unwrap().to_string();
        if SKIPPED.iter().any(|(skip, _)| *skip == name) {
            skipped += 1;
            eprintln!("skip: {name} — slice 4b");
            continue;
        }

        let source_path = dir.join(format!("{name}.parg"));
        let source = std::fs::read_to_string(&source_path).expect("source readable");
        let (mut document, atomic_rules) = parser::parse_with_atomic(&source)
            .unwrap_or_else(|err| panic!("{name}: rust parse failed: {err}"));
        imports::merge(&mut document, std::slice::from_ref(&dir))
            .unwrap_or_else(|err| panic!("{name}: import merge failed: {err}"));
        let artifact: Value =
            serde_json::from_str(&std::fs::read_to_string(path).expect("artifact")).expect("json");
        // Support grammars (shared helpers) ship no default entry —
        // every declared entry still compiles, and multi-entry
        // grammars (the atomic fixture) compile each side of the
        // exemption separately.
        let entries: Vec<String> = match artifact["default_entry"].as_str() {
            Some(entry) => vec![entry.to_string()],
            None => artifact["entries"]
                .as_object()
                .expect("entry map")
                .keys()
                .cloned()
                .collect(),
        };
        for entry in &entries {
            let tables: BTreeMap<String, Vec<BTreeMap<String, String>>> = artifact
                .get("tables")
                .and_then(Value::as_object)
                .map(|map| {
                    map.iter()
                        .map(|(name, manifest)| {
                            let rows = manifest["rows"]
                                .as_array()
                                .expect("table rows")
                                .iter()
                                .map(|row| {
                                    let mut cells = BTreeMap::new();
                                    for (key, value) in row.as_object().expect("row object") {
                                        cells.insert(
                                            key.clone(),
                                            value.as_str().unwrap_or_default().to_string(),
                                        );
                                    }
                                    cells
                                })
                                .collect();
                            (name.clone(), rows)
                        })
                        .collect()
                })
                .unwrap_or_default();
            let grammar = compile::compile_entry_with(&document, entry, &tables, &atomic_rules)
                .unwrap_or_else(|err| panic!("{name}/{entry}: rust compile failed: {err}"));
            let got = serde_json::to_value(&grammar).expect("grammar serializes");

            let want = &artifact["entries"][entry]["grammar"];
            assert_eq!(
                got["atoms"],
                want["atoms"],
                "{name}/{entry}: atom stream drift\n  got:  {}\n  want: {}",
                serde_json::to_string_pretty(&got["atoms"]).unwrap(),
                serde_json::to_string_pretty(&want["atoms"]).unwrap()
            );
            assert_eq!(got["root"], want["root"], "{name}/{entry}: root drift");
            compared += 1;
        }
    }
    assert!(compared >= 20, "parity corpus shrank: {compared}");
    eprintln!("compiler parity: {compared} atom streams identical, {skipped} skips");
}
