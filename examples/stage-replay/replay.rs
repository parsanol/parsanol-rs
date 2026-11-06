//! Staged testsuite replay (plurimath#540): parse the raw-tree rows of a
//! plurimath-testsuite stage generation with `PargArtifact::parse_shape`
//! and compare them with the Ruby-produced expectations.
//!
//! The canonical tagged form is the one `Canonical.dump` writes in
//! plurimath/latexmath scripts/stage_cases.rb:
//!
//!   nil          -> { nil: true }
//!   boolean      -> { bool: b }
//!   integer      -> { int: "<decimal>" }
//!   hash key     -> { sym: name }, or { str: name } for a renamed key
//!   string       -> { str: s }
//!   array        -> { array: [...] }
//!   hash         -> { hash: [[key, value], ...] }
//!   slice        -> { slice, byte_offset, char_offset }
//!   refusal      -> { refusal: { category, byte_offset } }
//!
//! parsanol-tree/v2 TaggedArray nodes are flattened first. Hash pair
//! order is not part of the contract across engines, so hashes compare
//! as unordered sets of pairs. Rows recorded as refusals carry no
//! preprocessed text: their caller input is parsed and the outcome is
//! compared by category only, since the generation records the refusal
//! offset in the caller's input.

#![allow(dead_code)]

use parsanol::parg::bindings_artifact::PargBindingsArtifact;
use parsanol::{PargArtifact, PargError};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// latexmath's Configuration: decimal marker -> artifact entry.
pub const LATEX_DECIMAL_ENTRIES: &[(&str, &str)] =
    &[(".", "dot"), (",", "comma"), ("\u{66b}", "arabic")];

/// latexmath's CAPTURE_RENAMES: the PARG capture -> the legacy key.
pub const LATEX_CAPTURE_RENAMES: &[(&str, &str)] = &[("row_break", "\\\\")];

/// The capture holding the tree.
pub const LATEX_CAPTURE: &str = "latex_tree";

/// One replayed row.
#[derive(Debug)]
pub struct ReplayRow {
    pub payload: String,
    pub id: String,
    pub input: String,
    pub parsed: String,
    pub differ: Option<String>,
    pub expected: Value,
    pub actual: Value,
}

/// Every replayed row of a run.
#[derive(Debug)]
pub struct ReplayReport {
    pub rows: Vec<ReplayRow>,
}

impl ReplayReport {
    pub fn differ(&self) -> usize {
        self.rows.iter().filter(|r| r.differ.is_some()).count()
    }

    pub fn equal(&self) -> usize {
        self.rows.len() - self.differ()
    }

    /// One summary line, then both trees of each differing row.
    pub fn format(&self) -> String {
        let mut out = format!(
            "{} rows: {} equal, {} differ\n",
            self.rows.len(),
            self.equal(),
            self.differ()
        );
        for row in self.rows.iter().filter(|r| r.differ.is_some()) {
            out.push_str(&format!(
                "DIFFER {} {}: {}\n  input:    {:?}\n  parsed:   {:?}\n  expected: {}\n  actual:   {}\n",
                row.payload,
                row.id,
                row.differ.as_deref().unwrap_or(""),
                row.input,
                row.parsed,
                sort_canonical(&row.expected),
                row.actual,
            ));
        }
        out
    }
}

fn is_input_ref(map: &Map<String, Value>) -> bool {
    map.get("offset").is_some_and(Value::is_u64)
        && map.get("length").is_some_and(Value::is_u64)
        && map.keys().all(|k| {
            matches!(
                k.as_str(),
                "offset" | "length" | "value" | "line" | "column"
            )
        })
}

fn is_tagged_array(map: &Map<String, Value>) -> bool {
    map.len() == 2
        && map.get("tag").is_some_and(Value::is_string)
        && map.get("items").is_some_and(Value::is_array)
}

fn is_plain_hash(value: &Value) -> bool {
    matches!(value, Value::Object(m) if !is_input_ref(m))
}

