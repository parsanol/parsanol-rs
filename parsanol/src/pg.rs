//! PG artifact envelopes: the Rust consumer side of the parsanol grammar
//! language.
//!
//! PG sources (`.pg`) are compiled by the Ruby-side `Parsanol::PG` compiler
//! — one compiler, N consumers — into a checksummed JSON envelope. This
//! module loads an envelope, verifies its canonical sha256 checksum
//! (byte-compatible with the Ruby compiler's `Compiler.checksum`), and
//! extracts the portable [`Grammar`] of any entry for parsing.
//!
//! ```no_run
//! use parsanol::PgArtifact;
//!
//! let artifact = PgArtifact::from_path("artifacts/iso.json").unwrap();
//! let grammar = artifact.grammar("identifier").unwrap();
//! // parse `input` with PortableParser::new(&grammar, input, &mut arena)
//! ```

use std::fmt;
use std::fmt::Write as _;
use std::path::Path;

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::portable::Grammar;

/// Errors raised while loading or extracting from a PG artifact envelope.
#[derive(Debug)]
pub enum PgError {
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
}

impl fmt::Display for PgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PgError::Json(err) => write!(f, "PG artifact is not valid JSON: {err}"),
            PgError::InvalidEnvelope(what) => {
                write!(f, "PG artifact envelope is invalid: missing {what}")
            }
            PgError::UnknownEntry(entry) => write!(f, "PG artifact has no entry {entry:?}"),
            PgError::ChecksumMismatch { stored, computed } => write!(
                f,
                "PG artifact checksum mismatch: stored {stored:?}, computed {computed:?}"
            ),
            PgError::FloatInCanonicalJson => {
                write!(
                    f,
                    "PG artifact contains a float, which has no canonical form"
                )
            }
            PgError::Io(err) => write!(f, "PG artifact could not be read: {err}"),
        }
    }
}

impl std::error::Error for PgError {}

/// A verified PG artifact envelope.
///
/// The checksum is verified at load time; a `PgArtifact` value never
/// exists for a mismatched envelope.
#[derive(Debug, Clone)]
pub struct PgArtifact {
    envelope: Value,
}

impl PgArtifact {
    /// Parse and verify an envelope from its JSON text.
    pub fn from_json(text: &str) -> Result<Self, PgError> {
        let envelope: Value = serde_json::from_str(text).map_err(PgError::Json)?;
        verify_checksum(&envelope)?;
        Ok(Self { envelope })
    }

    /// Parse and verify an envelope from a file.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, PgError> {
        let text = std::fs::read_to_string(path).map_err(PgError::Io)?;
        Self::from_json(&text)
    }

    /// The grammar version declared in the PG source header.
    pub fn version(&self) -> Option<&str> {
        self.envelope.get("version").and_then(Value::as_str)
    }

    /// The grammar name declared in the PG source header.
    pub fn grammar_name(&self) -> Option<&str> {
        self.envelope.get("grammar").and_then(Value::as_str)
    }

    /// The parsanol-shape contract the artifact was compiled against.
    pub fn shape(&self) -> Option<&str> {
        self.envelope.get("shape").and_then(Value::as_str)
    }

    /// The embedded PG source text (self-contained artifacts).
    pub fn source(&self) -> Option<&str> {
        self.envelope.get("source").and_then(Value::as_str)
    }

    /// The verified checksum (`sha256:...`).
    pub fn checksum(&self) -> Option<&str> {
        self.envelope.get("checksum").and_then(Value::as_str)
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
    pub fn entry_root(&self, entry: &str) -> Result<&str, PgError> {
        self.entry_value(entry, "root")
            .and_then(Value::as_str)
            .ok_or(PgError::UnknownEntry(entry.to_string()))
    }

    /// The portable [`Grammar`] of an entry, ready for the walker, VM, wasm
    /// or any other parsanol backend.
    pub fn grammar(&self, entry: &str) -> Result<Grammar, PgError> {
        let value = self
            .entry_value(entry, "grammar")
            .cloned()
            .ok_or(PgError::UnknownEntry(entry.to_string()))?;
        serde_json::from_value(value).map_err(PgError::Json)
    }

    fn entry_value(&self, entry: &str, field: &str) -> Option<&Value> {
        self.envelope
            .get("entries")
            .and_then(|entries| entries.get(entry))
            .and_then(|entry| entry.get(field))
    }
}

fn verify_checksum(envelope: &Value) -> Result<(), PgError> {
    let obj = envelope
        .as_object()
        .ok_or(PgError::InvalidEnvelope("top-level object"))?;
    let stored = obj
        .get("checksum")
        .and_then(Value::as_str)
        .ok_or(PgError::InvalidEnvelope("checksum"))?
        .to_string();
    let mut payload = obj.clone();
    payload.remove("checksum");
    let computed = checksum_hex(&Value::Object(payload))?;
    if stored != computed {
        return Err(PgError::ChecksumMismatch { stored, computed });
    }
    Ok(())
}

/// Canonical JSON, byte-compatible with the Ruby compiler's
/// `JSON.generate` of its key-sorted payload: object keys sorted, no
/// whitespace, short escapes for `\b \t \n \f \r " \`, lowercase `\u00xx`
/// for other control characters, raw UTF-8 otherwise.
pub fn canonical_json(value: &Value) -> Result<String, PgError> {
    let mut out = String::new();
    write_canonical(value, &mut out)?;
    Ok(out)
}

fn write_canonical(value: &Value, out: &mut String) -> Result<(), PgError> {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(number) => {
            if number.is_f64() {
                return Err(PgError::FloatInCanonicalJson);
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
pub fn checksum_hex(payload: &Value) -> Result<String, PgError> {
    let canonical = canonical_json(payload)?;
    let digest = Sha256::digest(canonical.as_bytes());
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(hex, "{byte:02x}").expect("writing to a String cannot fail");
    }
    Ok(format!("sha256:{hex}"))
}
