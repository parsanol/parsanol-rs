//! The declared per-target binding artifact (parsanol-ruby#432): from
//! one compiled grammar the toolchain emits one checksummed bindings
//! artifact per target, each carrying the entry binding records and
//! pinned to its grammar by source hash. Declared and diffable, not
//! baked.
//!
//! Boundary: the records carry AST construction only — the
//! production-to-binding mapping. Semantics live in the model layer
//! (LML). Execution resources (preprocess steps and their tables)
//! resolve from the PAIRED parse artifact's checksummed envelope,
//! which is why [`PargBindingsArtifact::apply`] takes the paired
//! [`PargArtifact`] and re-verifies the pairing.

use serde_json::{Map, Value};

use super::bindings::{apply_entry_records, ApplyEnv};
use crate::parg::{checksum_hex, PargArtifact, PargError};

/// The `artifact_kind` of a declared bindings artifact.
pub const BINDINGS_ARTIFACT_KIND: &str = "parg-bindings";

/// The record-schema version the engines understand.
pub const SUPPORTED_BINDING_VERSION: i64 = 1;

/// The target set the compiler emits; loaders pin their own.
pub const BINDING_TARGETS: [&str; 4] = ["ruby", "rs", "ts", "python"];

/// Emit the per-target bindings artifact from a compiled parse-artifact
/// envelope. Checksummed with the artifact scheme (canonical JSON
/// sha256 over everything except the checksum itself), so every engine
/// re-derives it identically.
pub fn bindings_envelope(envelope: &Value, target: &str) -> Result<Value, PargError> {
    if !BINDING_TARGETS.contains(&target) {
        return Err(PargError::UnknownBindingsTarget(target.to_string()));
    }
    let mut entries = Map::new();
    if let Some(map) = envelope.get("entries").and_then(Value::as_object) {
        for (name, entry) in map {
            let records = entry
                .get("bindings")
                .cloned()
                .unwrap_or_else(|| Value::Array(Vec::new()));
            let mut slot = Map::new();
            slot.insert("bindings".to_string(), records);
            entries.insert(name.clone(), Value::Object(slot));
        }
    }
    let mut payload = Map::new();
    payload.insert(
        "artifact_kind".to_string(),
        Value::String(BINDINGS_ARTIFACT_KIND.to_string()),
    );
    payload.insert("target".to_string(), Value::String(target.to_string()));
    payload.insert(
        "binding_version".to_string(),
        Value::Number(serde_json::Number::from(SUPPORTED_BINDING_VERSION)),
    );
    payload.insert(
        "grammar".to_string(),
        envelope.get("grammar").cloned().unwrap_or(Value::Null),
    );
    payload.insert(
        "grammar_version".to_string(),
        envelope.get("version").cloned().unwrap_or(Value::Null),
    );
    payload.insert(
        "grammar_source_hash".to_string(),
        envelope.get("source_hash").cloned().unwrap_or(Value::Null),
    );
    payload.insert(
        "shape".to_string(),
        envelope.get("shape").cloned().unwrap_or(Value::Null),
    );
    payload.insert("entries".to_string(), Value::Object(entries));
    let checksum = checksum_hex(&Value::Object(payload.clone()))?;
    payload.insert("checksum".to_string(), Value::String(checksum));
    Ok(Value::Object(payload))
}

/// A verified declared bindings artifact. Never exists for a tampered
/// payload: construction verifies kind, checksum, target and binding
/// version.
#[derive(Debug, Clone)]
pub struct PargBindingsArtifact {
    payload: Value,
}

impl PargBindingsArtifact {
    /// Parse and verify a bindings artifact from its JSON text.
    /// `expected_target` pins the target; `None` accepts any declared
    /// target (multi-target tooling).
    pub fn from_json(text: &str, expected_target: Option<&str>) -> Result<Self, PargError> {
        let payload: Value = serde_json::from_str(text).map_err(PargError::Json)?;
        Self::from_value(payload, expected_target)
    }

    /// Verify and adopt an already-parsed payload.
    pub fn from_value(payload: Value, expected_target: Option<&str>) -> Result<Self, PargError> {
        let artifact = Self { payload };
        artifact.verify()?;
        if let Some(expected) = expected_target {
            artifact.verify_target(expected)?;
        }
        Ok(artifact)
    }

