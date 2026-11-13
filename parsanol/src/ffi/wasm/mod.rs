//! WASM bindings for Parsanol
//!
//! This module provides JavaScript/Opal bindings for the portable parser.
//! When compiled with the `wasm` feature, this exposes a `WasmParser` class
//! that can be used from JavaScript.

use crate::parg::export::ExportTarget;
use crate::parg::PargArtifact;
use crate::portable::{AstArena, AstNode, Grammar, PortableParser};
use js_sys::{Array, JsString, Object, Reflect};
use wasm_bindgen::prelude::*;

/// WASM parser instance
///
/// Create with `new WasmParser(grammarJson)` and parse with `parse(input)`.
#[wasm_bindgen]
pub struct WasmParser {
    grammar: Grammar,
    arena: AstArena,
}

#[wasm_bindgen]
impl WasmParser {
    /// Create a new parser from grammar JSON
    ///
    /// # Arguments
    /// * `grammar_json` - JSON string representing the grammar
    ///
    /// # Returns
    /// A new WasmParser instance
    ///
    /// # Throws
    /// If the grammar JSON is invalid
    #[wasm_bindgen(constructor)]
    pub fn new(grammar_json: &str) -> Result<WasmParser, JsValue> {
        let grammar: Grammar = Grammar::from_json(grammar_json)
            .map_err(|e| JsValue::from_str(&format!("Invalid grammar JSON: {}", e)))?;

        Ok(WasmParser {
            grammar,
            arena: AstArena::new(),
        })
    }

    /// Create a new parser from a PARG artifact envelope and entry name.
    ///
    /// The artifact's canonical checksum is verified before any parsing;
    /// a mismatched or corrupt artifact is rejected (PN 2).
    ///
    /// # Arguments
    /// * `artifact_json` - JSON text of the PARG artifact envelope
    /// * `entry` - entry point name declared in the artifact
    #[wasm_bindgen(js_name = fromArtifact)]
    pub fn from_artifact(artifact_json: &str, entry: &str) -> Result<WasmParser, JsValue> {
        let artifact = PargArtifact::from_json(artifact_json)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let grammar = artifact
            .grammar(entry)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        Ok(WasmParser {
            grammar,
            arena: AstArena::new(),
        })
    }

    /// Parse input and return JavaScript AST
    ///
    /// # Arguments
    /// * `input` - The input string to parse
    ///
    /// # Returns
    /// A JavaScript object representing the parsed AST
    ///
    /// # Throws
    /// If parsing fails
    #[wasm_bindgen]
    pub fn parse(&mut self, input: &str) -> Result<JsValue, JsValue> {
        // Reset arena, re-sizing if needed for significantly different input sizes
        let input_len = input.len();
        let current_capacity = self.arena.capacity();
        let optimal_capacity = (input_len / 10).clamp(64, 100_000) * 2;

        // Re-create arena if size difference is significant (> 4x)
        if current_capacity < optimal_capacity / 4 || current_capacity > optimal_capacity * 4 {
            self.arena = AstArena::for_input(input_len);
        } else {
            self.arena.reset();
        }

        let mut parser = PortableParser::new(&self.grammar, input, &mut self.arena);
        let ast = parser
            .parse()
            .map_err(|e| JsValue::from_str(&format!("Parse error: {}", e)))?;

        Ok(ast_to_js(&ast, &self.arena, input))
    }

    /// Parse and return flat array (for Opal compatibility)
    ///
    /// # Arguments
    /// * `input` - The input string to parse
    ///
    /// # Returns
    /// A Uint32Array containing the flattened AST
    ///
    /// # Throws
    /// If parsing fails
    #[wasm_bindgen]
    pub fn parse_flat(&mut self, input: &str) -> Result<Vec<u64>, JsValue> {
        self.arena.reset();

        let mut parser = PortableParser::new(&self.grammar, input, &mut self.arena);
        let ast = parser
            .parse()
            .map_err(|e| JsValue::from_str(&format!("Parse error: {}", e)))?;

        // Use the unified flatten implementation
        let mut result = Vec::new();
        crate::ffi::shared::flatten_ast_to_u64(&ast, &self.arena, input, &mut result);

        Ok(result)
    }

