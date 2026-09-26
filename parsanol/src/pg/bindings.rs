//! Bindings runtime: applies an entry's artifact bindings to a
//! parsanol-shape parse tree, producing the attribute map that fills a
//! data-model class. Pipeline: capture collection -> preprocessing ->
//! type cast -> path assignment. Byte-compatible with
//! `Parsanol::PG::Bindings` — the parity fixture
//! (`tests/fixtures/pg_bindings.json`) pins the Ruby outputs.

use serde_json::{Map, Value};
use std::collections::BTreeMap;

use super::{PgArtifact, PgError};

/// Ruby `Integer(value)` semantics for the JSON values bindings see:
/// strings with optional `0x`/`0b`/`0o` radix and underscores, JSON
/// integers, and nothing else (bools/floats raise).
fn cast_integer(value: &Value) -> Result<Value, PgError> {
    let parsed = match value {
        Value::Number(n) => n
            .as_i64()
            .ok_or_else(|| PgError::Cast("integer".to_string(), value.to_string()))?,
        Value::String(s) => {
            let (negative, body) = match s.strip_prefix('-') {
                Some(rest) => (true, rest),
                None => (false, s.as_str()),
            };
            let (radix, digits) = if let Some(rest) = body.strip_prefix("0x") {
                (16, rest)
            } else if let Some(rest) = body.strip_prefix("0b") {
                (2, rest)
            } else if let Some(rest) = body.strip_prefix("0o") {
                (8, rest)
            } else {
                (10, body)
            };
            let cleaned = digits.replace('_', "");
            let magnitude = i128::from_str_radix(&cleaned, radix)
                .map_err(|_| PgError::Cast("integer".to_string(), s.clone()))?;
            let signed = if negative { -magnitude } else { magnitude };
            i64::try_from(signed).map_err(|_| PgError::Cast("integer".to_string(), s.clone()))?
        }
        _ => return Err(PgError::Cast("integer".to_string(), value.to_string())),
    };
    Ok(Value::Number(parsed.into()))
}

/// Ruby `Float(value)`: JSON floats/ints and numeric strings.
fn cast_float(value: &Value) -> Result<Value, PgError> {
    let parsed = match value {
        Value::Number(n) => n.as_f64().unwrap_or_default(),
        Value::String(s) => s
            .trim()
            .parse::<f64>()
            .map_err(|_| PgError::Cast("float".to_string(), s.clone()))?,
        _ => return Err(PgError::Cast("float".to_string(), value.to_string())),
    };
    Ok(serde_json::Number::from_f64(parsed)
        .map(Value::Number)
        .unwrap_or_else(|| Value::Null))
}

/// Ruby `value.to_s` for the JSON values bindings see.
fn to_ruby_string(value: &Value) -> String {
    to_ruby_string_inner(leaf_scalar(value))
}

fn to_ruby_string_inner(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.to_string()
            } else {
                let f = n.as_f64().unwrap_or_default();
                if f.fract() == 0.0 && f.is_finite() {
                    format!("{f:.1}")
                } else {
                    f.to_string()
                }
            }
        }
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        _ => String::new(), // unreachable: non-scalars are refused by the caller
    }
}

fn cast(value: Value, kind: Option<&str>) -> Result<Value, PgError> {
    let value = if is_leaf(&value) {
        leaf_scalar(&value).clone()
    } else {
        value
    };
    match kind {
        Some("integer") => cast_integer(&value),
        Some("float") => cast_float(&value),
        Some("string") => match &value {
            Value::String(_) | Value::Number(_) | Value::Bool(_) | Value::Null => {
                Ok(Value::String(to_ruby_string(&value)))
            }
            _ => Err(PgError::Cast("string".to_string(), value.to_string())),
        },
        Some("boolean") => Ok(Value::Bool(
            value == Value::Bool(true)
                || matches!(&value, Value::String(s) if s.eq_ignore_ascii_case("true")),
        )),
        _ => Ok(value),
    }
}

/// BFS capture collection, identical traversal order to Ruby's queue:
/// hash values are recorded under their key then enqueued; array
/// elements are enqueued in order.
fn collect_captures(shape: &Value) -> BTreeMap<String, Vec<Value>> {
    let mut found: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut queue: Vec<&Value> = vec![shape];
    while let Some(node) = queue.first().copied() {
        queue.remove(0);
        match node {
            Value::Object(object) => {
                for (key, value) in object {
                    found.entry(key.clone()).or_default().push(value.clone());
                    if !is_leaf(value) {
                        queue.push(value);
                    }
                }
            }
            Value::Array(items) => queue.extend(items),
            _ => {}
        }
    }
    found
}

