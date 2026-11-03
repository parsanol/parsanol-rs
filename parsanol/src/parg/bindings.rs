//! Bindings runtime: applies an entry's artifact bindings to a
//! parsanol-shape parse tree, producing the attribute map that fills a
//! data-model class. Pipeline: capture collection -> preprocessing ->
//! type cast -> path assignment. Byte-compatible with
//! `Parsanol::PARG::Bindings` — the parity fixture
//! (`tests/fixtures/parg_bindings.json`) pins the Ruby outputs.

use serde_json::{Map, Value};
use std::collections::BTreeMap;

use super::{PargArtifact, PargError};

/// Ruby `Integer(value)` semantics for the JSON values bindings see:
/// strings with optional `0x`/`0b`/`0o` radix and underscores, JSON
/// integers, and nothing else (bools/floats raise).
fn cast_integer(value: &Value) -> Result<Value, PargError> {
    let parsed = match value {
        Value::Number(n) => n
            .as_i64()
            .ok_or_else(|| PargError::Cast("integer".to_string(), value.to_string()))?,
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
                .map_err(|_| PargError::Cast("integer".to_string(), s.clone()))?;
            let signed = if negative { -magnitude } else { magnitude };
            i64::try_from(signed).map_err(|_| PargError::Cast("integer".to_string(), s.clone()))?
        }
        _ => return Err(PargError::Cast("integer".to_string(), value.to_string())),
    };
    Ok(Value::Number(parsed.into()))
}

/// Ruby `Float(value)`: JSON floats/ints and numeric strings.
fn cast_float(value: &Value) -> Result<Value, PargError> {
    let parsed = match value {
        Value::Number(n) => n.as_f64().unwrap_or_default(),
        Value::String(s) => s
            .trim()
            .parse::<f64>()
            .map_err(|_| PargError::Cast("float".to_string(), s.clone()))?,
        _ => return Err(PargError::Cast("float".to_string(), value.to_string())),
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

fn cast(value: Value, kind: Option<&str>) -> Result<Value, PargError> {
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
            _ => Err(PargError::Cast("string".to_string(), value.to_string())),
        },
        Some("boolean") => Ok(Value::Bool(
            value == Value::Bool(true)
                || matches!(&value, Value::String(s) if s.eq_ignore_ascii_case("true")),
        )),
        // The ONE plain shape on every engine (ruby#383): capture
        // leaves and scalars render as their text, named groups as
        // string-keyed plain hashes, repeats as arrays — so (any)-bound
        // keys survive a cross-engine gate without consumer shims.
        Some("any") => Ok(plain_shape(&value)),
        _ => Ok(value),
    }
}

/// The engine-neutral reduction behind `(any)`: wire capture leaves
/// (`{"value": …, …}`) render as their value's text, scalars as
/// strings, objects as string-keyed plain objects, arrays recurse.
/// Null passes through (absent captures stay absent — the assign
/// paths skip them upstream).
fn plain_shape(value: &Value) -> Value {
    match value {
        Value::Null => Value::Null,
        Value::Object(object) => match object.get("value") {
            // The wire capture leaf: its value, as text.
            Some(leaf_value) if object.contains_key("offset") => match leaf_value {
                Value::String(text) => Value::String(text.clone()),
                other => Value::String(match other {
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => other.to_string(),
                }),
            },
            _ => Value::Object(
                object
                    .iter()
                    .map(|(key, item)| (key.clone(), plain_shape(item)))
                    .collect(),
            ),
        },
        Value::Array(items) => Value::Array(items.iter().map(plain_shape).collect()),
        Value::String(text) => Value::String(text.clone()),
        other => Value::String(other.to_string()),
    }
}