    /// Parse and return JSON string
    ///
    /// # Arguments
    /// * `input` - The input string to parse
    ///
    /// # Returns
    /// A JSON string representing the parsed AST
    ///
    /// # Throws
    /// If parsing fails
    #[wasm_bindgen]
    pub fn parse_json(&mut self, input: &str) -> Result<String, JsValue> {
        self.arena.reset();

        let mut parser = PortableParser::new(&self.grammar, input, &mut self.arena);
        let ast = parser
            .parse()
            .map_err(|e| JsValue::from_str(&format!("Parse error: {}", e)))?;

        let js_val = ast_to_js(&ast, &self.arena, input);
        let json = js_sys::JSON::stringify(&js_val)
            .map_err(|_| JsValue::from_str("Failed to serialize AST"))?;

        Ok(json.as_string().unwrap_or_default())
    }
}

/// Convert AST node to JavaScript value
fn ast_to_js(node: &AstNode, arena: &AstArena, input: &str) -> JsValue {
    match node {
        AstNode::Nil => JsValue::NULL,

        AstNode::Bool(b) => JsValue::from_bool(*b),

        AstNode::Int(n) => JsValue::from_f64(*n as f64),

        AstNode::Float(f) => JsValue::from_f64(*f),

        AstNode::StringRef { pool_index } => {
            let s = arena.get_string(*pool_index as usize);
            JsString::from(s).into()
        }

        AstNode::InputRef { offset, length } => {
            let s = &input[*offset as usize..*offset as usize + *length as usize];
            JsString::from(s).into()
        }

        AstNode::Array { pool_index, length } => {
            let items = arena.get_array(*pool_index as usize, *length as usize);
            let arr = Array::new();
            for item in items {
                arr.push(&ast_to_js(&item, arena, input));
            }
            arr.into()
        }

        AstNode::Hash { pool_index, length } => {
            let pairs = arena.get_hash_items(*pool_index as usize, *length as usize);
            let obj = Object::new();
            for (key, value) in pairs {
                let js_key = JsString::from(key.as_str());
                let js_val = ast_to_js(&value, arena, input);
                Reflect::set(&obj, &js_key.into(), &js_val)
                    .unwrap_or_else(|_| panic!("Failed to set hash key"));
            }
            obj.into()
        }
    }
}

/// Compile PARG source to a checksummed artifact — the playground's
/// compiler-in-the-browser (site#19). Single-file grammars: `use`
/// imports have no filesystem here and reject with the merge error.
#[wasm_bindgen(js_name = compileSource)]
pub fn compile_source(source: &str) -> Result<String, JsValue> {
    serde_json::to_string(
        &crate::parg::frontend::envelope::compile_source(source)
            .map_err(|e| JsValue::from_str(&e))?,
    )
    .map_err(|_| JsValue::from_str("Failed to serialize artifact"))
}

fn parg_error_to_js(error: crate::parg::PargError) -> JsValue {
    let resource = match &error {
        crate::parg::PargError::RecursionLimitExceeded { depth, max_depth } => Some((
            "recursion_limit",
            "RecursionLimitExceeded",
            *depth,
            *max_depth,
        )),
        crate::parg::PargError::CallDepthLimitExceeded { depth, max_depth } => Some((
            "call_depth_limit",
            "CallDepthLimitExceeded",
            *depth,
            *max_depth,
        )),
        _ => None,
    };
    if let Some((category, name, depth, max_depth)) = resource {
        let value = js_sys::Error::new(&error.to_string());
        value.set_name(name);
        for (key, data) in [
            ("category", JsValue::from_str(category)),
            ("depth", JsValue::from_f64(depth as f64)),
            ("max_depth", JsValue::from_f64(max_depth as f64)),
        ] {
            let _ = js_sys::Reflect::set(&value, &JsValue::from_str(key), &data);
        }
        value.into()
    } else {
        JsValue::from_str(&error.to_string())
    }
}