/// Flatten parsanol-tree/v2 TaggedArray nodes the way the Parslet-style
/// binders do: `repetition` is an array, `maybe` is its item or nil,
/// `sequence` merges hashes (captures win over bare spans), drops nils,
/// and a lone item is itself.
pub fn flatten_shape(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(flatten_shape).collect()),
        Value::Object(map) if is_input_ref(map) => value.clone(),
        Value::Object(map) if is_tagged_array(map) => {
            let items: Vec<Value> = map["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(flatten_shape)
                .collect();
            let tag = map["tag"].as_str().unwrap().trim_start_matches(':');
            match tag {
                "repetition" => Value::Array(items),
                "maybe" => items.into_iter().next().unwrap_or(Value::Null),
                "sequence" => fold_sequence(items),
                other => panic!("parsanol-tree/v2: unknown TaggedArray tag {other:?}"),
            }
        }
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), flatten_shape(v)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn fold_sequence(items: Vec<Value>) -> Value {
    let present: Vec<Value> = items.into_iter().filter(|i| !i.is_null()).collect();
    if present.is_empty() {
        return Value::Null;
    }
    if present.len() == 1 {
        return present.into_iter().next().unwrap();
    }
    let hashes = present.iter().filter(|i| is_plain_hash(i)).count();
    let refs_or_hashes = present.iter().all(|i| matches!(i, Value::Object(_)));
    if hashes > 0 && refs_or_hashes {
        let mut merged = Map::new();
        for item in present.iter().filter(|i| is_plain_hash(i)) {
            merged.extend(item.as_object().unwrap().clone());
        }
        return Value::Object(merged);
    }
    Value::Array(present)
}

/// `value` (a parsanol-tree/v2 node) in canonical tagged form. `source`
/// is the grammar input the slices point into.
pub fn dump_canonical(value: &Value, source: &str, renames: &HashMap<String, String>) -> Value {
    fn dump(v: &Value, source: &str, renames: &HashMap<String, String>) -> Value {
        match v {
            Value::Null => json!({"nil": true}),
            Value::Bool(b) => json!({"bool": b}),
            Value::Number(n) => match n.as_i64() {
                Some(i) => json!({"int": i.to_string()}),
                None => panic!("no canonical form for non-integer {n}"),
            },
            Value::String(s) => json!({"str": s}),
            Value::Array(items) => {
                json!({"array": items.iter().map(|i| dump(i, source, renames)).collect::<Vec<_>>()})
            }
            Value::Object(map) if is_input_ref(map) => {
                let offset = map["offset"].as_u64().unwrap() as usize;
                let text = match map.get("value").and_then(Value::as_str) {
                    Some(text) => text.to_string(),
                    None => {
                        let length = map["length"].as_u64().unwrap() as usize;
                        source[offset..].chars().take(length).collect()
                    }
                };
                let char_offset = source.get(..offset).map(|s| s.chars().count());
                json!({"slice": text, "byte_offset": offset, "char_offset": char_offset})
            }
            Value::Object(map) => {
                let pairs: Vec<Value> = map
                    .iter()
                    .map(|(k, item)| {
                        let key = match renames.get(k) {
                            Some(renamed) => json!({"str": renamed}),
                            None => json!({"sym": k}),
                        };
                        json!([key, dump(item, source, renames)])
                    })
                    .collect();
                json!({"hash": pairs})
            }
        }
    }
    dump(&flatten_shape(value), source, renames)
}

/// `value` with every hash's pairs ordered by their key's JSON text, the
/// form two runtimes compare byte for byte. Object keys are already
/// sorted by serde_json's map.
pub fn sort_canonical(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(sort_canonical).collect()),
        Value::Object(map) if map.len() == 1 && map.get("hash").is_some_and(Value::is_array) => {
            let mut pairs: Vec<Value> = map["hash"]
                .as_array()
                .unwrap()
                .iter()
                .map(sort_canonical)
                .collect();
            pairs.sort_by_key(|pair| pair.get(0).map(Value::to_string).unwrap_or_default());
            json!({"hash": pairs})
        }
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), sort_canonical(v)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Equality of two canonical values, ignoring hash pair order.
pub fn canonical_equal(a: &Value, b: &Value) -> bool {
    sort_canonical(a) == sort_canonical(b)
}

/// The payload files: one file, or a directory's raw-*.json in byte order.
/// The generation stores payloads as YAML; convert them to JSON first
/// (the CI job does this with Ruby), so no YAML crate is needed.
pub fn payload_files(path: &Path) -> Vec<PathBuf> {
    if path.is_file() {
        return vec![path.to_path_buf()];
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(path)
        .unwrap_or_else(|err| panic!("{}: {err}", path.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("raw-") && n.ends_with(".json"))
        })
        .collect();
    files.sort();
    files
}