/// BFS capture collection, identical traversal order to Ruby's queue:
/// hash values are recorded under their key then enqueued; array
/// elements are enqueued in order.
pub(crate) fn collect_captures(shape: &Value) -> BTreeMap<String, Vec<Value>> {
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
pub(crate) fn is_leaf(value: &Value) -> bool {
    value
        .as_object()
        .is_some_and(|object| object.contains_key("value"))
}

pub(crate) fn leaf_scalar(value: &Value) -> &Value {
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

/// The `prefix[]` group's instance subtrees (ruby#381): the array
/// under a key naming the prefix, walked in collect_captures BFS
/// order. There is deliberately NO fallback for a repetition-root
/// entry — a bare top array cannot be told apart from per-token
/// rule siblings (ruby#408/#412: claiming it split paired captures
/// into single-field rows and sprouted phantom rows for unrelated
/// siblings), so those shapes pair by occurrence instead — the
/// 1.3.114 baseline ruby settled on (ruby#411/#419).
fn group_instances<'a>(shape: &'a Value, prefix: &str) -> Option<&'a Vec<Value>> {
    let mut queue: Vec<&Value> = vec![shape];
    while let Some(node) = queue.pop() {
        match node {
            Value::Object(object) => {
                for (key, value) in object {
                    if key == prefix {
                        if let Value::Array(array) = value {
                            return Some(array);
                        }
                    }
                    if value.is_object() || value.is_array() {
                        queue.push(value);
                    }
                }
            }
            Value::Array(items) => queue.extend(items),
            _ => {}
        }
    }
    None
}

fn leaf_key(path: &str) -> String {
    if path.contains("[]") {
        path.rsplit("].").next().unwrap_or(path).to_string()
    } else {
        path.to_string()
    }
}

fn validate_path(capture: &str, path: &str) -> Result<(), PargError> {
    if path.contains('.') || path.contains('[') {
        return Err(PargError::NestedBindingPath {
            capture: capture.to_string(),
            path: path.to_string(),
        });
    }
    Ok(())
}

/// The execution resources the apply core resolves against: the
/// preprocess step declarations and table rows of the PAIRED parse
/// artifact (parsanol-ruby#432 — the declared bindings artifact
/// carries records only; its apply borrows the paired artifact's
/// checksummed envelope).
pub(crate) struct ApplyEnv<'a> {
    pub preprocess: &'a Value,
    pub tables: &'a Value,
}

static NULL_VALUE: Value = Value::Null;

impl<'a> ApplyEnv<'a> {
    /// The env of a parse artifact: both resources resolve from its
    /// checksummed envelope.
    pub fn of_artifact(envelope: &'a Value) -> Self {
        ApplyEnv {
            preprocess: envelope.get("preprocess").unwrap_or(&NULL_VALUE),
            tables: envelope.get("tables").unwrap_or(&NULL_VALUE),
        }
    }
}

fn preprocess_steps<'a>(env: &ApplyEnv<'a>, name: &str) -> Result<&'a Vec<Value>, PargError> {
    env.preprocess
        .get(name)
        .and_then(Value::as_array)
        .ok_or_else(|| PargError::UnknownPreprocess(name.to_string()))
}

fn table_rows_of<'a>(env: &ApplyEnv<'a>, name: &str) -> Result<&'a Vec<Value>, PargError> {
    let declared = env
        .tables
        .get(name)
        .ok_or_else(|| PargError::UnknownTable(name.to_string()))?;
    if !declared.is_object() {
        return Err(PargError::UnknownTable(name.to_string()));
    }
    declared
        .get("rows")
        .and_then(Value::as_array)
        .ok_or_else(|| PargError::UnknownTable(name.to_string()))
}

