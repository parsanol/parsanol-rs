//! F6 v1: generic renderer over the artifact's render spec — ordered
//! segments (field / literal / cond-presence) evaluated against a bound
//! attribute map. Byte-compatible with the Ruby and TypeScript
//! renderers: the spec lives under the artifact checksum.

use serde_json::Value;

use super::{PgArtifact, PgError};

fn lookup<'a>(bound: &'a Value, field: &str) -> Option<&'a Value> {
    bound.get(field).filter(|v| !v.is_null())
}

fn render_segments(segments: &[Value], bound: &Value, out: &mut String) -> Result<(), PgError> {
    for segment in segments {
        let object = segment
            .as_object()
            .ok_or_else(|| PgError::UnknownRenderSegment("<non-object>".to_string()))?;
        match object.get("type").and_then(Value::as_str) {
            Some("field") => {
                let field = object.get("field").and_then(Value::as_str).unwrap_or_default();
                let value = lookup(bound, field);
                out.push_str(&match value {
                    Some(Value::String(s)) => s.clone(),
                    Some(Value::Number(n)) => n.to_string(),
                    Some(Value::Bool(b)) => b.to_string(),
                    _ => String::new(),
                });
            }
            Some("literal") => {
                out.push_str(object.get("text").and_then(Value::as_str).unwrap_or_default());
            }
            Some("cond") => {
                let field = object.get("field").and_then(Value::as_str).unwrap_or_default();
                if lookup(bound, field).is_some() {
                    let then = object
                        .get("then")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                    render_segments(&then, bound, out)?;
                }
            }
            other => return Err(PgError::UnknownRenderSegment(other.unwrap_or("missing").to_string())),
        }
    }
    Ok(())
}

/// Render a named variant against a bound attribute map.
pub fn apply(render: &Value, variant: &str, bound: &Value) -> Result<String, PgError> {
    let segments = render
        .get(variant)
        .and_then(Value::as_array)
        .ok_or_else(|| PgError::UnknownRenderVariant(variant.to_string()))?;
    let mut out = String::new();
    render_segments(segments, bound, &mut out)?;
    Ok(out)
}

impl PgArtifact {
    /// The artifact's render spec (empty when none is declared).
    pub fn render_spec(&self) -> &Value {
        self.envelope.get("render").unwrap_or(&Value::Null)
    }

    /// Parse, bind, and render the identifier string in one step.
    pub fn render_string(&self, entry: &str, input: &str, variant: &str) -> Result<String, PgError> {
        let bound = self.parse_and_bind(entry, input)?;
        apply(self.render_spec(), variant, &bound)
    }
}
