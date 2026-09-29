//! PARG artifact envelopes: the Rust consumer side of the parsanol grammar
//! language.
//!
//! PARG sources (`.pg`) are compiled by the Ruby-side `Parsanol::PARG` compiler
//! — one compiler, N consumers — into a checksummed JSON envelope. This
//! module loads an envelope, verifies its canonical sha256 checksum
//! (byte-compatible with the Ruby compiler's `Compiler.checksum`), and
//! extracts the portable [`Grammar`] of any entry for parsing.
//!
//! ```no_run
//! use parsanol::PargArtifact;
//!
//! let artifact = PargArtifact::from_path("artifacts/iso.json").unwrap();
//! let grammar = artifact.grammar("identifier").unwrap();
//! // parse `input` with PortableParser::new(&grammar, input, &mut arena)
//! ```

use std::fmt;
use std::fmt::Write as _;
use std::path::Path;

use serde_json::Value;
use sha2::{Digest, Sha256};

pub mod bindings;
pub mod derive;
pub mod render;
pub mod schema;
pub mod suite;

use crate::portable::Grammar;

/// Errors raised while loading or extracting from a PARG artifact envelope.
///
/// New failure modes arrive as the artifact schema grows; match with a
/// catch-all arm.
#[derive(Debug)]
#[non_exhaustive]
pub enum PargError {
    /// The envelope is not valid JSON.
    Json(serde_json::Error),
    /// The envelope is JSON but not shaped like an artifact.
    InvalidEnvelope(&'static str),
    /// A declared entry does not exist in the envelope.
    UnknownEntry(String),
    /// The stored checksum does not match the recomputed one. Never fall
    /// back: a mismatched artifact is a corrupt or tampered artifact.
    ChecksumMismatch {
        /// The `checksum` field stored in the envelope.
        stored: String,
        /// The checksum recomputed over the canonical payload.
        computed: String,
    },
    /// A float was found where the canonical form requires an integer;
    /// Ruby and Rust disagree on float formatting, so floats are refused.
    FloatInCanonicalJson,
    /// Filesystem access failed.
    Io(std::io::Error),
    /// A binding references an undeclared preprocess step.
    UnknownPreprocess(String),
    /// A preprocess step declares an op the engine does not implement.
    UnknownPreprocessOp(String),
    /// A binding references a table the artifact does not embed.
    UnknownTable(String),
    /// A binding path nests (`a.b` / `items[]` outside the leaf slot),
    /// which the flat binding contract does not support.
    NestedBindingPath {
        /// The offending binding's capture name.
        capture: String,
        /// The offending binding's path.
        path: String,
    },
    /// A value could not be cast to the binding's declared type.
    Cast(String, String),
    /// The artifact declares a shape contract this engine does not support.
    UnsupportedShape(String),
    /// A render spec references a variant the artifact does not declare.
    UnknownRenderVariant(String),
    /// A render segment type the engine does not implement.
    UnknownRenderSegment(String),
    /// The portable engine failed to parse the input.
    ParseFailed(String),
    /// Structured failure wire (F7): deepest offset, expected set, message.
    ParseWire {
        /// Byte offset of the deepest failure.
        offset: usize,
        /// One-based line of the deepest failure.
        line: usize,
        /// One-based column of the deepest failure.
        column: usize,
        /// The labels expected at that position.
        expected: Vec<String>,
        /// C4: ranked (position, expected) pairs, deepest first.
        ranked: Vec<(usize, Vec<String>)>,
    },
    /// A reject test unexpectedly parsed.
    UnexpectedParse(String),
    /// An example test's expected captures did not match the bound result.
    CaptureMismatch {
        /// The example's input.
        input: String,
        /// The mismatch description.
        detail: String,
    },
}

impl fmt::Display for PargError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PargError::Json(err) => write!(f, "PARG artifact is not valid JSON: {err}"),
            PargError::InvalidEnvelope(what) => {
                write!(f, "PARG artifact envelope is invalid: missing {what}")
            }
            PargError::UnknownEntry(entry) => write!(f, "PARG artifact has no entry {entry:?}"),
            PargError::ChecksumMismatch { stored, computed } => write!(
                f,
                "PARG artifact checksum mismatch: stored {stored:?}, computed {computed:?}"
            ),
            PargError::UnknownPreprocess(name) => {
                write!(f, "preprocess step {name:?} not declared in artifact")
            }
            PargError::UnknownPreprocessOp(op) => write!(f, "unknown preprocess op {op:?}"),
            PargError::UnknownTable(name) => {
                write!(f, "artifact does not declare table {name:?}")
            }
            PargError::NestedBindingPath { capture, path } => write!(
                f,
                "binding {capture:?}: nested path {path:?} is not supported; bind the components instead"
            ),
            PargError::Cast(kind, value) => {
                write!(f, "cannot cast {value:?} to {kind}")
            }
            PargError::UnknownRenderVariant(v) => {
                write!(f, "render variant {v:?} not declared")
            }
            PargError::UnknownRenderSegment(t) => {
                write!(f, "unknown render segment {t:?}")
            }
            PargError::UnsupportedShape(shape) => {
                write!(f, "unsupported artifact shape {shape:?} (engine supports {SUPPORTED_SHAPE:?})")
            }
            PargError::ParseFailed(detail) => write!(f, "{detail}"),
            PargError::ParseWire { offset, line, column, expected, .. } => write!(
                f,
                "Parse failed at offset {offset} (line {line}, column {column}): expected {}",
                expected.join(", ")
            ),
            PargError::UnexpectedParse(_input) => {
                write!(f, "expected the input to be rejected")
            }
            PargError::CaptureMismatch { detail, .. } => write!(f, "{detail}"),
            PargError::FloatInCanonicalJson => {
                write!(
                    f,
                    "PARG artifact contains a float, which has no canonical form"
                )
            }
            PargError::Io(err) => write!(f, "PARG artifact could not be read: {err}"),
        }
    }
}

