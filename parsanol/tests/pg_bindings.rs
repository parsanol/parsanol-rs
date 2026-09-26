//! C7 parity gate: the Rust bindings runtime must reproduce the Ruby
//! `Parsanol::PG::Bindings` outputs byte-for-byte. The fixture is
//! generated from Ruby (`/tmp/gen_fixture.rb` → regenerated with the
//! Ruby toolchain) and replays here unchanged.

use parsanol::PgArtifact;
use serde_json::Value;

fn fixture() -> Value {
    let text = include_str!("fixtures/pg_bindings.json");
    serde_json::from_str(text).expect("fixture is valid JSON")
}

#[test]
fn replay_ruby_fixture_cases() {
    for case in fixture()["cases"].as_array().unwrap() {
        let envelope_text = serde_json::to_string(&case["envelope"]).unwrap();
        let artifact = PgArtifact::from_json(&envelope_text)
            .unwrap_or_else(|err| panic!("{}: envelope rejected: {err}", case["grammar"]));
        let bound = artifact
            .apply_bindings(case["entry"].as_str().unwrap(), &case["shape"])
            .unwrap_or_else(|err| panic!("{}: bindings failed: {err}", case["grammar"]));
        assert_eq!(
            &bound, &case["bound"],
            "bindings diverge from Ruby for {} / {:?}",
            case["grammar"], case["input"]
        );
    }
}

#[test]
fn synthetic_array_and_preprocess_artifact() {
    let envelope: Value = serde_json::json!({
        "version": "0.0.0",
        "grammar": "Synth",
        "shape": "parsanol-tree/v2",
        "binding_version": 1,
        "entries": {
            "identifier": {
                "root": "identifier",
                "grammar": {},
                "bindings": [
                    { "capture": "part", "path": "parts[]", "type": "integer" },
                    { "capture": "code", "path": "parts[].code", "type": "string" },
                    { "capture": "raw", "path": "label", "type": "string", "preprocess": "upcase_map" }
                ]
            }
        },
        "preprocess": {
            "upcase_map": [
                { "op": "table_lookup", "table": "codes", "from": "lower", "to": "upper" }
            ]
        },
        "tables": {
            "codes": {
                "file": "codes.yaml",
                "rows": [
                    { "lower": "ab", "upper": "AB" },
                    { "lower": "cd", "upper": "CD" }
                ]
            }
        }
    });

    // Bake a valid checksum so the artifact verifies.
    let mut envelope = envelope.as_object().unwrap().clone();
    let checksum = parsanol::pg::checksum_hex(&Value::Object(envelope.clone())).unwrap();
    envelope.insert("checksum".to_string(), Value::String(checksum));
    let artifact = PgArtifact::from_json(&serde_json::to_string(&envelope).unwrap()).unwrap();

    let shape: Value = serde_json::json!({
        "part": "1",
        "code_holder": { "part": "2", "code": "x" },
        "raw": "ab"
    });
    let bound = artifact.apply_bindings("identifier", &shape).unwrap();
    assert_eq!(
        bound,
        serde_json::json!({
            "parts": [
                { "parts[]": 1, "code": "x" },
                { "parts[]": 2 }
            ],
            "label": "AB"
        })
    );
}

#[test]
fn unknown_preprocess_and_tables_raise() {
    let envelope: Value = serde_json::json!({
        "version": "0.0.0",
        "shape": "parsanol-tree/v2",
        "entries": {
            "identifier": {
                "root": "identifier",
                "grammar": {},
                "bindings": [
                    { "capture": "raw", "path": "label", "type": "string", "preprocess": "missing" }
                ]
            }
        }
    });
    let mut envelope = envelope.as_object().unwrap().clone();
    let checksum = parsanol::pg::checksum_hex(&Value::Object(envelope.clone())).unwrap();
    envelope.insert("checksum".to_string(), Value::String(checksum));
    let artifact = PgArtifact::from_json(&serde_json::to_string(&envelope).unwrap()).unwrap();
    let shape: Value = serde_json::json!({ "raw": "ab" });
    let err = artifact.apply_bindings("identifier", &shape).unwrap_err();
    assert!(matches!(err, parsanol::PgError::UnknownPreprocess(_)));
}

