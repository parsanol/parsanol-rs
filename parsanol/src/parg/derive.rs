//! F6 v1: derive specs — named field-composition templates evaluated
//! against a bound map. Byte-compatible with Ruby and TypeScript.

use serde_json::Value;

use super::{PargArtifact, PargError};

/// Evaluate a derive template: `{field}` interpolates the bound value;
/// missing fields render empty.
pub fn apply(derive: &Value, name: &str, bound: &Value) -> Result<String, PargError> {
    let template = derive
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| PargError::UnknownRenderVariant(format!("derive:{name}")))?;
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        let Some(end_rel) = rest[start..].find('}') else {
            break;
        };
        out.push_str(&rest[..start]);
        let field = &rest[start + 1..start + end_rel];
        let value = bound.get(field).filter(|v| !v.is_null());
        match value {
            Some(Value::String(s)) => out.push_str(s),
            Some(Value::Number(n)) => out.push_str(&n.to_string()),
            Some(Value::Bool(b)) => out.push_str(&b.to_string()),
            _ => {}
        }
        rest = &rest[start + end_rel + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

impl PargArtifact {
    /// Evaluate a named derive spec against a bound map.
    pub fn derive(&self, name: &str, bound: &Value) -> Result<String, PargError> {
        apply(
            self.envelope.get("derive").unwrap_or(&Value::Null),
            name,
            bound,
        )
    }

    /// Parse, bind, and evaluate a derive spec in one step.
    pub fn derive_string(&self, entry: &str, input: &str, name: &str) -> Result<String, PargError> {
        let bound = self.parse_and_bind(entry, input)?;
        self.derive(name, &bound)
    }
}
