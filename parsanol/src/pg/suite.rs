//! Suite runner: evaluates accept/reject/example tests against an
//! artifact with the native portable engine plus the bindings runtime.
//! Port of `Parsanol::PG::Artifact#run_tests` / `#run_test_list` —
//! failure descriptions are word-compatible with the Ruby runner.

use serde_json::Value;

use super::{PgArtifact, PgError};
use crate::portable::parslet_transform::to_parslet_compatible;
use crate::portable::{AstArena, Grammar, PortableParser};

fn ast_to_value(node: &crate::portable::ast::AstNode, arena: &AstArena, input: &str) -> Value {
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
            serde_json::json!({
                "value": value,
                "line": line,
                "column": column,
                "offset": offset,
                "length": length,
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

impl PgArtifact {
    /// Parse an entry with the native portable engine and return the
    /// parsanol-shape tree as JSON.
    pub fn parse_shape(&self, entry: &str, input: &str) -> Result<Value, PgError> {
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
                PgError::ParseWire { offset, line, column, expected, ranked }
            }
            None => PgError::ParseFailed(err.to_string()),
        })?;
        let shaped = to_parslet_compatible(&raw, &mut arena, input);
        Ok(ast_to_value(&shaped, &arena, input))
    }

    /// Parse and bind in one step, mirroring `Artifact#parse_and_bind`.
    pub fn parse_and_bind(&self, entry: &str, input: &str) -> Result<Value, PgError> {
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
                let entry = object.get("entry").and_then(Value::as_str).unwrap_or(&fallback);
                let outcome = (|| -> Result<(), PgError> {
                    let bound = self.parse_and_bind(entry, input)?;
                    if kind == "reject" {
                        return Err(PgError::UnexpectedParse(input.to_string()));
                    }
                    if kind == "example" {
                        let Some(expect) = object.get("expect").and_then(Value::as_object) else {
                            return Ok(());
                        };
                        let mismatched: Vec<(&String, &Value)> = expect
                            .iter()
                            .filter(|(key, value)| bound.get(*key) != Some(*value))
                            .collect();
                        if !mismatched.is_empty() {
                            return Err(PgError::CaptureMismatch {
                                input: input.to_string(),
                                detail: format!("expected captures {mismatched:?}, got {bound}"),
                            });
                        }
                    }
                    Ok(())
                })();
                match outcome {
                    Ok(()) => None,
                    // A reject test passing because parsing failed is green.
                    Err(PgError::ParseFailed(_) | PgError::ParseWire { .. }) if kind == "reject" => {
                        None
                    }
                    Err(err) => Some(format!("test {input:?}: {err}")),
                }
            })
            .collect()
    }
}
