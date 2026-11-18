//! The parsanol language server: parsanol parsing PARG with its own
//! diagnostics (rs#529). stdio transport, hand-rolled JSON-RPC 2.0
//! framing — no protocol framework dependency.

use parsanol_lsp::server;
use serde_json::{json, Value};
use std::io::{Read, Write};

fn main() {
    let server = server::Server::new();
    let stdout = std::io::stdout();
    let mut reader = std::io::stdin().lock();
    let mut open_doc: Option<String> = None;

    while let Some(message) = read_message(&mut reader) {
        let Ok(msg) = serde_json::from_str::<Value>(&message) else {
            continue;
        };
        let method = msg["method"].as_str().unwrap_or("").to_string();
        let id = msg["id"].clone();
        let params = msg["params"].clone();

        match method.as_str() {
            "initialize" => {
                let result = json!({
                    "capabilities": {
                        "textDocumentSync": 1, // full
                        "hoverProvider": true,
                        "definitionProvider": true,
                        "referencesProvider": true,
                        "completionProvider": { "triggerCharacters": ["_", "-"] },
                        "documentSymbolProvider": true,
                        "experimentalProvider": { "parg/treePreview": true },
                    },
                    "serverInfo": { "name": "parsanol-lsp", "version": env!("CARGO_PKG_VERSION") },
                });
                respond(&stdout, id, Some(result), None);
            }
            "initialized" => {}
            "shutdown" => respond(&stdout, id, Some(Value::Null), None),
            "exit" => break,
            "textDocument/didOpen" | "textDocument/didChange" => {
                let text = params["textDocument"]["text"]
                    .as_str()
                    .map(|s| s.to_string())
                    .or_else(|| {
                        params["contentChanges"]
                            .as_array()
                            .and_then(|c| c.last())
                            .and_then(|c| c["text"].as_str())
                            .map(|s| s.to_string())
                    });
                if let Some(text) = text {
                    let diags = server.diagnostics(&text);
                    notify(
                        &stdout,
                        "textDocument/publishDiagnostics",
                        json!({ "uri": params["textDocument"]["uri"], "diagnostics": diags }),
                    );
                    open_doc = Some(text);
                }
            }
            "textDocument/hover" => {
                let text = open_doc.clone().unwrap_or_default();
                let offset = offset_of(&text, &params["position"]);
                respond(&stdout, id, Some(server.hover(&text, offset)), None);
            }
            "textDocument/definition" => {
                let text = open_doc.clone().unwrap_or_default();
                let offset = offset_of(&text, &params["position"]);
                let mut result = server.definition(&text, offset);
                if let Some(uri) = params["textDocument"]["uri"].as_str() {
                    if result.is_object() {
                        result["uri"] = Value::String(uri.to_string());
                        if let Some(p) = result.get_mut("uri_placeholder") {
                            p.take();
                        }
                    }
                }
                respond(&stdout, id, Some(result), None);
            }
            "textDocument/references" => {
                let text = open_doc.clone().unwrap_or_default();
                let offset = offset_of(&text, &params["position"]);
                let include_decl = params["context"]["includeDeclaration"]
                    .as_bool()
                    .unwrap_or(true);
                let mut result = server.references(&text, offset, include_decl);
                if let Some(uri) = params["textDocument"]["uri"].as_str() {
                    if let Some(items) = result.as_array_mut() {
                        for item in items {
                            item["uri"] = Value::String(uri.to_string());
                        }
                    }
                }
                respond(&stdout, id, Some(result), None);
            }
            "textDocument/completion" => {
                let text = open_doc.clone().unwrap_or_default();
                let offset = offset_of(&text, &params["position"]);
                respond(&stdout, id, Some(server.completion(&text, offset)), None);
            }
            "textDocument/documentSymbol" => {
                let text = open_doc.clone().unwrap_or_default();
                respond(&stdout, id, Some(server.symbols(&text)), None);
            }
            "parg/treePreview" => {
                let text = open_doc.clone().unwrap_or_default();
                respond(&stdout, id, Some(server.tree_preview(&text)), None);
            }
            _ => {
                // A notification or an unknown request: keep the
                // protocol alive; unknown REQUESTS get a method-not-found.
                if !id.is_null() {
                    respond(
                        &stdout,
                        id,
                        None,
                        Some(json!({ "code": -32601, "message": "method not found" })),
                    );
                }
            }
        }
    }
}

fn offset_of(text: &str, position: &Value) -> usize {
    let line = position["line"].as_u64().unwrap_or(0) as usize;
    let character = position["character"].as_u64().unwrap_or(0) as usize;
    let mut offset = 0usize;
    for (seen, l) in text.split_inclusive('\n').enumerate() {
        if seen == line {
            return offset + character.min(l.len());
        }
        offset += l.len();
    }
    offset
}

fn read_message(reader: &mut impl Read) -> Option<String> {
    // LSP framing: Content-Length: N\r\n\r\n + N bytes.
    let mut header = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        match reader.read(&mut byte) {
            Ok(0) => return None,
            Ok(_) => {
                header.push(byte[0]);
                if header.ends_with(b"\r\n\r\n") {
                    break;
                }
                if header.len() > 16 * 1024 {
                    return None;
                }
            }
            Err(_) => return None,
        }
    }
    let header = String::from_utf8_lossy(&header);
    let length = header
        .lines()
        .find_map(|l| l.strip_prefix("Content-Length:"))?
        .trim()
        .parse::<usize>()
        .ok()?;
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).ok()?;
    Some(String::from_utf8_lossy(&body).to_string())
}

fn write_message(mut out: impl Write, body: &str) {
    let _ = write!(out, "Content-Length: {}\r\n\r\n{}", body.len(), body);
    let _ = out.flush();
}

fn respond(_out: &std::io::Stdout, id: Value, result: Option<Value>, error: Option<Value>) {
    let mut body = json!({ "jsonrpc": "2.0", "id": id });
    if let Some(r) = result {
        body["result"] = r;
    }
    if let Some(e) = error {
        body["error"] = e;
    }
    write_message(std::io::stdout().lock(), &body.to_string());
}

fn notify(_out: &std::io::Stdout, method: &str, params: Value) {
    let body = json!({ "jsonrpc": "2.0", "method": method, "params": params });
    write_message(std::io::stdout().lock(), &body.to_string());
}
