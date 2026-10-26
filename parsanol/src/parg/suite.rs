//! Suite runner: evaluates accept/reject/example/capture tests against
//! an artifact with the native portable engine plus the bindings
//! runtime. Port of `Parsanol::PARG::Artifact#run_tests` /
//! `#run_test_list` — failure descriptions are word-compatible with the
//! Ruby runner. `capture` is the grammar-layer phase (raw captures);
//! `example` the bindings layer (bound attribute hash).

use serde_json::Value;

use super::bindings::{is_leaf, leaf_scalar};
use super::{PargArtifact, PargError};
use crate::portable::ast::AstNode;
use crate::portable::capture_state::CaptureValue;
use crate::portable::grammar::{Atom, RepetitionTag};
use crate::portable::parslet_transform::to_parslet_compatible;
use crate::portable::{AstArena, Grammar, PortableParser};

pub(crate) fn ast_to_value(
    node: &crate::portable::ast::AstNode,
    arena: &AstArena,
    input: &str,
) -> Value {
    use crate::portable::ast::AstNode;
    match node {
        AstNode::Nil => Value::Null,
        AstNode::Bool(b) => Value::Bool(*b),
        AstNode::Int(n) => (*n).into(),
        AstNode::Float(f) => serde_json::Number::from_f64(*f)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        AstNode::StringRef { pool_index } => {
            Value::String(arena.get_string(*pool_index as usize).to_string())
        }
        AstNode::InputRef { offset, length } => {
            let offset = *offset as usize;
            let length = *length as usize;
            let value = input.get(offset..offset + length).unwrap_or_default();
            let before = &input[..offset.min(input.len())];
            let line = before.matches('\n').count() + 1;
            let column = offset - before.rfind('\n').map(|index| index + 1).unwrap_or(0) + 1;
            // Wire contract (Slice parity): offset/column are byte
            // positions, length is the character count (content.size).
            serde_json::json!({
                "value": value,
                "line": line,
                "column": column,
                "offset": offset,
                "length": value.chars().count(),
            })
        }
        AstNode::Array { pool_index, length } => {
            let items = arena.get_array(*pool_index as usize, *length as usize);
            Value::Array(
                items
                    .iter()
                    .map(|item| ast_to_value(item, arena, input))
                    .collect(),
            )
        }
        AstNode::Hash { pool_index, length } => {
            let pairs = arena.get_hash_items(*pool_index as usize, *length as usize);
            let mut object = serde_json::Map::new();
            for (key, value) in &pairs {
                object.insert(key.clone(), ast_to_value(value, arena, input));
            }
            Value::Object(object)
        }
    }
}