fn preprocess_with(
    env: &ApplyEnv,
    binding: &Map<String, Value>,
    value: Value,
) -> Result<Value, PargError> {
    let Some(name) = binding.get("preprocess").and_then(Value::as_str) else {
        return Ok(value);
    };
    let mut value = value;
    for step in preprocess_steps(env, name)? {
        let op = step
            .get("op")
            .and_then(Value::as_str)
            .ok_or_else(|| PargError::UnknownPreprocessOp("<missing>".to_string()))?;
        if op != "table_lookup" {
            return Err(PargError::UnknownPreprocessOp(op.to_string()));
        }
        let table = step
            .get("table")
            .and_then(Value::as_str)
            .ok_or_else(|| PargError::UnknownPreprocessOp(op.to_string()))?;
        let from = step
            .get("from")
            .and_then(Value::as_str)
            .ok_or_else(|| PargError::UnknownPreprocessOp(op.to_string()))?;
        let to = step
            .get("to")
            .and_then(Value::as_str)
            .ok_or_else(|| PargError::UnknownPreprocessOp(op.to_string()))?;
        let needle = to_ruby_string(&value);
        let mut mapped = None;
        for row in table_rows_of(env, table)? {
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

fn finalize_with(
    env: &ApplyEnv,
    binding: &Map<String, Value>,
    value: Value,
) -> Result<Value, PargError> {
    let kind = binding.get("type").and_then(Value::as_str);
    cast(preprocess_with(env, binding, value)?, kind)
}

fn assign_scalar_with(
    env: &ApplyEnv,
    out: &mut Map<String, Value>,
    binding: &Map<String, Value>,
    captures: &BTreeMap<String, Vec<Value>>,
) -> Result<(), PargError> {
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
    // Ruby `return if value.nil?` (Bindings.assign_scalar): a
    // capture that materialized as null (the absent-optional tree
    // wire) binds nothing — the key is skipped, never cast to "".
    if value.is_null() {
        return Ok(());
    }
    let finalized = finalize_with(env, binding, value)?;
    out.insert(leaf_key(&path), finalized);
    Ok(())
}

fn assign_array_with(
    env: &ApplyEnv,
    out: &mut Map<String, Value>,
    prefix: &str,
    group: &[&Map<String, Value>],
    captures: &BTreeMap<String, Vec<Value>>,
    shape: &Value,
) -> Result<(), PargError> {
    // Position-aware pairing (ruby#381): when the shape nests the
    // group's instances under the prefix key (or IS the instance
    // array — a repetition-root entry), each instance's subtree
    // owns its captures. Occurrence-index pairing landed a sparse
    // capture on the wrong instance; per-instance collection is
    // identical when every instance fires every capture. Captures
    // outside any instance keep the legacy occurrence pairing.
    if let Some(instances) = group_instances(shape, prefix) {
        // The occurrence path's `count == 0 → bind nothing`: a
        // repetition that fired zero iterations (the tree carries
        // `parts: []`) binds no key — the recorded native
        // expectations and the pubid builders rely on the absent
        // reading (rs#370, caught by the pubid corpus gate on the
        // 0.10.x bump).
        if instances.is_empty() {
            return Ok(());
        }
        let mut list: Vec<Value> = Vec::with_capacity(instances.len());
        for instance in instances {
            let found = collect_captures(instance);
            let mut element = Map::new();
            for binding in group {
                let capture = binding
                    .get("capture")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let Some(value) = found.get(capture).and_then(|c| c.first()) else {
                    continue;
                };
                if value.is_null() {
                    continue;
                }
                let finalized = finalize_with(env, binding, value.clone())?;
                let path = binding
                    .get("path")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                element.insert(leaf_key(path), finalized);
            }
            list.push(Value::Object(element));
        }
        out.insert(prefix.to_string(), Value::Array(list));
        return Ok(());
    }
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
            // Ruby `next if value.nil?` (Bindings.assign_array):
            // null slots bind nothing in their element.
            if value.is_null() {
                continue;
            }
            let finalized = finalize_with(env, binding, value.clone())?;
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

/// The apply core shared by the embedded-record path and the declared
/// bindings-artifact path: one materializer, two entry surfaces
/// (parsanol-ruby#432).
pub(crate) fn apply_entry_records(
    env: &ApplyEnv,
    bindings: &[Value],
    shape: &Value,
) -> Result<Value, PargError> {
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
        assign_scalar_with(env, &mut out, binding, &captures)?;
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
        assign_array_with(env, &mut out, prefix, &group, &captures, shape)?;
    }
    Ok(Value::Object(out))
}

impl PargArtifact {
    /// The binding list of an entry (empty when the entry declares none).
    pub fn bindings(&self, entry: &str) -> &[Value] {
        self.entry_value(entry, "bindings")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    fn apply_env(&self) -> ApplyEnv<'_> {
        ApplyEnv::of_artifact(&self.envelope)
    }

    /// Declared table rows (embedded in the envelope at compile time).
    pub fn table_rows(&self, name: &str) -> Result<&Vec<Value>, PargError> {
        table_rows_of(&self.apply_env(), name)
    }

    /// Apply an entry's bindings to a parsanol-shape parse tree.
    pub fn apply_bindings(&self, entry: &str, shape: &Value) -> Result<Value, PargError> {
        apply_entry_records(&self.apply_env(), self.bindings(entry), shape)
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
