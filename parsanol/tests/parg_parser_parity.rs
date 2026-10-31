//! rs#298 slice 2: the document parity gate. Every fixture holds the
//! RUBY parser's document structure (grammar name/version, entries,
//! embedded tests, and each rule's AST) for a corpus source; the Rust
//! parser must reproduce it field for field. Regenerate with
//! `ruby tools/dump_parg_document.rb <source> <fixture>` in the same
//! PR as any Ruby parser change.

use parsanol::parg::frontend::parser;
use serde_json::Value;
use std::path::{Path, PathBuf};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/doc_parity")
}

#[test]
fn rust_parser_matches_the_ruby_documents() {
    let dir = fixture_dir();
    assert!(dir.is_dir(), "doc_parity fixtures missing");

    let mut fixtures: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("fixture dir")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    fixtures.sort();

    let mut compared = 0usize;
    for path in &fixtures {
        let name = path
            .file_stem()
            .unwrap()
            .to_str()
            .unwrap()
            .replace(".document", "");

        let raw = std::fs::read_to_string(path).expect("fixture readable");
        let fixture: Value = serde_json::from_str(&raw).expect("fixture json");

        // The fixture carries no source; the corpus copy is the input.
        let corpus = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../corpus/conformance")
            .join(format!("{name}.parg"));
        let source = std::fs::read_to_string(&corpus).expect("corpus source");
        let document = parser::parse(&source)
            .unwrap_or_else(|err| panic!("{name}: rust parser failed: {err}"));

        assert_eq!(
            document.grammar_name, fixture["grammar_name"],
            "{name}: grammar name"
        );
        assert_eq!(document.version, fixture["version"], "{name}: version");

        // The wire is a name→rule mapping; JSON object order is an
        // artifact of the generator, not the contract.
        let mut want_entries: Vec<(String, String)> = fixture["entries"]
            .as_object()
            .expect("entries object")
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().expect("entry rule").to_string()))
            .collect();
        want_entries.sort();
        let mut got_entries = document.entries.clone();
        got_entries.sort();
        assert_eq!(got_entries, want_entries, "{name}: entries");

        let want_tests = fixture["tests"].as_array().expect("tests");
        assert_eq!(document.tests.len(), want_tests.len(), "{name}: test count");
        for (test, want) in document.tests.iter().zip(want_tests) {
            assert_eq!(test.kind, want["kind"], "{name}: test kind");
            assert_eq!(test.input, want["input"], "{name}: test input");
            let want_entry = want["entry"].as_str();
            assert_eq!(
                test.entry.as_deref(),
                want_entry,
                "{name}: test entry (input {:?})",
                test.input
            );
            let want_expect = if want["expect"].is_null() {
                Value::Object(serde_json::Map::new())
            } else {
                want["expect"].clone()
            };
            assert_eq!(test.expect, want_expect, "{name}: test expect");
        }

        let want_rules = fixture["rules"].as_object().expect("rules");
        let got_rules: Vec<&(String, parser::Node)> = document.rules.iter().collect();
        let want_names: Vec<&String> = want_rules.keys().collect();
        // JSON object order is the generator's, not the contract.
        let got_names: Vec<&String> = got_rules.iter().map(|(name, _)| name).collect();
        let mut got_sorted: Vec<&String> = got_names.clone();
        got_sorted.sort();
        let mut want_sorted: Vec<&String> = want_names.clone();
        want_sorted.sort();
        assert_eq!(got_sorted, want_sorted, "{name}: rule names");
        for (rule_name, node) in &got_rules {
            let want_node = &want_rules[rule_name];
            let got = serde_json::to_value(node).expect("node serializes");
            if got != *want_node {
                std::fs::write(
                    "/tmp/tree_got.json",
                    serde_json::to_string_pretty(&got).unwrap(),
                )
                .ok();
                std::fs::write(
                    "/tmp/tree_want.json",
                    serde_json::to_string_pretty(want_node).unwrap(),
                )
                .ok();
                panic!("{name}: rule {rule_name} tree (dumps in /tmp/tree_got.json / /tmp/tree_want.json)");
            }
        }
        compared += 1;
    }
    assert!(compared >= 35, "parity corpus shrank: {compared} fixtures");
    eprintln!("parser parity: {compared} documents identical");
}

// ruby#380: `!x as k` binds the GUARD — a capture inside a negated
// lookahead never binds (it consumes nothing), so the postfix wraps
// the pred, matching the ruby parser's postfix order.
#[test]
fn guard_capture_binds_the_guard_not_the_target() {
    let document = parser::parse(
        "grammar E version \"1\" edition 2 {\n\
           text = ( colon ) ( word ) [ colon ( \"19\" / \"20\" ) 2*digit !digit as year ]\n\
           colon = \":\"\n\
           word = \"AMD\" / \"COR\"\n\
           digit = %x30-39\n\
           entry text: text\n\
         }\n",
    )
    .expect("parses");

    let rule = document
        .rules
        .iter()
        .find(|(name, _)| name == "text")
        .map(|(_, node)| node)
        .expect("rule");

    let mut found = None;
    let mut stack: Vec<&parser::Node> = vec![rule];
    while let Some(node) = stack.pop() {
        if node.kind == "cap" && node.name.as_deref() == Some("year") {
            found = Some(node);
        }
        if let Some(child) = &node.child {
            stack.push(child);
        }
        if let Some(items) = &node.items {
            stack.extend(items);
        }
        if let Some(branches) = &node.branches {
            stack.extend(branches);
        }
    }
    let cap_node = found.expect("year capture present in the tree");
    let pred = cap_node.child.as_deref().expect("cap child");
    assert_eq!(
        pred.kind, "pred",
        "the capture wraps the guard, not the target"
    );
    assert_eq!(pred.positive, Some(false));
    // The guard's target is the bare digit ref, uncaptured.
    let target = pred.child.as_deref().expect("pred child");
    assert_eq!(target.kind, "ref");
}
