//! B1 reference-mode gate (TODO.sota/8): the plain recursive relation —
//! no memo tables, no fused repetition scans, no terminal fast paths —
//! must agree with the default walker on every conformance case.
//!
//! E1 declares memoization, program compilation and caching
//! unobservable; this test is what makes that claim executable. A
//! divergence is either an engine bug or an E1 violation, and the case
//! is pinned here until it is resolved.

use parsanol::portable::transform::{ast_to_value, Value};
use parsanol::portable::{to_parslet_compatible, AstArena, Grammar, PortableParser};
use parsanol::PargArtifact;
use serde_json::Value as JsonValue;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn conformance_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join("corpus")
        .join("conformance")
}

/// Parse `input` against `grammar` and return the parsanol-shape tree
/// as JSON — or `None` on rejection. Mirrors `PargArtifact::parse_shape`
/// so both modes go through the same shaping pipeline.
fn shape(grammar: &Grammar, input: &str, reference: bool) -> Option<Value> {
    // The shaped tree can dwarf the input (large grammars bind few
    // characters per node); size the arena for the tree, not the text.
    let mut arena = AstArena::for_input(input.len().max(1 << 16));
    let mut parser = if reference {
        PortableParser::new_reference(grammar, input, &mut arena)
    } else {
        PortableParser::new(grammar, input, &mut arena)
    };
    let raw = parser.parse().ok()?;
    let shaped = to_parslet_compatible(&raw, &mut arena, input);
    Some(ast_to_value(&shaped, &arena, input))
}

#[test]
fn reference_mode_agrees_with_the_default_walker() {
    let dir = conformance_dir();
    if !dir.is_dir() {
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

    let mut mismatches: Vec<String> = Vec::new();
    let mut checked = 0usize;
    let mut accepted = 0usize;

    for name in &names {
        let artifact = PargArtifact::from_path(dir.join(format!("{name}.json")))
            .unwrap_or_else(|err| panic!("{name}: artifact rejected: {err}"));

        // Every test input, deduplicated by (entry, input); the suite
        // may assert the same pair through several test kinds.
        let mut cases: BTreeSet<(String, String)> = BTreeSet::new();
        for test in artifact.tests() {
            let (Some(entry), Some(input)) = (
                test.get("entry").and_then(JsonValue::as_str),
                test.get("input").and_then(JsonValue::as_str),
            ) else {
                continue;
            };
            cases.insert((entry.to_string(), input.to_string()));
        }

        for (entry, input) in cases {
            let Ok(grammar) = artifact.grammar(&entry) else {
                continue;
            };
            let default = shape(&grammar, &input, false);
            let reference = shape(&grammar, &input, true);
            checked += 1;
            accepted += usize::from(default.is_some());
            if default != reference {
                mismatches.push(format!(
                    "{name} [{entry}] {input:?}: default={default:?} reference={reference:?}"
                ));
            }
        }
    }

    eprintln!(
        "reference-mode: {checked} cases across {} artifacts ({accepted} accepted, {} rejected)",
        names.len(),
        checked - accepted
    );
    assert!(
        checked > 0,
        "no conformance inputs were exercised — the corpus has no test cases"
    );
    assert!(
        accepted > 0 && accepted < checked,
        "expected both accept and reject cases (got {accepted} accepted of {checked})"
    );
    assert!(
        mismatches.is_empty(),
        "{} reference-mode divergence(s) over {checked} cases:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}
