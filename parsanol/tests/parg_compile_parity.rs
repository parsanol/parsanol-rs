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
use parsanol::parg::frontend::{compile, envelope, imports};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn artifact_tables(artifact: &Value) -> compile::Tables {
    artifact
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
        .unwrap_or_default()
}

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
        let mut parsed = parser::parse_grammar(&source)
            .unwrap_or_else(|err| panic!("{name}: rust parse failed: {err}"));
        parsed.docs.extend(
            imports::merge(&mut parsed.document, std::slice::from_ref(&dir))
                .unwrap_or_else(|err| panic!("{name}: import merge failed: {err}")),
        );
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
        let tables = artifact_tables(&artifact);
        for entry in &entries {
            let grammar =
                compile::compile_entry_with(&parsed.document, entry, &tables, &parsed.atomic_rules)
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

/// The full-envelope gate: the Rust envelope assembly matches the
/// Ruby artifact on every field except `checksum` and
/// `compiler_version` (the compiling engine's own stamps).
#[test]
fn rust_envelope_matches_the_ruby_artifacts() {
    let dir = corpus_dir();
    let mut compared = 0usize;
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("corpus dir")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    paths.sort();

    for path in &paths {
        let name = path.file_stem().unwrap().to_str().unwrap().to_string();
        let artifact: Value =
            serde_json::from_str(&std::fs::read_to_string(path).expect("artifact json"))
                .expect("json");
        // Normalize CRLF checkouts (Windows): the envelope's source,
        // docs, and hashes are LF (the Ruby bake's form).
        let source = std::fs::read_to_string(dir.join(format!("{name}.parg")))
            .expect("source")
            .replace("\r\n", "\n");
        let mut parsed = parser::parse_grammar(&source)
            .unwrap_or_else(|err| panic!("{name}: rust parse failed: {err}"));
        parsed.docs.extend(
            imports::merge(&mut parsed.document, std::slice::from_ref(&dir))
                .unwrap_or_else(|err| panic!("{name}: import merge failed: {err}")),
        );
        let tables = artifact_tables(&artifact);
        let got = envelope::build(&parsed, &tables, None)
            .unwrap_or_else(|err| panic!("{name}: rust envelope failed: {err}"));

        for (key, want) in artifact.as_object().expect("artifact object") {
            if key == "checksum" || key == "compiler_version" {
                continue;
            }
            assert_eq!(
                got.get(key),
                Some(want),
                "{name}: envelope field {key:?} drifts\n  got:  {}\n  want: {}",
                serde_json::to_string_pretty(got.get(key).unwrap_or(&Value::Null)).unwrap(),
                serde_json::to_string_pretty(want).unwrap()
            );
        }
        compared += 1;
    }
    assert!(compared >= 20, "parity corpus shrank: {compared}");
    eprintln!("envelope parity: {compared} envelopes identical (minus engine stamps)");
}

/// The lint gate: order_warnings match the Ruby artifacts byte-for-byte
/// across the flavor corpus (33 flavors carry non-empty lists).
#[test]
fn rust_order_warnings_match_the_ruby_flavor_corpus() {
    let artifacts = match std::env::var("PARG_ARTIFACT_DIR") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => {
            let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
            match manifest
                .ancestors()
                .map(|ancestor| {
                    ancestor
                        .join("pubid")
                        .join("pubid-grammar")
                        .join("artifacts")
                })
                .find(|candidate| candidate.is_dir())
            {
                Some(dir) => dir,
                None => {
                    eprintln!("skip: no sibling pubid-grammar checkout");
                    return;
                }
            }
        }
    };
    let entries = std::fs::read_dir(&artifacts).expect("artifacts dir");
    // An ambient sibling predating the artifact re-bake (pubid-grammar
    // #61) carries pre-lint-change order_warnings: its adobe.json is
    // still at the 1.3.93 bake. Pinned dirs (CI) and re-baked siblings
    // assert fully; stale ambient siblings skip with guidance (the
    // parg_corpus stale-sibling pattern).
    let sibling_is_stale = std::env::var("PARG_ARTIFACT_DIR").is_err()
        && serde_json::from_str::<Value>(
            &std::fs::read_to_string(artifacts.join("adobe.json")).expect("adobe artifact"),
        )
        .expect("adobe json")["compiler_version"]
            .as_str()
            == Some("1.3.93");
    if sibling_is_stale {
        eprintln!(
            "skip: the sibling pubid-grammar checkout predates the re-bake (adobe.json is 1.3.93); refresh the checkout or set PARG_ARTIFACT_DIR"
        );
        return;
    }
    let mut checked = 0usize;
    let mut warning_carrying = 0usize;
    let mut unparsable: Vec<String> = Vec::new();
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    paths.sort();
    for artifact_path in &paths {
        let name = artifact_path
            .file_stem()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        let artifact: Value =
            serde_json::from_str(&std::fs::read_to_string(artifact_path).expect("artifact json"))
                .expect("json");
        let Some(source) = artifact["source"].as_str() else {
            continue;
        };
        // Slice-5 scope: the frontend's parse coverage is the
        // conformance corpus; flavors it cannot parse yet are the
        // visible worklist, listed here.
        let Ok(mut parsed) = parser::parse_grammar(source) else {
            unparsable.push(name.clone());
            continue;
        };
        let grammars = artifacts.parent().expect("repo root").join("grammars");
        parsed.docs.extend(
            imports::merge(&mut parsed.document, &[grammars])
                .unwrap_or_else(|err| panic!("{name}: import merge failed: {err}")),
        );
        let tables = artifact_tables(&artifact);
        let compiler = compile::compiler_for(&parsed.document, &tables, &parsed.atomic_rules)
            .unwrap_or_else(|err| panic!("{name}: rust compile failed: {err}"));
        let got = serde_json::to_value(compiler.order_warnings()).expect("warnings serialize");
        assert_eq!(
            got,
            artifact["lint"]["order_warnings"],
            "{name}: order_warnings drift\n  got:  {}\n  want: {}",
            serde_json::to_string_pretty(&got).unwrap(),
            serde_json::to_string_pretty(&artifact["lint"]["order_warnings"]).unwrap()
        );
        if !artifact["lint"]["order_warnings"]
            .as_array()
            .unwrap()
            .is_empty()
        {
            warning_carrying += 1;
        }
        checked += 1;
    }
    assert!(checked >= 10, "flavor coverage shrank: {checked}");
    assert!(
        warning_carrying >= 5,
        "warning-carrying coverage shrank: {warning_carrying}"
    );
    eprintln!(
        "order_warnings parity: {checked} flavors identical ({warning_carrying} carrying warnings); unparsable: {:?}",
        unparsable
    );
}

