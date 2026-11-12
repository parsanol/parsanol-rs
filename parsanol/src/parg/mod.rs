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
pub mod bindings_artifact;
pub mod derive;
pub mod export;
pub mod frontend;
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
    /// The payload is not a `parg-bindings` artifact (parsanol-ruby#432).
    InvalidBindingsArtifact(&'static str),
    /// The declared bindings artifact's checksum does not verify (#432).
    BindingsChecksumMismatch {
        /// The `checksum` field stored in the bindings artifact.
        stored: String,
        /// The checksum recomputed over the canonical payload.
        computed: String,
    },
    /// The bindings artifact declares a target the caller did not ask
    /// for (#432).
    BindingsTargetMismatch {
        /// The artifact's declared `target`.
        declared: String,
        /// The target the caller pinned.
        expected: String,
    },
    /// A bindings artifact declares a target name outside the set (#432).
    UnknownBindingsTarget(String),
    /// The bindings artifact's `binding_version` is not supported (#432).
    UnsupportedBindingVersion(i64),
    /// The bindings artifact does not pair with the parse artifact —
    /// grammar, version or source hash differ (#432).
    BindingsPairingMismatch(String),
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
    /// Never constructed since floats were allowed to canonicalize (B6);
    /// kept so matchers written against 0.18.11 keep compiling.
    FloatInCanonicalJson,
    /// A rule-recursion guard aborted parsing, not a syntax refusal.
    RecursionLimitExceeded {
        /// Recursion depth at the guard.
        depth: usize,
        /// Configured rule-recursion budget.
        max_depth: usize,
    },
    /// VM atom-call depth reached the platform stack-resource guard.
    CallDepthLimitExceeded {
        /// VM atom-call depth at the guard.
        depth: usize,
        /// Configured VM atom-call budget.
        max_depth: usize,
    },
}

impl PargError {
    pub(crate) fn runtime_abort(
        error: crate::portable::ast::ParseError,
        call_depth_limit_hit: bool,
    ) -> Self {
        match error {
            crate::portable::ast::ParseError::RecursionLimitExceeded { depth, max_depth } => {
                if call_depth_limit_hit {
                    Self::CallDepthLimitExceeded { depth, max_depth }
                } else {
                    Self::RecursionLimitExceeded { depth, max_depth }
                }
            }
            other => Self::ParseFailed(other.to_string()),
        }
    }
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
            PargError::InvalidBindingsArtifact(what) => {
                write!(f, "not a parg-bindings artifact: {what}")
            }
            PargError::BindingsChecksumMismatch { stored, computed } => write!(
                f,
                "bindings artifact checksum mismatch: stored {stored:?}, computed {computed:?}"
            ),
            PargError::BindingsTargetMismatch { declared, expected } => write!(
                f,
                "bindings artifact targets {declared:?}, not {expected:?}"
            ),
            PargError::UnknownBindingsTarget(target) => {
                write!(f, "unknown binding target {target:?}")
            }
            PargError::UnsupportedBindingVersion(version) => {
                write!(f, "unsupported binding_version {version}")
            }
            PargError::BindingsPairingMismatch(detail) => write!(
                f,
                "bindings artifact does not pair with the parse artifact: {detail}"
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
            PargError::RecursionLimitExceeded { depth, max_depth } => write!(f,
                "PARG parse failed: {}", crate::portable::ast::ParseError::RecursionLimitExceeded { depth: *depth, max_depth: *max_depth }),
            PargError::CallDepthLimitExceeded { depth, max_depth } => write!(f,
                "PARG parse failed: VM call depth exceeded: depth {depth} exceeds limit of {max_depth}"),
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
                write!(f, "floats are canonicalized, not refused")
            }
            PargError::Io(err) => write!(f, "PARG artifact could not be read: {err}"),
        }
    }
}

impl std::error::Error for PargError {}

/// The parsanol-shape contract this engine supports (F8). A mismatching
/// artifact is refused loudly at load, never parsed with wrong semantics.
pub const SUPPORTED_SHAPE: &str = "parsanol-tree/v2";

