//! The document state and the LSP feature handlers. One server holds
//! one document kind: .parg sources parsed by the self-hosting PgMeta
//! artifact (parsanol parsing PARG), with compile_source as the
//! authoritative validity oracle.

use crate::rules;
use parsanol::PargArtifact;
use serde_json::{json, Value};

pub struct Doc {
    pub text: String,
}

pub struct Server {
    pub artifact: PargArtifact,
    pub entry: String,
}

impl Default for Server {
    fn default() -> Self {
        Self::new()
    }
}

impl Server {
    pub fn new() -> Self {
        let raw = include_str!("../artifacts/parg.json");
        let artifact = PargArtifact::from_json(raw).expect("the embedded PgMeta artifact loads");
        Self {
            artifact,
            entry: "file".to_string(),
        }
    }

    /// The live diagnostics: the PgMeta parse's structured failure
    /// wire (the deepest position, the expected labels, the ranked
    /// failures) plus the authoritative compile verdict.
    /// Severity 1 = error, 2 = warning.
    pub fn diagnostics(&self, text: &str) -> Vec<Value> {
        let mut out = Vec::new();
        // The self-hosting parse: position-accurate structure errors
        // with the expected-set surface.
        match self.artifact.parse_shape(&self.entry, text) {
            Ok(_) => {}
            Err(parsanol::PargError::ParseWire {
                offset,
                line,
                column,
                expected,
                ranked,
            }) => {
                let mut message = format!("grammar does not parse at line {line}, column {column}");
                if !expected.is_empty() {
                    message.push_str("; expected ");
                    message.push_str(&expected.join(", "));
                }
                if !ranked.is_empty() && ranked.len() > 1 {
                    message.push_str(&format!(" (+{} ranked failures)", ranked.len() - 1));
                }
                out.push(json!({
                    "range": rules::range_of(text, offset, offset),
                    "severity": 1,
                    "source": "parsanol",
                    "message": message,
                    "parsanol": {
                        "expected": expected,
                        "ranked": ranked.iter().map(|(pos, labels)| json!({
                            "offset": pos,
                            "expected": labels,
                        })).collect::<Vec<_>>(),
                    },
                }));
            }
            Err(e) => {
                out.push(json!({
                    "range": rules::range_of(text, 0, 0),
                    "severity": 1,
                    "source": "parsanol",
                    "message": format!("grammar does not parse: {e}"),
                }));
            }
        }
        // The authoritative compile: the artifact verdict. The error
        // string may carry no position — anchor it at the top.
        if let Err(msg) = parsanol::parg::frontend::envelope::compile_source(text) {
            out.push(json!({
                "range": rules::range_of(text, 0, 0),
                "severity": 1,
                "source": "parsanol-compile",
                "message": format!("the grammar does not compile: {msg}"),
            }));
        }
        out
    }

    /// Hover: the rule's docs + the signature line.
    pub fn hover(&self, text: &str, offset: usize) -> Value {
        let Some((word, ws, we)) = rules::word_at(text, offset) else {
            return Value::Null;
        };
        let defs = rules::rule_defs(text);
        let Some(def) = rules::defs_of(&defs, &word).into_iter().next() else {
            return Value::Null;
        };
        let signature = text[def.line_start..def.line_end].trim().to_string();
        let mut contents = String::new();
        if !def.docs.is_empty() {
            contents.push_str(&def.docs.join("\n"));
            contents.push_str("\n\n");
        }
        contents.push_str("```parg\n");
        contents.push_str(&signature);
        contents.push_str("\n```");
        json!({
            "contents": { "kind": "markdown", "value": contents },
            "range": rules::range_of(text, ws, we),
        })
    }

    /// Go-to-definition: the word at the offset → the rule's def.
    pub fn definition(&self, text: &str, offset: usize) -> Value {
        let Some((word, _, _)) = rules::word_at(text, offset) else {
            return Value::Null;
        };
        let defs = rules::rule_defs(text);
        let Some(def) = rules::defs_of(&defs, &word).into_iter().next() else {
            return Value::Null;
        };
        json!({
            "uri_placeholder": (),
            "range": rules::range_of(text, def.name_start, def.name_end),
        })
    }

    /// Find-references: the rule at the offset → every occurrence.
    pub fn references(&self, text: &str, offset: usize, include_decl: bool) -> Value {
        let Some((word, _, _)) = rules::word_at(text, offset) else {
            return Value::Null;
        };
        let names: Vec<String> = rules::rule_defs(text)
            .iter()
            .map(|r| r.name.clone())
            .collect();
        let locations: Vec<Value> = rules::occurrences(text, &names)
            .into_iter()
            .filter(|o| o.name == word && (include_decl || !o.is_def))
            .map(|o| {
                json!({
                    "uri": "",
                    "range": rules::range_of(text, o.start, o.end),
                })
            })
            .collect();
        json!(locations)
    }

    /// Completion: the rule names + the keywords, filtered by the
    /// prefix at the offset.
    pub fn completion(&self, text: &str, offset: usize) -> Value {
        let prefix = rules::word_at(text, offset)
            .map(|(w, _, _)| w)
            .unwrap_or_default();
        let mut items = Vec::new();
        for def in rules::rule_defs(text) {
            if def.name.starts_with(&prefix) {
                items.push(json!({
                    "label": def.name,
                    "kind": 3, // Function-ish: a rule reference
                    "documentation": if def.docs.is_empty() {
                        Value::Null
                    } else {
                        Value::String(def.docs.join("\n"))
                    },
                }));
            }
        }
        for kw in rules::KEYWORDS {
            if kw.starts_with(&prefix) {
                items.push(json!({ "label": kw, "kind": 14 })); // Keyword
            }
        }
        json!({ "isIncomplete": false, "items": items })
    }

    /// Document symbols: the rule list.
    pub fn symbols(&self, text: &str) -> Value {
        let defs = rules::rule_defs(text);
        let symbols: Vec<Value> = defs
            .iter()
            .map(|d| {
                json!({
                    "name": d.name,
                    "kind": 12, // Function
                    "range": rules::range_of(text, d.line_start, d.line_end),
                    "selectionRange": rules::range_of(text, d.name_start, d.name_end),
                })
            })
            .collect();
        json!(symbols)
    }

    /// The parse-tree preview (the differentiator): the PgMeta shape
    /// tree of the document, rendered as indented text.
    pub fn tree_preview(&self, text: &str) -> Value {
        match self.artifact.parse_shape(&self.entry, text) {
            Ok(shape) => {
                let mut out = String::new();
                render_shape(&shape, 0, &mut out);
                json!({ "ok": true, "preview": out })
            }
            Err(e) => json!({ "ok": false, "error": format!("{e}") }),
        }
    }
}

fn render_shape(node: &Value, depth: usize, out: &mut String) {
    match node {
        Value::String(s) => {
            out.push_str(&"  ".repeat(depth));
            out.push_str(s);
            out.push('\n');
        }
        Value::Array(items) => {
            for item in items {
                render_shape(item, depth, out);
            }
        }
        Value::Object(map) => {
            for (key, value) in map {
                out.push_str(&"  ".repeat(depth));
                out.push_str(key);
                out.push(':');
                out.push('\n');
                render_shape(value, depth + 1, out);
            }
        }
        other => {
            out.push_str(&"  ".repeat(depth));
            out.push_str(&other.to_string());
            out.push('\n');
        }
    }
}