/// The flavor-source parse coverage gate (rs#298 closeout): every
/// pubid-grammar flavor source parses with the Rust frontend.
#[test]
fn rust_parses_every_flavor_source() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let grammars = manifest
        .ancestors()
        .map(|ancestor| {
            ancestor
                .join("pubid")
                .join("pubid-grammar")
                .join("grammars")
        })
        .find(|candidate| candidate.is_dir());
    let Some(grammars) = grammars else {
        eprintln!("skip: no sibling pubid-grammar checkout");
        return;
    };
    let mut parsed = 0usize;
    let mut failures = Vec::new();
    let mut sources: Vec<PathBuf> = std::fs::read_dir(&grammars)
        .expect("grammars dir")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "parg"))
        .collect();
    sources.sort();
    for path in &sources {
        let name = path.file_stem().unwrap().to_str().unwrap().to_string();
        let source = std::fs::read_to_string(path).expect("grammar readable");
        match parser::parse_grammar(&source) {
            Ok(_) => parsed += 1,
            Err(err) => failures.push(format!("{name}: {}", err.message)),
        }
    }
    assert!(
        failures.is_empty(),
        "flavor sources the frontend cannot parse yet:\n{}",
        failures.join("\n")
    );
    assert!(parsed >= 45, "flavor corpus shrank: {parsed}");
    eprintln!("flavor parse coverage: {parsed} sources parse");
}