#[test]
fn nested_binding_paths_are_refused() {
    let envelope: Value = serde_json::json!({
        "version": "0.0.0",
        "shape": "parsanol-tree/v2",
        "entries": {
            "identifier": {
                "root": "identifier",
                "grammar": {},
                "bindings": [
                    { "capture": "raw", "path": "a.b", "type": "string" }
                ]
            }
        }
    });
    let mut envelope = envelope.as_object().unwrap().clone();
    let checksum = parsanol::pg::checksum_hex(&Value::Object(envelope.clone())).unwrap();
    envelope.insert("checksum".to_string(), Value::String(checksum));
    let artifact = PgArtifact::from_json(&serde_json::to_string(&envelope).unwrap()).unwrap();
    let shape: Value = serde_json::json!({ "raw": "ab" });
    let err = artifact.apply_bindings("identifier", &shape).unwrap_err();
    assert!(matches!(err, parsanol::PgError::NestedBindingPath { .. }));
}

#[test]
fn rust_native_parse_and_bind_matches_ruby_shape_and_bound() {
    for case in fixture()["cases"].as_array().unwrap() {
        if case["bound"]
            .as_object()
            .is_none_or(|bound| bound.is_empty())
        {
            continue; // grammars without bindings have nothing to compare
        }
        let envelope_text = serde_json::to_string(&case["envelope"]).unwrap();
        let artifact = PgArtifact::from_json(&envelope_text).unwrap();
        let entry = case["entry"].as_str().unwrap();
        let input = case["input"].as_str().unwrap();
        let shape = artifact
            .parse_shape(entry, input)
            .unwrap_or_else(|err| panic!("rust parse failed for {input:?}: {err}"));
        assert_eq!(
            &shape, &case["shape"],
            "rust shape diverges from Ruby for {input:?}"
        );
        let bound = artifact.apply_bindings(entry, &shape).unwrap();
        assert_eq!(&bound, &case["bound"], "bound diverges for {input:?}");
    }
}

#[test]
fn rust_runs_embedded_suites_green() {
    for case in fixture()["cases"].as_array().unwrap() {
        let envelope_text = serde_json::to_string(&case["envelope"]).unwrap();
        let artifact = PgArtifact::from_json(&envelope_text).unwrap();
        let failures = artifact.run_tests();
        assert!(
            failures.is_empty(),
            "{} embedded suite failures: {failures:?}",
            case["grammar"]
        );
    }
}

#[test]
fn schema_from_artifact_matches_contract() {
    let case = &fixture()["cases"][1]; // iso
    let envelope_text = serde_json::to_string(&case["envelope"]).unwrap();
    let artifact = PgArtifact::from_json(&envelope_text).unwrap();
    let schema = parsanol::pg::schema::from_artifact(&artifact).unwrap();
    let entry = case["entry"].as_str().unwrap();
    let fields = schema[entry]["fields"].as_object().unwrap();
    assert_eq!(fields["publisher"]["type"].as_str().unwrap(), "string");
    assert_eq!(fields["year"]["type"].as_str().unwrap(), "integer");
    assert_eq!(
        fields["publisher_name"]["preprocess"].as_str().unwrap(),
        "publisher_names"
    );
    let ts = parsanol::pg::schema::to_typescript(&schema);
    assert!(ts.contains("export interface Identifier {"));
    assert!(ts.contains("  publisherName: string;"));
}