/// Native capture leaves arrive as `{value, line, column, offset, length}`
/// objects. Ruby's FFI hands Bindings opaque leaf objects whose `to_s` is
/// the captured text and whose internals never join the capture map; the
/// JSON-serialized equivalent is an object carrying a `value` field.
fn is_leaf(value: &Value) -> bool {
    value
        .as_object()
        .is_some_and(|object| object.contains_key("value"))
}

fn leaf_scalar(value: &Value) -> &Value {
    if is_leaf(value) {
        value
            .as_object()
            .and_then(|object| object.get("value"))
            .unwrap_or(value)
    } else {
        value
    }
}

fn first_capture(captures: &BTreeMap<String, Vec<Value>>, capture: &str) -> Option<Value> {
    captures.get(capture)?.first().cloned()
}

fn leaf_key(path: &str) -> String {
    if path.contains("[]") {
        path.rsplit("].").next().unwrap_or(path).to_string()
    } else {
        path.to_string()
    }
}

fn validate_path(capture: &str, path: &str) -> Result<(), PgError> {
    if path.contains('.') || path.contains('[') {
        return Err(PgError::NestedBindingPath {
            capture: capture.to_string(),
            path: path.to_string(),
        });
    }
    Ok(())
}

impl PgArtifact {
    /// The binding list of an entry (empty when the entry declares none).
    pub fn bindings(&self, entry: &str) -> &[Value] {
        self.entry_value(entry, "bindings")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    fn preprocess_steps(&self, name: &str) -> Result<&Vec<Value>, PgError> {
        self.envelope
            .get("preprocess")
            .and_then(|steps| steps.get(name))
            .and_then(Value::as_array)
            .ok_or_else(|| PgError::UnknownPreprocess(name.to_string()))
    }

    /// Declared table rows (embedded in the envelope at compile time).
    pub fn table_rows(&self, name: &str) -> Result<&Vec<Value>, PgError> {
        let declared = self
            .envelope
            .get("tables")
            .and_then(|tables| tables.get(name))
            .ok_or_else(|| PgError::UnknownTable(name.to_string()))?;
        if !declared.is_object() {
            return Err(PgError::UnknownTable(name.to_string()));
        }
        declared
            .get("rows")
            .and_then(Value::as_array)
            .ok_or_else(|| PgError::UnknownTable(name.to_string()))
    }

    fn preprocess(&self, binding: &Map<String, Value>, value: Value) -> Result<Value, PgError> {
        let Some(name) = binding.get("preprocess").and_then(Value::as_str) else {
            return Ok(value);
        };
        let mut value = value;
        for step in self.preprocess_steps(name)? {
            let op = step
                .get("op")
                .and_then(Value::as_str)
                .ok_or_else(|| PgError::UnknownPreprocessOp("<missing>".to_string()))?;
            if op != "table_lookup" {
                return Err(PgError::UnknownPreprocessOp(op.to_string()));
            }
            let table = step
                .get("table")
                .and_then(Value::as_str)
                .ok_or_else(|| PgError::UnknownPreprocessOp(op.to_string()))?;
            let from = step
                .get("from")
                .and_then(Value::as_str)
                .ok_or_else(|| PgError::UnknownPreprocessOp(op.to_string()))?;
            let to = step
                .get("to")
                .and_then(Value::as_str)
                .ok_or_else(|| PgError::UnknownPreprocessOp(op.to_string()))?;
            let needle = to_ruby_string(&value);
            let mut mapped = None;
            for row in self.table_rows(table)? {
                let Some(row) = row.as_object() else {
                    continue;
                };
                let key = row.get(from).map(to_ruby_string);
                if key.as_deref() == Some(needle.as_str()) {
                    mapped = row.get(to).cloned();
                    break;
                }
            }
            if let Some(mapped) = mapped {
                value = mapped;
            }
        }
        Ok(value)
    }

    fn finalize(&self, binding: &Map<String, Value>, value: Value) -> Result<Value, PgError> {
        let kind = binding.get("type").and_then(Value::as_str);
        cast(self.preprocess(binding, value)?, kind)
    }

    fn assign_scalar(
        &self,
        out: &mut Map<String, Value>,
        binding: &Map<String, Value>,
        captures: &BTreeMap<String, Vec<Value>>,
    ) -> Result<(), PgError> {
        let capture = binding
            .get("capture")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let path = binding
            .get("path")
            .map(|p| p.as_str().unwrap_or_default().to_string())
            .unwrap_or_default();
        validate_path(capture, &path)?;
        let Some(value) = first_capture(captures, capture) else {
            return Ok(());
        };
        let finalized = self.finalize(binding, value)?;
        out.insert(leaf_key(&path), finalized);
        Ok(())
    }

    fn assign_array(
        &self,
        out: &mut Map<String, Value>,
        prefix: &str,
        group: &[&Map<String, Value>],
        captures: &BTreeMap<String, Vec<Value>>,
    ) -> Result<(), PgError> {
        let count = group
            .iter()
            .filter_map(|binding| {
                let capture = binding
                    .get("capture")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                captures.get(capture).map(Vec::len)
            })
            .max()
            .unwrap_or(0);
        if count == 0 {
            return Ok(());
        }
        let mut list: Vec<Value> = Vec::with_capacity(count);
        for index in 0..count {
            let mut element = Map::new();
            for binding in group {
                let capture = binding
                    .get("capture")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let Some(value) = captures.get(capture).and_then(|c| c.get(index)) else {
                    continue;
                };
                let finalized = self.finalize(binding, value.clone())?;
                let path = binding
                    .get("path")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                element.insert(leaf_key(path), finalized);
            }
            list.push(Value::Object(element));
        }
        out.insert(prefix.to_string(), Value::Array(list));
        Ok(())
    }

    /// Apply an entry's bindings to a parsanol-shape parse tree.
    pub fn apply_bindings(&self, entry: &str, shape: &Value) -> Result<Value, PgError> {
        let bindings = self.bindings(entry);
        let captures = collect_captures(shape);
        let mut scalars: Vec<&Map<String, Value>> = Vec::new();
        let mut arrays: Vec<(&str, &Map<String, Value>)> = Vec::new();
        for binding in bindings {
            let Some(object) = binding.as_object() else {
                continue;
            };
            let path = object
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if path.contains("[]") {
                let prefix = path.split("[]").next().unwrap_or(path);
                arrays.push((prefix, object));
            } else {
                scalars.push(object);
            }
        }

        let mut out = Map::new();
        for binding in &scalars {
            self.assign_scalar(&mut out, binding, &captures)?;
        }
        // Ruby groups by prefix while preserving first-seen order.
        let mut order: Vec<&str> = Vec::new();
        for (prefix, _) in &arrays {
            if !order.contains(prefix) {
                order.push(prefix);
            }
        }
        for prefix in order {
            let group: Vec<&Map<String, Value>> = arrays
                .iter()
                .filter(|(candidate, _)| *candidate == prefix)
                .map(|(_, object)| *object)
                .collect();
            self.assign_array(&mut out, prefix, &group, &captures)?;
        }
        Ok(Value::Object(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cast_integer_matches_ruby() {
        assert_eq!(
            cast_integer(&Value::Number(12.into())).unwrap(),
            Value::Number(12.into())
        );
        assert_eq!(
            cast_integer(&Value::String("42".into())).unwrap(),
            Value::Number(42.into())
        );
        assert_eq!(
            cast_integer(&Value::String("0x1A".into())).unwrap(),
            Value::Number(26.into())
        );
        assert_eq!(
            cast_integer(&Value::String("1_000".into())).unwrap(),
            Value::Number(1000.into())
        );
        assert!(cast_integer(&Value::String("12.5".into())).is_err());
        assert!(cast_integer(&Value::Bool(true)).is_err());
    }

    #[test]
    fn cast_boolean_matches_ruby() {
        assert_eq!(
            cast(Value::Bool(true), Some("boolean")).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            cast(Value::String("TRUE".into()), Some("boolean")).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            cast(Value::String("true".into()), Some("boolean")).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            cast(Value::String("yes".into()), Some("boolean")).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            cast(Value::Number(1.into()), Some("boolean")).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn cast_string_matches_ruby_to_s() {
        assert_eq!(
            cast(Value::Number(12.into()), Some("string")).unwrap(),
            Value::String("12".into())
        );
        assert_eq!(
            cast(
                Value::Number(serde_json::Number::from_f64(12.0).unwrap()),
                Some("string")
            )
            .unwrap(),
            Value::String("12.0".into())
        );
        assert_eq!(
            cast(Value::Bool(false), Some("string")).unwrap(),
            Value::String("false".into())
        );
    }

    #[test]
    fn collect_captures_walks_nested_trees() {
        let shape: Value = serde_json::json!({
            "a": "1",
            "b": { "a": "2", "c": [{ "a": "3" }] }
        });
        let captures = collect_captures(&shape);
        assert_eq!(
            captures.get("a").unwrap(),
            &vec![
                Value::String("1".into()),
                Value::String("2".into()),
                Value::String("3".into())
            ]
        );
    }

    #[test]
    fn nested_binding_paths_are_rejected() {
        assert!(validate_path("y", "a.b").is_err());
        assert!(validate_path("y", "items[]").is_err());
        assert!(validate_path("y", "year").is_ok());
    }
}