/// Canonical test-value form (parsanol-ruby#216): one equality surface
/// for capture and example expectations. Terminals reduce to their text
/// (native slice wire objects to their `value` field), structure
/// recurses. Value-compatible with Ruby's `Bindings.canonical`.
pub(crate) fn canonical(value: &Value) -> Value {
    match value {
        Value::Number(number) if number.is_f64() => {
            // B6 float canonical form: the Ruby JSON number text (see
            // ruby_json_float), so float expectations and bound floats
            // compare identically on every engine.
            Value::String(super::ruby_json_float(number.as_f64().unwrap()))
        }
        Value::Array(items) => Value::Array(items.iter().map(canonical).collect()),
        Value::Object(_) if is_leaf(value) => leaf_scalar(value).clone(),
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, item)| (key.clone(), canonical(item)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Wire rendering of one committed capture for the recovery envelope:
/// input-backed spans render as the standard slice wire objects, text
/// captures render bare, host-opaque handles render null (not
/// portable).
fn capture_wire(value: &CaptureValue, input: &str) -> Value {
    match value {
        CaptureValue::Span { offset, length } => {
            let offset = *offset;
            let length = *length;
            let text = input.get(offset..offset + length).unwrap_or_default();
            let before = &input[..offset.min(input.len())];
            let line = before.matches('\n').count() + 1;
            let column = offset - before.rfind('\n').map(|i| i + 1).unwrap_or(0) + 1;
            serde_json::json!({
                "value": text, "line": line, "column": column,
                "offset": offset, "length": text.chars().count()
            })
        }
        CaptureValue::Text(t) => Value::String(t.to_string()),
        CaptureValue::Opaque { .. } => Value::Null,
    }
}

/// Resync rounds per recovery (R2): bounded so recovery stays linear
/// in the input even on pathological skip distance.
const MAX_RESYNCS: usize = 8;

/// The recovery envelope for a failed parse: the captures committed
/// before the failure plus the failure wire. Captures of the failing
/// branch may be present — this is best-effort data, annotated as such
/// by the `ok: false` marker.
#[allow(clippy::too_many_arguments)]
fn recovery_shape(
    entry: &str,
    journal: &[(String, CaptureValue)],
    consumed_upto: usize,
    failure_wire: Option<(usize, Vec<String>)>,
    input: &str,
    arena: &mut AstArena,
    partial: Option<(usize, Vec<AstNode>, RepetitionTag)>,
    root_is_named: Option<String>,
    resyncs: Vec<Value>,
) -> Value {
    let _ = entry;
    let mut captures = serde_json::Map::new();
    for (name, value) in journal {
        // Journal order is capture order — an array per name preserves
        // the per-iteration narrative a rep of same-named captures
        // leaves behind.
        let slot = captures
            .entry(name.clone())
            .or_insert_with(|| Value::Array(Vec::new()));
        if let Value::Array(items) = slot {
            items.push(capture_wire(value, input));
        }
    }
    let error = match failure_wire {
        Some((offset, expected)) => {
            let before = &input[..offset.min(input.len())];
            let line = before.matches('\n').count() + 1;
            let column = offset - before.rfind('\n').map(|i| i + 1).unwrap_or(0) + 1;
            serde_json::json!({
                "offset": offset, "line": line, "column": column, "expected": expected
            })
        }
        None => serde_json::json!({ "offset": input.len(), "expected": [] }),
    };
    // R1.5: the deepest repetition prefix the failed parse matched,
    // shaped exactly like the root would have been (tagged array, Named
    // wrapper when the root rule is named). Null when nothing matched.
    let partial_value = partial.map(|(_end, items, tag)| {
        let tag_str = match tag {
            RepetitionTag::Maybe => ":maybe",
            RepetitionTag::Repetition => ":repetition",
        };
        let (pool, len) = arena.store_tagged_array(tag_str, &items);
        let mut node = AstNode::Array {
            pool_index: pool,
            length: len,
        };
        if let Some(name) = root_is_named {
            let (hp, hl) = arena.store_hash(&[(name.as_str(), node)]);
            node = AstNode::Hash {
                pool_index: hp,
                length: hl,
            };
        }
        let shaped = to_parslet_compatible(&node, arena, input);
        Some(ast_to_value(&shaped, arena, input))
    });
    serde_json::json!({
        "ok": false, "captures": captures, "error": error,
        "consumed_upto": consumed_upto,
        "partial": partial_value,
        "resyncs": resyncs,
    })
}

impl PargArtifact {
    /// Parse an entry with the native portable engine and return the
    /// parsanol-shape tree as JSON.
    pub fn parse_shape(&self, entry: &str, input: &str) -> Result<Value, PargError> {
        let grammar: Grammar = self.grammar(entry)?;
        // The shaped tree can dwarf the input (large grammars bind few
        // characters per node); size the arena for the tree, not the text.
        let mut arena = AstArena::for_input(input.len().max(1 << 16));
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        let raw = parser.parse().map_err(|err| {
            if err.is_abort() {
                // Resource and runtime aborts are not syntax expectations:
                // the failure wire belongs to whatever branch was active, so
                // rendering it would send users after a phantom `]`.
                return PargError::ParseFailed(err.to_string());
            }
            match parser.failure_wire() {
                Some((offset, expected)) => {
                    let before = &input[..offset.min(input.len())];
                    let line = before.matches('\n').count() + 1;
                    let column = offset - before.rfind('\n').map(|i| i + 1).unwrap_or(0) + 1;
                    let ranked = parser
                        .failure_ranks()
                        .iter()
                        .map(|(p, labels)| (*p, labels.clone()))
                        .collect();
                    PargError::ParseWire {
                        offset,
                        line,
                        column,
                        expected,
                        ranked,
                    }
                }
                None => PargError::ParseFailed(err.to_string()),
            }
        })?;
        let shaped = to_parslet_compatible(&raw, &mut arena, input);
        Ok(ast_to_value(&shaped, &arena, input))
    }

    /// Parse and bind in one step, mirroring `Artifact#parse_and_bind`.
    pub fn parse_and_bind(&self, entry: &str, input: &str) -> Result<Value, PargError> {
        let shape = self.parse_shape(entry, input)?;
        self.apply_bindings(entry, &shape)
    }

    /// Best-effort recovery (parsanol-ruby TODO.sota/1, R1): on success
    /// returns `{"ok": true, "shape": ...}` exactly like `parse_shape`;
    /// on failure returns `{"ok": false, "captures": ..., "error": ...}`
    /// where `captures` carries every capture committed before the
    /// failure (spans as the standard slice wire objects) and `error`
    /// is the failure wire (offset/line/column/expected). Zero extra
    /// cost on success paths — the recovery envelope is built only
    /// from state the failed parse already left behind.
    pub fn parse_shape_recovering(&self, entry: &str, input: &str) -> Result<Value, PargError> {
        let grammar: Grammar = self.grammar(entry)?;
        let mut arena = AstArena::for_input(input.len().max(1 << 16));
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        match parser.parse() {
            Ok(_) => {
                let shape = self.parse_shape(entry, input)?;
                Ok(serde_json::json!({ "ok": true, "shape": shape }))
            }
            Err(_) => {
                // The generational undo log empties the visible store
                // on the failing branch — that rollback IS the
                // correctness mechanism. Recovery re-parses once with
                // journaling enabled: the journal observes every
                // capture, including writes the failed attempt rolled
                // back. Success paths never journal (zero cost).
                let mut arena2 = AstArena::for_input(input.len().max(1 << 16));
                let mut journaling = PortableParser::new(&grammar, input, &mut arena2);
                journaling.capture_state_mut().set_journaling(true);
                journaling.set_track_max_end(true);
                journaling.set_collect_partial(true);
                let _ = journaling.parse();
                let mut partial = journaling.take_partial_stash();
                let journal: Vec<(String, CaptureValue)> =
                    journaling.capture_state().journal().to_vec();
                let consumed_upto = journaling.max_end_pos();
                let failure_wire = journaling.failure_wire();
                // R2 (TODO.sota/1), inferred resync — no grammar
                // addition: from the failure, scan forward for the next
                // byte offset where a spine iteration matches; the
                // skipped span becomes an error annotation and parsing
                // continues. Bounded (MAX_RESYNCS rounds, each must
                // advance) so recovery stays linear in the input.
                let spine_body = match grammar.get_atom(grammar.root) {
                    Some(Atom::Repetition { atom, .. }) => Some(*atom),
                    Some(Atom::Named { atom, .. }) => match grammar.get_atom(*atom) {
                        Some(Atom::Repetition { atom, .. }) => Some(*atom),
                        _ => None,
                    },
                    _ => None,
                };
                let mut resyncs: Vec<Value> = Vec::new();
                // The spine boundary, not the failure offset, anchors
                // the resync scan: junk frequently lies BEFORE the
                // deepest failure (the failed iteration consumed its
                // own prefix), so a scan from the failure offset can
                // never match it. With no matched prefix the boundary
                // is 0 - a declared pattern is usable from the start
                // (rs#304 Gap B, the ruby engine's convergence twin).
                if let Some(body) = spine_body {
                    let stash_was_empty = partial.is_none();
                    let mut fresh_items: Vec<AstNode> = Vec::new();
                    let mut cursor = match partial.as_ref() {
                        Some((end, _, _)) => *end,
                        None => 0,
                    };
                    let items: &mut Vec<AstNode> = match partial.as_mut() {
                        Some((_, items, _)) => items,
                        None => &mut fresh_items,
                    };
                    // Declared resync (`resync <rule> = <pattern>`, R2):
                    // the pattern is baked into the entry; the skip spans
                    // are exactly the pattern's matches — the author owns
                    // them, deterministically. When no pattern is
                    // declared, the inferred trial scan applies.
                    let declared_grammar: Option<Grammar> = self
                        .entry_value(entry, "resync")
                        .cloned()
                        .map(|value| serde_json::from_value::<Grammar>(value))
                        .transpose()
                        .ok()
                        .flatten();
                    // The pattern parser gets its own arena: the
                    // journaling parser holds arena2 for the whole
                    // recovery pass.
                    let mut pattern_arena = AstArena::for_input(input.len().max(1 << 16));
                    let mut pattern_parser = declared_grammar
                        .as_ref()
                        .map(|grammar| PortableParser::new(grammar, input, &mut pattern_arena));
                    'rounds: for _ in 0..MAX_RESYNCS {
                        // An iteration may already be viable at the
                        // cursor (e.g. right after a declared skip).
                        if let Ok(r) = journaling.try_atom_impl(body, cursor, false) {
                            if r.end_pos > cursor {
                                items.push(r.value.clone());
                                cursor = r.end_pos;
                                loop {
                                    match journaling.try_atom_impl(body, cursor, false) {
                                        Ok(r) if r.end_pos > cursor => {
                                            items.push(r.value.clone());
                                            cursor = r.end_pos;
                                        }
                                        _ => break,
                                    }
                                }
                                if cursor >= input.len() {
                                    break;
                                }
                            }
                        }
                        // Skip phase.
                        if let Some(pattern) = pattern_parser.as_mut() {
                            // Declared: scan for the next pattern match;
                            // the whole match (gap + delimiter) is the
                            // annotated error span.
                            let mut scan = cursor;
                            let mut found = None;
                            while scan < input.len() {
                                if let Ok(r) = pattern.parse_from_pos(scan) {
                                    if r.end_pos > scan {
                                        found = Some(r.end_pos);
                                        break;
                                    }
                                }
                                scan += 1;
                            }
                            let Some(resume) = found else {
                                break 'rounds;
                            };
                            resyncs.push(serde_json::json!({
                                "offset": cursor,
                                "skipped": &input[cursor..resume],
                                "resumed_at": resume,
                            }));
                            cursor = resume;
                        } else {
                            // Inferred: scan for the next viable
                            // iteration start.
                            let mut found = None;
                            let mut scan = cursor;
                            while scan < input.len() {
                                if let Ok(r) = journaling.try_atom_impl(body, scan, false) {
                                    if r.end_pos > scan {
                                        found = Some((scan, r.end_pos, r.value.clone()));
                                        break;
                                    }
                                }
                                scan += 1;
                            }
                            let Some((skip_start, resume, item)) = found else {
                                break 'rounds;
                            };
                            resyncs.push(serde_json::json!({
                                "offset": cursor,
                                "skipped": &input[cursor..skip_start],
                                "resumed_at": skip_start,
                            }));
                            items.push(item);
                            cursor = resume;
                        }
                    }
                    if let Some((end, _, _)) = partial.as_mut() {
                        *end = cursor;
                    }
                    if stash_was_empty && !fresh_items.is_empty() {
                        partial = Some((cursor, fresh_items, RepetitionTag::Repetition));
                    }
                }
                // The entry shape is wrapped under the root rule's
                // name (the shape contract), so the partial wraps the
                // same way.
                let root_is_named = self.entry_root(entry).map(str::to_string).ok();
                drop(journaling);
                Ok(recovery_shape(
                    entry,
                    &journal,
                    consumed_upto,
                    failure_wire,
                    input,
                    &mut arena2,
                    partial,
                    root_is_named,
                    resyncs,
                ))
            }
        }
    }

    /// Run the artifact's embedded tests; empty result means green.
    pub fn run_tests(&self) -> Vec<String> {
        let tests: Vec<Value> = self
            .envelope
            .get("tests")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        self.run_test_list(&tests)
    }

    /// Run an external test list (artifact-shaped test hashes).
    /// The compiler-baked default entry (declaration order, not the
    /// sorted view JSON engines see).
    pub fn default_entry(&self) -> Option<&str> {
        self.envelope.get("default_entry").and_then(Value::as_str)
    }

    /// Evaluate artifact-shaped tests (accept/reject/example); returns
    /// Ruby-word-compatible failure descriptions. Empty means green.
    pub fn run_test_list(&self, tests: &[Value]) -> Vec<String> {
        let fallback = self
            .default_entry()
            .or_else(|| self.entry_names().first().copied())
            .unwrap_or_default()
            .to_string();
        tests
            .iter()
            .filter_map(|test| {
                let object = test.as_object()?;
                let kind = object
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or("accept");
                let input = object
                    .get("input")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let entry = object
                    .get("entry")
                    .and_then(Value::as_str)
                    .unwrap_or(&fallback);
                let outcome = (|| -> Result<(), PargError> {
                    let shape = self.parse_shape(entry, input)?;
                    if kind == "reject" {
                        return Err(PargError::UnexpectedParse(input.to_string()));
                    }
                    if kind == "example" {
                        let bound = self.apply_bindings(entry, &shape)?;
                        let Some(expect) = object.get("expect").and_then(Value::as_object) else {
                            return Ok(());
                        };
                        let mismatched: Vec<(&String, &Value)> = expect
                            .iter()
                            .filter(|(key, value)| {
                                canonical(bound.get(*key).unwrap_or(&Value::Null))
                                    != canonical(value)
                            })
                            .collect();
                        if !mismatched.is_empty() {
                            return Err(PargError::CaptureMismatch {
                                input: input.to_string(),
                                detail: format!("expected captures {mismatched:?}, got {bound}"),
                            });
                        }
                    }
                    if kind == "shape" {
                        // Whole-tree assertion: the expectation is the
                        // canonical flattened tree, not a capture map. This
                        // is the phase that can observe repetition flattening
                        // and sequence merge laws.
                        let expected = canonical(object.get("expect").unwrap_or(&Value::Null));
                        let actual = canonical(&shape);
                        if actual != expected {
                            return Err(PargError::CaptureMismatch {
                                input: input.to_string(),
                                detail: format!("expected shape {expected:?}, got {actual:?}"),
                            });
                        }
                    }
                    if kind == "capture" {
                        let Some(expect) = object.get("expect").and_then(Value::as_object) else {
                            return Ok(());
                        };
                        let captures = super::bindings::collect_captures(&shape);
                        let mismatched: Vec<(&String, &Value)> = expect
                            .iter()
                            .filter(|(key, expected)| {
                                // A present nil capture (for example an absent
                                // optional inside `as`) is a value, not a
                                // missing capture: compare presence first.
                                let actual =
                                    captures.get(key.as_str()).and_then(|values| values.first());
                                !matches!(actual,
                                    Some(actual) if canonical(actual) == canonical(expected))
                            })
                            .collect();
                        if !mismatched.is_empty() {
                            let actuals: serde_json::Map<String, Value> = mismatched
                                .iter()
                                .map(|(key, _)| {
                                    let actual = captures
                                        .get(key.as_str())
                                        .and_then(|values| values.first())
                                        .map(canonical)
                                        .unwrap_or(Value::Null);
                                    ((*key).clone(), actual)
                                })
                                .collect();
                            return Err(PargError::CaptureMismatch {
                                input: input.to_string(),
                                detail: format!(
                                    "expected captures {mismatched:?}, got {actuals:?}"
                                ),
                            });
                        }
                    }
                    Ok(())
                })();
                match outcome {
                    Ok(()) => None,
                    // A reject test passing because parsing failed is green.
                    Err(PargError::ParseFailed(_) | PargError::ParseWire { .. })
                        if kind == "reject" =>
                    {
                        None
                    }
                    Err(err) => Some(format!("test {input:?}: {err}")),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::canonical;
    use crate::PargArtifact;
    use serde_json::Value;

    #[test]
    fn canonical_reduces_slice_wire_objects_to_text() {
        let slice = serde_json::json!({
            "value": "ab", "line": 1, "column": 1, "offset": 0, "length": 2
        });
        assert_eq!(canonical(&slice), Value::String("ab".into()));
    }

    #[test]
    fn canonical_recurses_structure_and_keeps_scalars() {
        let value = serde_json::json!({
            "name": {"value": "ab", "line": 1, "column": 1, "offset": 0, "length": 2},
            "year": 2020,
            "ok": true,
            "items": [{"value": "x", "line": 1, "column": 2, "offset": 1, "length": 1}]
        });
        assert_eq!(
            canonical(&value),
            serde_json::json!({ "name": "ab", "year": 2020, "ok": true, "items": ["x"] })
        );
    }

    #[test]
    fn canonical_leaves_plain_objects_intact() {
        let value = serde_json::json!({ "a": { "b": "c" } });
        assert_eq!(canonical(&value), value);
    }

    #[test]
    fn capture_kind_replays_green_against_ruby_compiled_fixture() {
        let artifact =
            PargArtifact::from_json(include_str!("../../tests/fixtures/parg_capture.json"))
                .expect("ruby-compiled fixture must verify");
        assert!(artifact.run_tests().is_empty());
    }

    #[test]
    fn recovering_returns_the_shape_on_success() {
        let artifact =
            PargArtifact::from_json(include_str!("../../tests/fixtures/parg_capture.json"))
                .expect("ruby-compiled fixture must verify");
        let recovered = artifact
            .parse_shape_recovering("identifier", "AB2020")
            .unwrap();
        assert_eq!(recovered["ok"], Value::Bool(true));
        assert_eq!(
            recovered["shape"],
            artifact.parse_shape("identifier", "AB2020").unwrap()
        );
    }

    #[test]
    fn recovering_reports_committed_captures_on_failure() {
        let artifact =
            PargArtifact::from_json(include_str!("../../tests/fixtures/parg_capture.json"))
                .expect("ruby-compiled fixture must verify");
        // "AB20": `pub` matches "AB", the digits match "2" and "0",
        // then `4digit` wants a third digit at EOF and the sequence
        // fails. The furthest successful match ends at EOF — the
        // failure wire points there too. Rule-level captures are
        // tree-level in the portable engine and do not survive the
        // failed sequence: the recovery envelope is failure forensics
        // (R1), with partial-tree recovery specced as the engine
        // capability in TODO.sota/1.
        let recovered = artifact
            .parse_shape_recovering("identifier", "AB20")
            .unwrap();
        assert_eq!(recovered["ok"], Value::Bool(false));
        assert_eq!(recovered["consumed_upto"], Value::Number(4.into()));
        assert_eq!(recovered["error"]["offset"], Value::Number(4.into()));
        assert!(
            recovered["error"]["expected"]
                .as_array()
                .is_some_and(|e| !e.is_empty()),
            "expected set present: {recovered}"
        );
    }

    #[test]
    fn capture_kind_reports_mismatched_expectations() {
        let artifact =
            PargArtifact::from_json(include_str!("../../tests/fixtures/parg_capture.json"))
                .expect("ruby-compiled fixture must verify");
        let mut tests: Vec<Value> =
            serde_json::from_str(artifact.envelope.get("tests").unwrap().to_string().as_str())
                .unwrap();
        // Corrupt the capture expectation: "AB" -> "ZZ" (same shape).
        for test in tests.iter_mut() {
            if test["kind"] == "capture" {
                test["expect"]["pub"] = Value::String("ZZ".into());
            }
        }
        let failures = artifact.run_test_list(&tests);
        assert_eq!(failures.len(), 1, "{failures:?}");
        assert!(failures[0].contains("expected captures"), "{failures:?}");
    }
}
