//! Suite runner: evaluates accept/reject/example/capture tests against
//! an artifact with the native portable engine plus the bindings
//! runtime. Port of `Parsanol::PARG::Artifact#run_tests` /
//! `#run_test_list` — failure descriptions are word-compatible with the
//! Ruby runner. `capture` is the grammar-layer phase (raw captures);
//! `example` the bindings layer (bound attribute hash).

use serde_json::Value;

use super::bindings::{is_leaf, leaf_scalar};
use super::{PargArtifact, PargError};
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

impl PargArtifact {
    /// Parse an entry with the native portable engine and return the
    /// parsanol-shape tree as JSON.
    pub fn parse_shape(&self, entry: &str, input: &str) -> Result<Value, PargError> {
        let grammar: Grammar = self.grammar(entry)?;
        // The shaped tree can dwarf the input (large grammars bind few
        // characters per node); size the arena for the tree, not the text.
        let mut arena = AstArena::for_input(input.len().max(1 << 16));
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        let raw = parser.parse().map_err(|err| match parser.failure_wire() {
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
        })?;
        let shaped = to_parslet_compatible(&raw, &mut arena, input);
        Ok(ast_to_value(&shaped, &arena, input))
    }

    /// Parse and bind in one step, mirroring `Artifact#parse_and_bind`.
    pub fn parse_and_bind(&self, entry: &str, input: &str) -> Result<Value, PargError> {
        let shape = self.parse_shape(entry, input)?;
        self.apply_bindings(entry, &shape)
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
                    if kind == "capture" {
                        let Some(expect) = object.get("expect").and_then(Value::as_object) else {
                            return Ok(());
                        };
                        let captures = super::bindings::collect_captures(&shape);
                        let mismatched: Vec<(&String, &Value)> = expect
                            .iter()
                            .filter(|(key, expected)| {
                                let actual =
                                    captures.get(key.as_str()).and_then(|values| values.first());
                                !matches!(actual,
                                    Some(actual) if !actual.is_null()
                                        && canonical(actual) == canonical(expected))
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