impl std::error::Error for PargError {}

/// The parsanol-shape contract this engine supports (F8). A mismatching
/// artifact is refused loudly at load, never parsed with wrong semantics.
pub const SUPPORTED_SHAPE: &str = "parsanol-tree/v2";

/// A verified PARG artifact envelope.
///
/// The checksum is verified at load time; a `PargArtifact` value never
/// exists for a mismatched envelope.
#[derive(Debug, Clone)]
pub struct PargArtifact {
    envelope: Value,
}

fn collect_terminals(atoms: &[Value], terms: &mut Vec<String>) {
    for atom in atoms {
        if let Some(obj) = atom.as_object() {
            if let Some(kind) = obj.keys().next() {
                match kind.as_str() {
                    "Str" => {
                        if let Some(p) = obj[kind].get("pattern").and_then(Value::as_str) {
                            terms.push(p.to_string());
                        }
                    }
                    "Re" => {
                        if let Some(p) = obj[kind].get("pattern").and_then(Value::as_str) {
                            terms.push(p.to_string());
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

impl PargArtifact {
    /// Parse and verify an envelope from its JSON text.
    pub fn from_json(text: &str) -> Result<Self, PargError> {
        let envelope: Value = serde_json::from_str(text).map_err(PargError::Json)?;
        let shape = envelope
            .get("shape")
            .and_then(Value::as_str)
            .ok_or(PargError::InvalidEnvelope("shape"))?;
        if shape != SUPPORTED_SHAPE {
            return Err(PargError::UnsupportedShape(shape.to_string()));
        }
        verify_checksum(&envelope)?;
        Ok(Self { envelope })
    }

    /// Parse and verify an envelope from a file.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, PargError> {
        let text = std::fs::read_to_string(path).map_err(PargError::Io)?;
        Self::from_json(&text)
    }

    /// The grammar version declared in the PARG source header.
    pub fn version(&self) -> Option<&str> {
        self.envelope.get("version").and_then(Value::as_str)
    }

    /// The grammar name declared in the PARG source header.
    pub fn grammar_name(&self) -> Option<&str> {
        self.envelope.get("grammar").and_then(Value::as_str)
    }

    /// The parsanol-shape contract the artifact was compiled against.
    pub fn shape(&self) -> Option<&str> {
        self.envelope.get("shape").and_then(Value::as_str)
    }

    /// The embedded PARG source text (self-contained artifacts).
    pub fn source(&self) -> Option<&str> {
        self.envelope.get("source").and_then(Value::as_str)
    }

    /// The verified checksum (`sha256:...`).
    pub fn checksum(&self) -> Option<&str> {
        self.envelope.get("checksum").and_then(Value::as_str)
    }

    /// C12: constrained-decoding vocabulary — every terminal literal and
    /// character-class pattern in every entry's grammar, deduplicated and
    /// sorted, for LLM constrained-decoding integration.
    pub fn terminal_vocabulary(&self) -> Vec<String> {
        let mut terms: Vec<String> = Vec::new();
        let entries: Vec<&Value> = self
            .envelope
            .get("entries")
            .and_then(Value::as_object)
            .map(|m| m.values().collect())
            .unwrap_or_default();
        for entry in entries {
            if let Some(atoms) = entry
                .get("grammar")
                .and_then(|g| g.get("atoms"))
                .and_then(Value::as_array)
            {
                collect_terminals(atoms, &mut terms);
            }
        }
        terms.sort();
        terms.dedup();
        terms
    }

    /// The artifact's embedded test list.
    pub fn tests(&self) -> &[Value] {
        self.envelope
            .get("tests")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Declared entry point names, sorted.
    pub fn entry_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self
            .envelope
            .get("entries")
            .and_then(Value::as_object)
            .map(|entries| entries.keys().map(String::as_str).collect())
            .unwrap_or_default();
        names.sort_unstable();
        names
    }

    /// The root rule name of an entry.
    pub fn entry_root(&self, entry: &str) -> Result<&str, PargError> {
        self.entry_value(entry, "root")
            .and_then(Value::as_str)
            .ok_or(PargError::UnknownEntry(entry.to_string()))
    }

    /// The portable [`Grammar`] of an entry, ready for the walker, VM, wasm
    /// or any other parsanol backend.
    pub fn grammar(&self, entry: &str) -> Result<Grammar, PargError> {
        let value = self
            .entry_value(entry, "grammar")
            .cloned()
            .ok_or(PargError::UnknownEntry(entry.to_string()))?;
        serde_json::from_value(value).map_err(PargError::Json)
    }

    fn entry_value(&self, entry: &str, field: &str) -> Option<&Value> {
        self.envelope
            .get("entries")
            .and_then(|entries| entries.get(entry))
            .and_then(|entry| entry.get(field))
    }
}

fn verify_checksum(envelope: &Value) -> Result<(), PargError> {
    let obj = envelope
        .as_object()
        .ok_or(PargError::InvalidEnvelope("top-level object"))?;
    let stored = obj
        .get("checksum")
        .and_then(Value::as_str)
        .ok_or(PargError::InvalidEnvelope("checksum"))?
        .to_string();
    let mut payload = obj.clone();
    payload.remove("checksum");
    let computed = checksum_hex(&Value::Object(payload))?;
    if stored != computed {
        return Err(PargError::ChecksumMismatch { stored, computed });
    }
    Ok(())
}

/// Canonical JSON, byte-compatible with the Ruby compiler's
/// `JSON.generate` of its key-sorted payload: object keys sorted, no
/// whitespace, short escapes for `\b \t \n \f \r " \`, lowercase `\u00xx`
/// for other control characters, raw UTF-8 otherwise.
pub fn canonical_json(value: &Value) -> Result<String, PargError> {
    let mut out = String::new();
    write_canonical(value, &mut out)?;
    Ok(out)
}

fn write_canonical(value: &Value, out: &mut String) -> Result<(), PargError> {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(number) => {
            if number.is_f64() {
                return Err(PargError::FloatInCanonicalJson);
            }
            out.push_str(&number.to_string());
        }
        Value::String(text) => write_canonical_string(text, out),
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_canonical(item, out)?;
            }
            out.push(']');
        }
        Value::Object(map) => {
            // serde_json's default map is a BTreeMap (sorted); sort
            // explicitly so the canonical form does not depend on the
            // `preserve_order` feature being off.
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (index, key) in keys.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_canonical_string(key, out);
                out.push(':');
                write_canonical(&map[*key], out)?;
            }
            out.push('}');
        }
    }
    Ok(())
}

fn write_canonical_string(text: &str, out: &mut String) {
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other if (other as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", other as u32));
            }
            other => out.push(other),
        }
    }
    out.push('"');
}

/// The `sha256:...` checksum of a payload's canonical JSON.
pub fn checksum_hex(payload: &Value) -> Result<String, PargError> {
    let canonical = canonical_json(payload)?;
    let digest = Sha256::digest(canonical.as_bytes());
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(hex, "{byte:02x}").expect("writing to a String cannot fail");
    }
    Ok(format!("sha256:{hex}"))
}