/// The sibling `<name>.bindings.rs.json` of an artifact, when present.
pub fn default_bindings(artifact: &Path) -> Option<PathBuf> {
    let stem = artifact.file_stem()?.to_str()?;
    let candidate = artifact.with_file_name(format!("{stem}.bindings.rs.json"));
    candidate.is_file().then_some(candidate)
}

/// Replay every raw-tree row of `payloads` against `artifact` with the
/// latexmath configuration.
pub fn replay_stage(artifact_path: &Path, payloads: &Path) -> ReplayReport {
    let artifact = PargArtifact::from_path(artifact_path)
        .unwrap_or_else(|err| panic!("{}: artifact rejected: {err}", artifact_path.display()));
    let bindings = default_bindings(artifact_path).map(|p| {
        let text = std::fs::read_to_string(&p).expect("bindings readable");
        PargBindingsArtifact::from_json(&text, Some("rs"))
            .unwrap_or_else(|err| panic!("{}: bindings rejected: {err}", p.display()))
    });
    let entries: HashMap<&str, &str> = LATEX_DECIMAL_ENTRIES.iter().copied().collect();
    let renames: HashMap<String, String> = LATEX_CAPTURE_RENAMES
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    let mut rows = Vec::new();
    for file in payload_files(payloads) {
        let text = std::fs::read_to_string(&file).expect("payload readable");
        let doc: Value = serde_json::from_str(&text)
            .unwrap_or_else(|err| panic!("{}: not JSON: {err}", file.display()));
        if doc["boundary"] != "raw-tree" {
            continue;
        }
        let payload = file.file_name().unwrap().to_string_lossy().into_owned();
        for case in doc["cases"].as_array().into_iter().flatten() {
            let id = case["id"].as_str().unwrap_or("?").to_string();
            let decimal = case["options"]["decimal"].as_str().unwrap_or(".");
            let entry = *entries
                .get(decimal)
                .unwrap_or_else(|| panic!("{id}: no entry for decimal marker {decimal:?}"));
            let input = case["input"]["str"].as_str().unwrap_or("").to_string();
            let expected_refusal = case["expected"].get("refusal");
            let parsed = match expected_refusal {
                Some(_) => input.clone(),
                None => case["expected"]["preprocessed"]["str"]
                    .as_str()
                    .unwrap_or_else(|| {
                        panic!("{id}: expected neither a preprocessed text nor a refusal")
                    })
                    .to_string(),
            };

            let actual = match artifact.parse_shape(entry, &parsed) {
                Ok(shape) => {
                    let bound = match &bindings {
                        Some(b) => b
                            .apply(&artifact, entry, &shape)
                            .unwrap_or_else(|err| panic!("{id}: bindings failed: {err}")),
                        None => shape,
                    };
                    let tree = bound
                        .get(LATEX_CAPTURE)
                        .unwrap_or_else(|| panic!("{id}: the tree has no {LATEX_CAPTURE} capture"));
                    dump_canonical(tree, &parsed, &renames)
                }
                Err(err) => {
                    let offset = match err {
                        PargError::ParseWire { offset, .. } => Some(offset),
                        _ => None,
                    };
                    json!({"refusal": {"category": "parse_error", "byte_offset": offset}})
                }
            };
            let refused = actual.get("refusal").is_some();
            let (expected, differ) = match expected_refusal {
                Some(refusal) => {
                    let differ = if !refused {
                        Some("the generation refuses, rs parsed".to_string())
                    } else if refusal["category"] != "parse_error" {
                        Some(format!("refusal category {}", refusal["category"]))
                    } else {
                        None
                    };
                    (json!({"refusal": refusal}), differ)
                }
                None => {
                    let tree = case["expected"]["tree"].clone();
                    let differ = if refused {
                        Some("the generation parsed, rs refused".to_string())
                    } else if !canonical_equal(&actual, &tree) {
                        Some("trees differ".to_string())
                    } else {
                        None
                    };
                    (tree, differ)
                }
            };
            rows.push(ReplayRow {
                payload: payload.clone(),
                id,
                input,
                parsed,
                differ,
                expected,
                actual: sort_canonical(&actual),
            });
        }
    }
    ReplayReport { rows }
}
