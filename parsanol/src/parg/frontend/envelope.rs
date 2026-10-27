//! The artifact envelope assembly (rs#298 slice 4c): everything the
//! Ruby compiler's `build_envelope` emits around the per-entry
//! grammars — pass-through document data, the computed entries map,
//! the order-dependence lint output, and the freshness fields. The
//! checksum and `compiler_version` are the caller's (the parity gate
//! compares everything else byte-for-byte against Ruby's artifacts).

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use super::compile::{self, CompileError, Tables};
use super::imports;
use super::parser::{self, Document, Node, ParsedSource};
use crate::parg::checksum_hex;

/// Assemble the full envelope a Ruby compile would emit (minus
/// `checksum` and `compiler_version`).
pub fn build(
    parsed: &ParsedSource,
    tables: &Tables,
    bindings: Option<Value>,
) -> Result<Value, CompileError> {
    build_mode(parsed, tables, bindings, None)
}

/// The envelope with the header's capture mode (ruby#304).
pub fn build_mode(
    parsed: &ParsedSource,
    tables: &Tables,
    bindings: Option<Value>,
    mode: Option<&str>,
) -> Result<Value, CompileError> {
    let document = &parsed.document;
    let mut compiler = compile::compiler_for_mode(document, tables, &parsed.atomic_rules, mode)?;

    let mut entries = Map::new();
    for (name, rule) in &document.entries {
        let grammar = compiler.compile_entry_grammar(rule)?;
        let mut envelope = json!({
            "root": rule,
            "grammar": serde_json::to_value(&grammar)
                .expect("grammar serializes"),
            "bindings": binding_list(document, rule),
        });
        if let Some(pattern) = document
            .resyncs
            .get(rule)
            .or_else(|| document.resyncs.get(&rule.to_string()))
        {
            let resync = compiler.compile_resync_grammar(pattern)?;
            envelope["resync"] = serde_json::to_value(&resync).expect("resync grammar serializes");
        }
        entries.insert(name.clone(), envelope);
    }

    let default_entry = if parsed.own_entries.len() == 1 {
        parsed.own_entries.first().cloned()
    } else if document.entries.len() == 1 {
        document.entries.first().map(|(name, _)| name.clone())
    } else {
        None
    };

    let tests: Vec<Value> = document
        .tests
        .iter()
        .map(|test| {
            json!({
                "entry": test.entry.clone().or_else(|| {
                    if parsed.own_entries.len() == 1 {
                        parsed.own_entries.first().cloned()
                    } else {
                        None
                    }
                }),
                "kind": test.kind,
                "input": test.input,
                "expect": test.expect,
            })
        })
        .collect();

    let mut envelope = json!({
        "bindings": bindings,
        "version": document.version,
        "grammar": document.grammar_name,
        "shape": "parsanol-tree/v2",
        "binding_version": 1,
        "entries": entries,
        "preprocess": serde_json::to_value(&document.preprocess)
            .expect("preprocess serializes"),
        "tables": table_manifest(document, tables),
        "lint": { "order_warnings": compiler.order_warnings() },
        "default_entry": default_entry,
        // No conformance fixture declares a render/derive section yet;
        // the envelope always carries the (empty) maps.
        "render": json!({}),
        "derive": json!({}),
        "tests": tests,
        "docs": serde_json::to_value(&parsed.docs).expect("docs serialize"),
        "source": parsed.source,
        "source_hash": source_hash(&parsed.source),
    });
    // ruby#304: the envelope always names the capture dialect —
    // consumers read the tree-shape contract before parsing.
    envelope["mode"] = Value::String(mode.unwrap_or("v2").to_string());
    // ruby#353: the capture-emission level the trees were frozen
    // against (must match the ruby compiler's Artifact::EMISSION_VERSION).
    envelope["emission"] = Value::from(crate::parg::EMISSION_VERSION);
    if document.skip_declaration.is_some() {
        envelope["skip"] = skip_source(document.skip_declaration.as_deref());
    }
    Ok(envelope)
}