/// Bound JSON container nesting before using serde's recursive Value decoder.
/// This scans structural bytes only; serde remains the JSON syntax authority.
/// The explicit budget replaces serde's implicit 127-container budget for the
/// shape bridge, allowing parser-produced depth-64 trees to round-trip.
const MAX_SHAPE_JSON_DEPTH: usize = 512;

fn check_shape_json_depth(input: &str) -> Result<(), usize> {
    let (mut depth, mut quoted, mut escaped) = (0usize, false, false);
    for byte in input.bytes() {
        if quoted {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
        } else {
            match byte {
                b'"' => quoted = true,
                b'[' | b'{' => {
                    depth += 1;
                    if depth > MAX_SHAPE_JSON_DEPTH {
                        return Err(depth);
                    }
                }
                b']' | b'}' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    Ok(())
}

fn binding_shape_from_json(input: &str) -> Result<serde_json::Value, JsValue> {
    if let Err(depth) = check_shape_json_depth(input) {
        let error = js_sys::Error::new("Binding shape exceeds the JSON container depth budget");
        error.set_name("ShapeDepthLimitExceeded");
        for (key, value) in [
            ("category", JsValue::from_str("shape_depth_limit")),
            ("depth", JsValue::from_f64(depth as f64)),
            ("max_depth", JsValue::from_f64(MAX_SHAPE_JSON_DEPTH as f64)),
        ] {
            let _ = js_sys::Reflect::set(&error, &JsValue::from_str(key), &value);
        }
        return Err(error.into());
    }
    let mut decoder = serde_json::Deserializer::from_str(input);
    // Safe only after the explicit structural budget above. Keep the default
    // serde guard on all unrelated JSON/artifact/model decoders.
    decoder.disable_recursion_limit();
    let shape = <serde_json::Value as serde::Deserialize>::deserialize(&mut decoder)
        .map_err(|error| JsValue::from_str(&format!("Invalid shape JSON: {error}")))?;
    decoder
        .end()
        .map_err(|error| JsValue::from_str(&format!("Invalid shape JSON: {error}")))?;
    Ok(shape)
}

#[cfg(test)]
mod shape_depth_tests {
    use super::*;
    #[test]
    fn structural_budget_ignores_quoted_and_escaped_delimiters() {
        assert_eq!(
            check_shape_json_depth(&format!("{}0{}", "[".repeat(512), "]".repeat(512))),
            Ok(())
        );
        assert_eq!(
            check_shape_json_depth(&format!("{}0{}", "[".repeat(513), "]".repeat(513))),
            Err(513)
        );
        assert_eq!(check_shape_json_depth(r#"{"text":"[\"{{]"}"#), Ok(()));
        assert_eq!(check_shape_json_depth("[[]] [[],[]]"), Ok(()));
    }
}

/// A verified PARG artifact exposed to JavaScript: checksum-verified
/// load, native parse, bindings application, embedded-suite runs and
/// the binding-requirements schema (PN 2/4).
#[wasm_bindgen]
pub struct PargArtifactJs {
    artifact: PargArtifact,
}

#[wasm_bindgen]
impl PargArtifactJs {
    /// Load and checksum-verify an artifact envelope.
    #[wasm_bindgen(constructor)]
    pub fn new(artifact_json: &str) -> Result<PargArtifactJs, JsValue> {
        let artifact = PargArtifact::from_json(artifact_json)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(PargArtifactJs { artifact })
    }

    /// Declared entry point names, sorted.
    #[wasm_bindgen(js_name = entryNames)]
    pub fn entry_names(&self) -> Vec<JsString> {
        self.artifact
            .entry_names()
            .into_iter()
            .map(JsString::from)
            .collect()
    }

    /// Parse an entry with the native engine; returns the parsanol-shape
    /// tree as JSON text.
    #[wasm_bindgen(js_name = parseShape)]
    pub fn parse_shape(&self, entry: &str, input: &str) -> Result<JsString, JsValue> {
        let shape = self
            .artifact
            .parse_shape(entry, input)
            .map_err(parg_error_to_js)?;
        serde_json::to_string(&shape)
            .map(JsString::from)
            .map_err(|_| JsValue::from_str("Failed to serialize shape"))
    }

    /// Explicit compatibility without changing parseShape defaults.
    /// The JSON envelope contains either shape or causal error diagnostics;
    /// it does not claim best-effort recovery captures.
    #[wasm_bindgen(js_name = parseShapeWithCompatibility)]
    pub fn parse_shape_with_compatibility(
        &self,
        entry: &str,
        input: &str,
        compatibility: &str,
    ) -> Result<JsString, JsValue> {
        use crate::portable::parser::ParserCompatibility;
        let policy = match compatibility {
            "strict" => ParserCompatibility::Strict,
            "parslet" => ParserCompatibility::Parslet,
            _ => return Err(JsValue::from_str("unknown parsing compatibility policy")),
        };
        let value = match self
            .artifact
            .parse_shape_with_compatibility(entry, input, policy)
        {
            Ok(shape) => serde_json::json!({"ok": true, "shape": shape}),
            Err(crate::parg::PargError::ParseWire { offset, ranked, .. }) => {
                serde_json::json!({"ok": false, "error": {"offset": offset, "ranked": ranked}})
            }
            Err(crate::parg::PargError::RecursionLimitExceeded { depth, max_depth }) => {
                serde_json::json!({"ok": false, "error": {"category": "recursion_limit", "offset": null, "depth": depth, "max_depth": max_depth}})
            }
            Err(crate::parg::PargError::CallDepthLimitExceeded { depth, max_depth }) => {
                serde_json::json!({"ok": false, "error": {"category": "call_depth_limit", "offset": null, "depth": depth, "max_depth": max_depth}})
            }
            Err(error) => return Err(parg_error_to_js(error)),
        };
        serde_json::to_string(&value)
            .map(JsString::from)
            .map_err(|_| JsValue::from_str("Failed to serialize parse result"))
    }

    /// Best-effort recovery (TODO.sota/1): success returns
    /// `{"ok": true, "shape": ...}`; failure returns
    /// `{"ok": false, "captures": ..., "error": ..., "consumed_upto"}`.
    #[wasm_bindgen(js_name = parseShapeRecovering)]
    pub fn parse_shape_recovering(&self, entry: &str, input: &str) -> Result<JsString, JsValue> {
        let recovered = self
            .artifact
            .parse_shape_recovering(entry, input)
            .map_err(parg_error_to_js)?;
        serde_json::to_string(&recovered)
            .map(JsString::from)
            .map_err(|_| JsValue::from_str("Failed to serialize recovery"))
    }

    /// Apply an entry's bindings to a parsanol-shape tree (JSON text).
    #[wasm_bindgen(js_name = applyBindings)]
    pub fn apply_bindings(&self, entry: &str, shape_json: &str) -> Result<JsString, JsValue> {
        let shape = binding_shape_from_json(shape_json)?;
        let bound = self
            .artifact
            .apply_bindings(entry, &shape)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        serde_json::to_string(&bound)
            .map(JsString::from)
            .map_err(|_| JsValue::from_str("Failed to serialize bindings"))
    }

    /// Apply an entry's DECLARED bindings artifact (parsanol-ruby#432)
    /// to a parsanol-shape tree. The bindings artifact is verified
    /// (kind, checksum, `ts` target, binding version) and pair-checked
    /// against this parse artifact by grammar source hash before the
    /// shared apply core runs.
    #[wasm_bindgen(js_name = applyBindingsArtifact)]
    pub fn apply_bindings_artifact(
        &self,
        bindings_json: &str,
        entry: &str,
        shape_json: &str,
    ) -> Result<JsString, JsValue> {
        let bindings = crate::parg::bindings_artifact::PargBindingsArtifact::from_json(
            bindings_json,
            Some("ts"),
        )
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let shape = binding_shape_from_json(shape_json)?;
        let bound = bindings
            .apply(&self.artifact, entry, &shape)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        serde_json::to_string(&bound)
            .map(JsString::from)
            .map_err(|_| JsValue::from_str("Failed to serialize bindings"))
    }

    /// Parse and bind in one step: the bound attribute hash as JSON.
    ///
    /// Returns `{}` when the entry declares no `bindings` block — the
    /// parse succeeded, there was simply nothing to bind.
    #[wasm_bindgen(js_name = parseAndBind)]
    pub fn parse_and_bind(&self, entry: &str, input: &str) -> Result<JsString, JsValue> {
        let bound = self
            .artifact
            .parse_and_bind(entry, input)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        serde_json::to_string(&bound)
            .map(JsString::from)
            .map_err(|_| JsValue::from_str("Failed to serialize bindings"))
    }

    /// Run the artifact's embedded tests; returns the failure list as
    /// JSON (empty array means green).
    #[wasm_bindgen(js_name = runTests)]
    pub fn run_tests(&self) -> Result<JsString, JsValue> {
        let failures = self.artifact.run_tests();
        serde_json::to_string(&failures)
            .map(JsString::from)
            .map_err(|_| JsValue::from_str("Failed to serialize failures"))
    }

    /// The binding-requirements schema as JSON text.
    pub fn schema(&self) -> Result<JsString, JsValue> {
        let schema = crate::parg::schema::from_artifact(&self.artifact)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        serde_json::to_string(&schema)
            .map(JsString::from)
            .map_err(|_| JsValue::from_str("Failed to serialize schema"))
    }

    /// The binding-requirements schema as TypeScript source.
    #[wasm_bindgen(js_name = schemaTypescript)]
    pub fn schema_typescript(&self) -> Result<JsString, JsValue> {
        let schema = crate::parg::schema::from_artifact(&self.artifact)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(JsString::from(crate::parg::schema::to_typescript(&schema)))
    }

    /// The semantics hash (ruby#377): sha256 over the canonicalized
    /// envelope minus the producer set — byte-stable across re-bakes
    /// by a newer compiler when the grammar is unchanged. `undefined`
    /// on artifacts baked before the field existed.
    #[wasm_bindgen(js_name = semanticsHash)]
    pub fn semantics_hash(&self) -> Option<JsString> {
        self.artifact
            .semantics_hash()
            .map(|hash| JsString::from(hash.to_string()))
    }

    /// Lower the artifact to a constrained-decoder grammar form
    /// (`target`: "xgrammar" | "llguidance"); `recovery` lowers each
    /// declared resync pattern as an escape alternative at the
    /// repetition spine. Rust is the authoritative lowering — TS
    /// inherits it through this surface instead of a per-language port.
    #[wasm_bindgen(js_name = export)]
    pub fn export(&self, target: &str, recovery: bool) -> Result<JsString, JsValue> {
        let target = match target {
            "xgrammar" => ExportTarget::Xgrammar,
            "llguidance" => ExportTarget::Llguidance,
            other => {
                return Err(JsValue::from_str(&format!(
                    "unknown export target {other:?} (supported: xgrammar, llguidance)"
                )))
            }
        };
        self.artifact
            .export(target, recovery)
            .map(JsString::from)
            .map_err(|e| JsValue::from_str(&e.message))
    }

}

/// Initialize the WASM module
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

/// Install the panic hook (better panic messages in the console).
///
/// The glue's real bootstrap is the DEFAULT export —
/// `__wbg_init(module_or_path?)` / `initSync` — which calls this on
/// its way up; calling it directly is only needed outside the
/// bootstrap. It is deliberately NOT named `init`: that name in the
/// generated d.ts typed the bootstrap as `() => void` and hid the
/// real signatures (rs#308).
#[wasm_bindgen(js_name = setPanicHook)]
pub fn set_panic_hook() {
    // Set up panic hook for better error messages
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
}
