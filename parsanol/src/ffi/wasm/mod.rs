//! WASM bindings for Parsanol
//!
//! This module provides JavaScript/Opal bindings for the portable parser.
//! When compiled with the `wasm` feature, this exposes a `WasmParser` class
//! that can be used from JavaScript.

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

/// A verified PARG artifact exposed to JavaScript: checksum-verified load,
/// native parse, bindings application, embedded-suite runs and the
/// binding-requirements schema (PN 2/4).
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
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        serde_json::to_string(&shape)
            .map(JsString::from)
            .map_err(|_| JsValue::from_str("Failed to serialize shape"))
    }

    /// Apply an entry's bindings to a parsanol-shape tree (JSON text).
    #[wasm_bindgen(js_name = applyBindings)]
    pub fn apply_bindings(&self, entry: &str, shape_json: &str) -> Result<JsString, JsValue> {
        let shape: serde_json::Value = serde_json::from_str(shape_json)
            .map_err(|e| JsValue::from_str(&format!("Invalid shape JSON: {e}")))?;
        let bound = self
            .artifact
            .apply_bindings(entry, &shape)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        serde_json::to_string(&bound)
            .map(JsString::from)
            .map_err(|_| JsValue::from_str("Failed to serialize bindings"))
    }

    /// Parse and bind in one step.
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
}

/// Initialize the WASM module
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

/// Initialize function for WASM
#[wasm_bindgen]
pub fn init() {
    // Set up panic hook for better error messages
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
}