/// Compile PARG source to a complete checksummed artifact envelope —
/// the compiler-in-the-browser's engine (site#19) and the native
/// round-trip tests' shared path. Single-file grammars: `use` imports
/// have no filesystem here and reject with the merge error. The
/// checksum covers this crate's version stamp, so the artifact is
/// self-consistent and honestly provenance-stamped.
pub fn compile_source(source: &str) -> Result<Value, String> {
    let (mut parsed, mode) = parser::parse_grammar_mode(source).map_err(|e| e.to_string())?;
    imports::merge(&mut parsed.document, &[]).map_err(|e| e.to_string())?;
    let mut value = build_mode(&parsed, &Tables::default(), None, mode.as_deref())
        .map_err(|e| e.to_string())?;
    if let Some(obj) = value.as_object_mut() {
        obj.remove("checksum");
        obj.insert(
            "compiler_version".to_string(),
            Value::String(env!("CARGO_PKG_VERSION").to_string()),
        );
        // The U3 provenance stamp (rs#297): the regex crate's UCD
        // tables are pinned transitively by this parsanol version's
        // lockfile. An engine-specific stamp, checksummed here and
        // excluded from the cross-engine parity set like
        // compiler_version.
        obj.insert(
            "unicode_tables".to_string(),
            Value::String(format!("regex (parsanol {})", env!("CARGO_PKG_VERSION"))),
        );
    }
    let checksum = checksum_hex(&value).map_err(|e| e.to_string())?;
    if let Some(obj) = value.as_object_mut() {
        obj.insert("checksum".to_string(), Value::String(checksum));
    }
    Ok(value)
}

fn source_hash(source: &str) -> String {
    let digest = Sha256::digest(source.as_bytes());
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

/// The Ruby binding_list: each declared binding as the envelope row.
fn binding_list(document: &Document, rule: &str) -> Value {
    let Some(rows) = document.bindings.get(rule) else {
        return json!([]);
    };
    Value::Array(
        rows.iter()
            .map(|binding| {
                json!({
                    "capture": binding.capture,
                    "path": binding.path,
                    "type": binding.kind,
                    "card": binding.card,
                    "preprocess": binding.preprocess,
                })
            })
            .collect(),
    )
}

/// The referenced tables' manifest: `{name: {file, rows}}` in
/// first-reference order (the Ruby table_manifest).
fn table_manifest(document: &Document, tables: &Tables) -> Value {
    let mut referenced: Vec<String> = Vec::new();
    for (_, node) in &document.rules {
        collect_tables(node, &mut referenced);
    }
    let mut manifest = Map::new();
    for name in referenced {
        let Some(rows) = tables.get(&name) else {
            continue;
        };
        manifest.insert(
            name.clone(),
            json!({
                "file": format!("{name}.yaml"),
                "rows": rows,
            }),
        );
    }
    Value::Object(manifest)
}

fn collect_tables(node: &Node, referenced: &mut Vec<String>) {
    match node.kind.as_str() {
        "table" => {
            let name = node.table.clone().unwrap_or_default();
            if !referenced.contains(&name) {
                referenced.push(name);
            }
        }
        "seq" | "alt" => {
            for child in node.items.iter().chain(node.branches.iter()).flatten() {
                collect_tables(child, referenced);
            }
        }
        "rep" | "opt" | "cap" | "pred" => {
            if let Some(child) = node.child.as_deref() {
                collect_tables(child, referenced);
            }
        }
        _ => {}
    }
}

/// The skip declaration's source spelling: rule references and ` / `
/// alternations (Ruby skip_source_string).
fn skip_source(node: Option<&Node>) -> Value {
    let Some(node) = node else {
        return Value::Null;
    };
    match node.kind.as_str() {
        "ref" => node.name.clone().map(Value::String).unwrap_or(Value::Null),
        "alt" => Value::String(
            node.branches
                .as_deref()
                .unwrap_or_default()
                .iter()
                .filter_map(|branch| match skip_source(Some(branch)) {
                    Value::String(text) => Some(text),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(" / "),
        ),
        _ => Value::Null,
    }
}