/// The highest binding_version this engine executes (grow-with-minor,
/// rs#147): 1 = the v1 preprocess vocabulary (`table_lookup`); 2 adds
/// `const_map`, `default_when_absent`, and `rename`. Artifacts
/// declaring a higher level refuse at load.
pub const SUPPORTED_BINDING_VERSION: i64 = 2;

/// A verified PARG artifact envelope.
///
/// The checksum is verified at load time; a `PargArtifact` value never
/// exists for a mismatched envelope.
#[derive(Debug, Clone)]
pub struct PargArtifact {
    envelope: Value,
}

/// The edition spellings this runtime loads: edition 2 (named trees +
/// explicit spans) is the only edition — edition 1 was removed, so its
/// spellings normalize to `None` and the artifact is rejected at load
/// with the recompile instruction. Absent field = edition 2.
/// Public so the parity gates normalize both sides the same way.
pub fn normalize_edition(value: Option<&Value>) -> Option<u8> {
    match value {
        None | Some(Value::Null) => Some(2),
        Some(Value::Number(n)) => match n.as_u64() {
            Some(2) => Some(2),
            _ => None,
        },
        Some(Value::String(s)) => match s.as_str() {
            "2" | "v2" => Some(2),
            _ => None,
        },
        Some(_) => None,
    }
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
    /// Parse and verify an envelope from its JSON text. When the bake
    /// sealed the file (parsanol-ruby#443), the byte-level checksum is
    /// verified first: a zero-hole sha256 over the exact input text, one
    /// O(bytes) pass before any structural work.
    pub fn from_json(text: &str) -> Result<Self, PargError> {
        let envelope: Value = serde_json::from_str(text).map_err(PargError::Json)?;
        verify_byte_checksum(&envelope, text)?;
        let shape = envelope
            .get("shape")
            .and_then(Value::as_str)
            .ok_or(PargError::InvalidEnvelope("shape"))?;
        if shape != SUPPORTED_SHAPE {
            return Err(PargError::UnsupportedShape(shape.to_string()));
        }
        // Grow-with-minor (rs#147): the envelope's binding_version is
        // the contract level the artifact relies on. 1 = the v1
        // preprocess vocabulary (table_lookup); 2 = the v1.1 ops
        // (const_map, default_when_absent, rename). Absent = 1 (the
        // minimal pre-v1.1 envelopes some fixtures hand-build).
        let binding_version = envelope
            .get("binding_version")
            .and_then(Value::as_i64)
            .unwrap_or(1);
        if !(1..=SUPPORTED_BINDING_VERSION).contains(&binding_version) {
            return Err(PargError::UnsupportedBindingVersion(binding_version));
        }
        // Edition 2 is the only edition (edition 1 was removed): a
        // removed or unknown spelling rejects the artifact at load
        // with the recompile instruction. The quoted v-form stays
        // loadable for artifacts baked before the numeric surface.
        normalize_edition(envelope.get("edition")).ok_or(PargError::InvalidEnvelope(
            "artifact edition is not supported -- edition 1 was removed; \
             recompile the grammar with edition \"v2\" (named trees + spans)",
        ))?;
        verify_checksum(&envelope)?;
        Ok(Self { envelope })
    }

    /// The language edition the artifact's trees carry (ruby#357):
    /// 2 = fully named trees, the only edition.
    pub fn edition(&self) -> u8 {
        normalize_edition(self.envelope.get("edition")).unwrap_or(2)
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

    /// The verified envelope (read-only): the pure functions over the
    /// wire — export, schema, derive — take it from here.
    pub fn envelope_value(&self) -> &Value {
        &self.envelope
    }

    /// The embedded PARG source text (self-contained artifacts).
    pub fn source(&self) -> Option<&str> {
        self.envelope.get("source").and_then(Value::as_str)
    }

    /// The SHA-256 of the .parg text at compile time (parsanol-ruby#209
    /// freshness; pins the declared bindings artifact pairing, #432).
    pub fn source_hash(&self) -> Option<&str> {
        self.envelope.get("source_hash").and_then(Value::as_str)
    }

    /// The verified checksum (`sha256:...`).
    pub fn checksum(&self) -> Option<&str> {
        self.envelope.get("checksum").and_then(Value::as_str)
    }

    /// The semantics hash (ruby#377): sha256 over the canonicalized
    /// envelope minus the producer set — stable across re-bakes by a
    /// newer compiler when the grammar's behavior is unchanged. None
    /// on artifacts baked before the field existed; never an error.
    pub fn semantics_hash(&self) -> Option<&str> {
        self.envelope.get("semantics_hash").and_then(Value::as_str)
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

    /// Load executable grammar data, retaining the explicit StateMatchAscii wire
    /// operation as per-atom matching policy. The ordinary grammar() API keeps
    /// rejecting that unknown operation instead of silently dropping its mode.
    /// Atom and Grammar retain their existing public construction/match shapes.
    pub(crate) fn grammar_with_ascii_state_matches(
        &self,
        entry: &str,
    ) -> Result<(Grammar, Vec<usize>), PargError> {
        let value = self
            .entry_value(entry, "grammar")
            .cloned()
            .ok_or_else(|| PargError::UnknownEntry(entry.to_owned()))?;
        decode_ascii_state_grammar(value)
    }

    fn entry_value(&self, entry: &str, field: &str) -> Option<&Value> {
        self.envelope
            .get("entries")
            .and_then(|entries| entries.get(entry))
            .and_then(|entry| entry.get(field))
    }
}

// Shared by executable entries and explicitly declared recovery grammars.
fn decode_ascii_state_grammar(mut value: Value) -> Result<(Grammar, Vec<usize>), PargError> {
    if !value.get("atoms").is_some_and(Value::is_array) {
        return serde_json::from_value(value)
            .map(|grammar| (grammar, Vec::new()))
            .map_err(PargError::Json);
    }

    let atoms = value
        .get_mut("atoms")
        .and_then(Value::as_array_mut)
        .ok_or(PargError::InvalidEnvelope("grammar atoms"))?;
    let mut indexes = Vec::new();
    for (index, atom) in atoms.iter_mut().enumerate() {
        let Some(fields) = atom.as_object_mut() else {
            continue;
        };
        if fields.contains_key("StateMatchAscii") {
            if fields.len() != 1 {
                return Err(PargError::InvalidEnvelope("StateMatchAscii atom fields"));
            }
            let payload = fields.remove("StateMatchAscii").unwrap();
            let valid = payload
                .as_object()
                .is_some_and(|p| p.len() == 1 && p.get("slot").is_some_and(Value::is_string));
            if !valid {
                return Err(PargError::InvalidEnvelope("StateMatchAscii slot"));
            }
            fields.insert("StateMatch".into(), payload);
            indexes.push(index);
        }
    }
    let grammar = serde_json::from_value(value).map_err(PargError::Json)?;
    Ok((grammar, indexes))
}

/// The byte-level checksum's fast tier: the bake hashed the file bytes
/// with the digest's own 64-hex region zeroed (parsanol-ruby#443); the
/// loader repeats the same substitution over the exact text it received.
fn verify_byte_checksum(envelope: &Value, text: &str) -> Result<(), PargError> {
    let Some(stored) = envelope.get("byte_checksum").and_then(Value::as_str) else {
        return Ok(()); // pre-seal artifact: the canonical tier covers it
    };
    let placeholder = "0".repeat(64);
    let zeroed = text.replace(stored, &placeholder);
    // Raw bytes, exactly like the ruby bake: Digest over the text, NOT
    // the canonical JSON form (this tier is byte-exact by design).
    let digest = Sha256::digest(zeroed.as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    let computed = format!("sha256:{hex}");
    if stored == computed {
        return Ok(());
    }
    Err(PargError::ChecksumMismatch {
        stored: stored.to_string(),
        computed,
    })
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
    // The byte tier's own digest region is producer-serialization
    // specific (parsanol-ruby#443) and must not enter the canonical
    // payload — rs#397: including it rejected every ruby-1.3.131+ bake.
    payload.remove("byte_checksum");
    let computed = checksum_hex(&Value::Object(payload))?;
    if stored != computed {
        return Err(PargError::ChecksumMismatch { stored, computed });
    }
    Ok(())
}

/// Render an `f64` the way Ruby's JSON gem does: the shortest round-trip
/// digits, decimal notation for decimal exponents -9..=14, scientific
/// notation (`d.ddde+NN`, sign always, exponent unpadded) outside that
/// range, and no trailing-`.0` stripping (2.0 stays `2.0`, 1e15 is
/// `1e+15`). This is the B6 canonical float form; it makes checksums and
/// expectations byte-identical with the reference compiler.
pub(crate) fn ruby_json_float(value: f64) -> String {
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0.0"
        } else {
            "0.0"
        }
        .to_string();
    }

    let sign = if value < 0.0 { "-" } else { "" };
    // Shortest round-trip digits: raise precision until parsing recovers
    // the exact value (16 significant digits always suffice for f64).
    let (digits, exponent) = (0..=17)
        .map(|precision| format!("{value:.precision$e}"))
        .find(|text| text.parse::<f64>() == Ok(value))
        .map(|text| {
            let (mantissa, exp) = text.split_once('e').expect("scientific form");
            let digits: String = mantissa.chars().filter(|c| c.is_ascii_digit()).collect();
            let digits = digits.trim_end_matches('0');
            let digits = if digits.is_empty() { "0" } else { digits };
            (digits.to_string(), exp.parse::<i32>().expect("exponent"))
        })
        .expect("at least 17 digits always round-trips");

    if !(-9..=14).contains(&exponent) {
        let head = digits[..1].to_string();
        let tail = &digits[1..];
        let mantissa = if tail.is_empty() {
            head
        } else {
            format!("{head}.{tail}")
        };
        return format!("{sign}{mantissa}e{exponent:+03}");
    }

    if exponent >= 0 {
        let point = exponent as usize + 1;
        if point >= digits.len() {
            let zeros = "0".repeat(point - digits.len());
            format!("{sign}{digits}{zeros}.0")
        } else {
            format!("{sign}{}.{}", &digits[..point], &digits[point..])
        }
    } else {
        let zeros = "0".repeat((-exponent) as usize - 1);
        format!("{sign}0.{zeros}{digits}")
    }
}

/// Canonical JSON, byte-compatible with the Ruby compiler's
/// `JSON.generate` of its key-sorted payload: object keys sorted, no
/// whitespace, short escapes for `\b \t \n \f \r " \`, lowercase `\u00xx`
/// for other control characters, raw UTF-8 otherwise, and floats rendered
/// with the B6 float form (see `ruby_json_float`).
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
                // B6 float canonical form: the shortest round-trip digits
                // rendered with the Ruby JSON gem's rules, so checksums and
                // expectations match the reference compiler byte for byte.
                out.push_str(&ruby_json_float(number.as_f64().unwrap()));
            } else {
                out.push_str(&number.to_string());
            }
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

#[cfg(test)]
mod float_tests {
    use super::ruby_json_float;

    #[test]
    fn floats_render_like_the_ruby_json_gem() {
        // Ground truth: JSON.generate(v) on ruby 3.4 (JSON gem 2.x).
        let cases: &[(f64, &str)] = &[
            (0.1, "0.1"),
            (1.5, "1.5"),
            (2.0, "2.0"),
            (100.0, "100.0"),
            (-2.5, "-2.5"),
            (-0.0, "-0.0"),
            (0.0, "0.0"),
            (0.0001, "0.0001"),
            (1e-5, "0.00001"),
            (1e-8, "0.00000001"),
            (1.5e-10, "1.5e-10"),
            (5e-324, "5e-324"),
            (12345.6789, "12345.6789"),
            (123456789012345.0, "123456789012345.0"),
            (999999999999999.9, "999999999999999.9"),
            (1e15, "1e+15"),
            (1e16, "1e+16"),
            (1e17, "1e+17"),
            (1e300, "1e+300"),
            (1.7976931348623157e308, "1.7976931348623157e+308"),
        ];
        for (value, text) in cases {
            assert_eq!(&ruby_json_float(*value), *text, "value {value:?}");
        }
    }
}