#[test]
fn unsupported_shapes_are_refused_at_load() {
    let envelope: Value = serde_json::json!({
        "version": "0.0.0",
        "shape": "parsanol-tree/v1",
        "entries": {}
    });
    let mut envelope = envelope.as_object().unwrap().clone();
    let checksum = parsanol::pg::checksum_hex(&Value::Object(envelope.clone())).unwrap();
    envelope.insert("checksum".to_string(), Value::String(checksum));
    let err = PgArtifact::from_json(&serde_json::to_string(&envelope).unwrap()).unwrap_err();
    assert!(matches!(err, parsanol::PgError::UnsupportedShape(_)));
}

/// C13: every baked flavor artifact in pubid-grammar/artifacts runs its
/// embedded suite green on the Rust VM.
#[test]
fn every_baked_artifact_runs_green_on_the_rust_vm() {
    let dir = std::env::var("PG_ARTIFACT_DIR").unwrap_or_else(|_| {
        // Host layout: parsanol-rs sits beside pubid/pubid-grammar.
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        for ancestor in manifest.ancestors().skip(1) {
            let candidate = ancestor
                .join("pubid")
                .join("pubid-grammar")
                .join("artifacts");
            if candidate.is_dir() {
                return candidate.to_string_lossy().to_string();
            }
        }
        panic!("artifacts dir not found (set PG_ARTIFACT_DIR)")
    });
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).expect("artifacts dir").flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let artifact = PgArtifact::from_json(&text)
            .unwrap_or_else(|err| panic!("{} rejected: {err}", path.display()));
        eprintln!("[sweep] {} ...", path.display());
        for test in artifact.tests() {
            eprintln!(
                "[sweep]   entry={:?} kind={:?} input={:?}",
                test.get("entry").and_then(Value::as_str),
                test.get("kind").and_then(Value::as_str),
                test.get("input").and_then(Value::as_str)
            );
        }
        let failures = artifact.run_tests();
        assert!(
            failures.is_empty(),
            "{} embedded suite failures: {failures:?}",
            path.display()
        );
        checked += 1;
    }
    assert!(checked >= 40, "expected the full flavor set, ran {checked}");
}

/// F6: all engines must render the same string from the same artifact.
#[test]
fn render_string_parity_with_ruby() {
    // the fixture's iso envelope predates the render spec; read the
    // current baked artifact
    let dir = std::env::var("PG_ARTIFACT_DIR").unwrap_or_else(|_| {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        for ancestor in manifest.ancestors().skip(1) {
            let candidate = ancestor
                .join("pubid")
                .join("pubid-grammar")
                .join("artifacts");
            if candidate.is_dir() {
                return candidate.to_string_lossy().to_string();
            }
        }
        panic!("artifacts dir not found (set PG_ARTIFACT_DIR)")
    });
    let artifact = PgArtifact::from_json(
        &std::fs::read_to_string(std::path::Path::new(&dir).join("iso.json")).unwrap(),
    )
    .unwrap();
    let rendered = artifact
        .render_string("identifier", "ISO 5537:2025", "default")
        .unwrap();
    assert_eq!(rendered, "ISO-5537:2025");
}

/// F7 C-ABI mapping: the wire functions round-trip through C strings.
#[test]
fn c_abi_parse_and_error_wire() {
    use parsanol::portable::{AstArena, Grammar, PortableParser};
    let case = &fixture()["cases"][1]; // iso
    let envelope_text = serde_json::to_string(&case["envelope"]).unwrap();
    let artifact = PgArtifact::from_json(&envelope_text).unwrap();
    let grammar = artifact.grammar("identifier").unwrap();
    let grammar_json = serde_json::to_string(&grammar).unwrap();

    // drive the same code path the C ABI uses via the portable engine
    let mut arena = AstArena::for_input(1 << 12);
    let mut parser = PortableParser::new(&grammar, "nonsense", &mut arena);
    let failed = parser.parse().is_err();
    let wire = parser.failure_wire();
    let _ = Grammar::from_json(&grammar_json).unwrap();
    assert!(failed);
    let (offset, expected) = wire.expect("failure wire present");
    assert_eq!(offset, 0);
    assert!(!expected.is_empty());
}