    fn verify(&self) -> Result<(), PargError> {
        let object = self
            .payload
            .as_object()
            .ok_or_else(|| PargError::InvalidBindingsArtifact("payload must be an object"))?;
        if object.get("artifact_kind").and_then(Value::as_str) != Some(BINDINGS_ARTIFACT_KIND) {
            return Err(PargError::InvalidBindingsArtifact("artifact_kind mismatch"));
        }
        let target = object
            .get("target")
            .and_then(Value::as_str)
            .ok_or_else(|| PargError::UnknownBindingsTarget("<missing>".to_string()))?;
        if !BINDING_TARGETS.contains(&target) {
            return Err(PargError::UnknownBindingsTarget(target.to_string()));
        }
        match object.get("binding_version").and_then(Value::as_i64) {
            Some(version) if version == SUPPORTED_BINDING_VERSION => {}
            other => {
                return Err(PargError::UnsupportedBindingVersion(
                    other.unwrap_or_default(),
                ));
            }
        }
        if !object.get("entries").is_some_and(Value::is_object) {
            return Err(PargError::InvalidBindingsArtifact("no entries map"));
        }

        let stored = object
            .get("checksum")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let mut payload = self.payload.clone();
        payload
            .as_object_mut()
            .expect("verified as an object above")
            .remove("checksum");
        let computed = checksum_hex(&payload)?;
        if stored != computed {
            return Err(PargError::BindingsChecksumMismatch { stored, computed });
        }
        Ok(())
    }

    /// Pin the expected target (`ruby` / `rs` / `ts` / `python`).
    pub fn verify_target(&self, expected: &str) -> Result<(), PargError> {
        let declared = self
            .payload
            .get("target")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if declared == expected {
            Ok(())
        } else {
            Err(PargError::BindingsTargetMismatch {
                declared: declared.to_string(),
                expected: expected.to_string(),
            })
        }
    }

    /// The declared target.
    pub fn target(&self) -> &str {
        self.payload
            .get("target")
            .and_then(Value::as_str)
            .unwrap_or_default()
    }

    /// The grammar name the records were declared under.
    pub fn grammar(&self) -> &str {
        self.payload
            .get("grammar")
            .and_then(Value::as_str)
            .unwrap_or_default()
    }

    /// The grammar version at emission time.
    pub fn grammar_version(&self) -> &str {
        self.payload
            .get("grammar_version")
            .and_then(Value::as_str)
            .unwrap_or_default()
    }

    /// The source hash pinning the paired grammar text.
    pub fn source_hash(&self) -> &str {
        self.payload
            .get("grammar_source_hash")
            .and_then(Value::as_str)
            .unwrap_or_default()
    }

    /// The declared entry names.
    pub fn entry_names(&self) -> Vec<String> {
        self.payload
            .get("entries")
            .and_then(Value::as_object)
            .map(|map| map.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// An entry's declared binding records (empty when none).
    pub fn bindings(&self, entry: &str) -> &[Value] {
        self.payload
            .get("entries")
            .and_then(|entries| entries.get(entry))
            .and_then(|slot| slot.get("bindings"))
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Pair-check against the parse artifact this layer was compiled
    /// from: same grammar, same version, same source.
    fn verify_pairing(&self, parg: &PargArtifact) -> Result<(), PargError> {
        let envelope = &parg.envelope;
        let mut mismatches: Vec<String> = Vec::new();
        if self.grammar()
            != envelope
                .get("grammar")
                .and_then(Value::as_str)
                .unwrap_or_default()
        {
            mismatches.push("grammar".to_string());
        }
        if self.grammar_version()
            != envelope
                .get("version")
                .and_then(Value::as_str)
                .unwrap_or_default()
        {
            mismatches.push("version".to_string());
        }
        if self.source_hash()
            != envelope
                .get("source_hash")
                .and_then(Value::as_str)
                .unwrap_or_default()
        {
            mismatches.push("source_hash".to_string());
        }
        if mismatches.is_empty() {
            Ok(())
        } else {
            Err(PargError::BindingsPairingMismatch(mismatches.join(", ")))
        }
    }

    /// Materialize the bound map from a parse shape using the DECLARED
    /// records. The paired parse artifact supplies entry context and
    /// preprocess resolution; the same apply core runs as for the
    /// embedded records, so the declared artifact and the baked records
    /// produce identical bound maps.
    pub fn apply(
        &self,
        parg: &PargArtifact,
        entry: &str,
        shape: &Value,
    ) -> Result<Value, PargError> {
        self.verify_pairing(parg)?;
        let env = ApplyEnv::of_artifact(&parg.envelope);
        apply_entry_records(&env, self.bindings(entry), shape)
    }
}
