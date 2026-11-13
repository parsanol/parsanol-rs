//! rs#459 slice 2: the segment→generator lowering. The artifact's
//! render spec and derive templates are uniform wire data; the
//! graduated path lowers them ONCE into typed [`GenSegment`] trees and
//! executes those against a bound map — an engine-owned generator
//! ("parse in reverse"), cached per variant like the decoded grammars.
//!
//! Byte-identity is the gate: `Generator::emit` must reproduce the
//! generic interpreter's output byte-for-byte (tests/generate_parity
//! replays every render-spec artifact, and the corpus gate now runs
//! the frozen-render replay through BOTH paths).

use serde_json::Value;
use std::sync::Arc;

use super::{PargArtifact, PargError};

/// A lowered output atom: emit a literal, emit a bound field's value,
/// or emit-or-skip a subtree on the field's presence.
#[derive(Debug, Clone, PartialEq)]
pub enum GenSegment {
    /// Emit a literal string.
    Literal(String),
    /// Emit the bound value of this field (String/Number/Bool; absent
    /// or null fields emit nothing).
    Field(String),
    /// Emit `then` only when the field is present in the bound map.
    Cond {
        /// The presence-tested field.
        field: String,
        /// The subtree emitted on presence.
        then: Arc<Vec<GenSegment>>,
    },
}

/// A lowered template: literal runs interleaved with field holes (the
/// `derive` `{field}` spelling).
#[derive(Debug, Clone, PartialEq)]
pub struct GenTemplate {
    /// Literal text before each hole; `parts.len() == holes.len() + 1`.
    parts: Vec<String>,
    holes: Vec<String>,
}

/// A compiled generator for one named variant.
#[derive(Debug, Clone, PartialEq)]
pub struct Generator {
    variant: String,
    segments: Arc<Vec<GenSegment>>,
}

fn lower_field(object: &serde_json::Map<String, Value>) -> Result<GenSegment, PargError> {
    let field = object
        .get("field")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Ok(GenSegment::Field(field.to_string()))
}

fn lower_segments(segments: &[Value]) -> Result<Vec<GenSegment>, PargError> {
    let mut out = Vec::with_capacity(segments.len());
    for segment in segments {
        let object = segment
            .as_object()
            .ok_or_else(|| PargError::UnknownRenderSegment("<non-object>".to_string()))?;
        match object.get("type").and_then(Value::as_str) {
            Some("field") => out.push(lower_field(object)?),
            Some("literal") => out.push(GenSegment::Literal(
                object
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            )),
            Some("cond") => {
                let field = object
                    .get("field")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let then = object
                    .get("then")
                    .and_then(Value::as_array)
                    .map(|a| lower_segments(a))
                    .transpose()?
                    .unwrap_or_default();
                out.push(GenSegment::Cond {
                    field: field.to_string(),
                    then: Arc::new(then),
                });
            }
            other => {
                return Err(PargError::UnknownRenderSegment(
                    other.unwrap_or("missing").to_string(),
                ))
            }
        }
    }
    Ok(out)
}

fn bound_text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

fn lookup<'a>(bound: &'a Value, field: &str) -> Option<&'a Value> {
    bound.get(field).filter(|v| !v.is_null())
}

impl Generator {
    /// Lower one named variant of a render spec.
    pub fn lower(render: &Value, variant: &str) -> Result<Self, PargError> {
        let segments = render
            .get(variant)
            .and_then(Value::as_array)
            .ok_or_else(|| PargError::UnknownRenderVariant(variant.to_string()))?;
        Ok(Self {
            variant: variant.to_string(),
            segments: Arc::new(lower_segments(segments)?),
        })
    }

    /// Emit the variant's string against a bound map. Byte-identical
    /// with the generic interpreter (`render::apply`) — the same walk
    /// shape, over lowered segments instead of raw wire objects.
    pub fn emit(&self, bound: &Value) -> Result<String, PargError> {
        let mut out = String::new();
        emit_segments(&self.segments, bound, &mut out);
        Ok(out)
    }
}

fn emit_segments(segments: &[GenSegment], bound: &Value, out: &mut String) {
    for segment in segments {
        match segment {
            GenSegment::Literal(text) => out.push_str(text),
            GenSegment::Field(field) => out.push_str(&bound_text(lookup(bound, field))),
            GenSegment::Cond { field, then } => {
                if lookup(bound, field).is_some() {
                    emit_segments(then, bound, out);
                }
            }
        }
    }
}

impl GenTemplate {
    /// Lower a derive template: `{field}` interpolates the bound value;
    /// missing fields render empty.
    pub fn lower(template: &str) -> Self {
        let mut parts = Vec::new();
        let mut holes = Vec::new();
        let mut rest = template;
        let mut literal = String::new();
        while let Some(start) = rest.find('{') {
            let Some(end_rel) = rest[start..].find('}') else {
                break;
            };
            literal.push_str(&rest[..start]);
            parts.push(std::mem::take(&mut literal));
            holes.push(rest[start + 1..start + end_rel].to_string());
            rest = &rest[start + end_rel + 1..];
        }
        literal.push_str(rest);
        parts.push(literal);
        Self { parts, holes }
    }

    /// Emit the template against a bound map. Byte-identical with the
    /// generic interpreter (`derive::apply`).
    pub fn emit(&self, bound: &Value) -> String {
        let mut out = String::new();
        for (i, part) in self.parts.iter().enumerate() {
            out.push_str(part);
            if let Some(hole) = self.holes.get(i) {
                out.push_str(&bound_text(lookup(bound, hole)));
            }
        }
        out
    }
}

impl PargArtifact {
    /// The lowered generator for a render variant, cached per variant.
    pub fn generator(&self, variant: &str) -> Result<Arc<Generator>, PargError> {
        if let Some(hit) = self.generators.lock().unwrap().get(variant) {
            return Ok(hit.clone());
        }
        let lowered = Arc::new(Generator::lower(self.render_spec(), variant)?);
        self.generators
            .lock()
            .unwrap()
            .insert(variant.to_string(), lowered.clone());
        Ok(lowered)
    }

    /// Emit a render variant against a bound map through the lowered
    /// generator (the graduated path).
    pub fn generate(&self, variant: &str, bound: &Value) -> Result<String, PargError> {
        self.generator(variant)?.emit(bound)
    }

    /// The lowered template for a derive spec name, cached per name.
    pub fn gen_template(&self, name: &str) -> Result<Arc<GenTemplate>, PargError> {
        if let Some(hit) = self.gen_templates.lock().unwrap().get(name) {
            return Ok(hit.clone());
        }
        let template = self
            .envelope
            .get("derive")
            .and_then(|d| d.get(name))
            .and_then(Value::as_str)
            .ok_or_else(|| PargError::UnknownRenderVariant(format!("derive:{name}")))?;
        let lowered = Arc::new(GenTemplate::lower(template));
        self.gen_templates
            .lock()
            .unwrap()
            .insert(name.to_string(), lowered.clone());
        Ok(lowered)
    }

    /// Evaluate a derive template against a bound map through the
    /// lowered template (the graduated path).
    pub fn gen_derive(&self, name: &str, bound: &Value) -> Result<String, PargError> {
        Ok(self.gen_template(name)?.emit(bound))
    }
}
