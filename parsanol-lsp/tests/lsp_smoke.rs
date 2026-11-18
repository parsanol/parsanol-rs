//! The smoke receipt (rs#529): a scripted editor session driving the
//! server over stdio — initialize, didOpen with a deliberate error
//! (the ParseWire diagnostics), hover, go-to-definition, references,
//! completion, document symbols, the parse-tree preview, shutdown.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command, Stdio};

struct Client {
    child: Child,
    stdin: std::process::ChildStdin,
    reader: BufReader<std::process::ChildStdout>,
    next_id: u64,
}

impl Client {
    fn spawn() -> Client {
        let bin = env!("CARGO_BIN_EXE_parsanol-lsp");
        let mut child = Command::new(bin)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("parsanol-lsp spawns");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = child.stdout.take().expect("stdout");
        Client {
            child,
            stdin,
            reader: BufReader::new(stdout),
            next_id: 1,
        }
    }

    fn send(&mut self, body: &Value) {
        let text = body.to_string();
        write!(self.stdin, "Content-Length: {}\r\n\r\n{}", text.len(), text).expect("write");
        self.stdin.flush().expect("flush");
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        loop {
            let msg = self.read_message().expect("a message");
            if msg["id"].as_u64() == Some(id) {
                return msg;
            }
            // Notifications interleaved (publishDiagnostics) — skip.
        }
    }

    fn read_message(&mut self) -> Option<Value> {
        let mut header = String::new();
        // Read until the blank line.
        loop {
            let mut line = String::new();
            self.reader.read_line(&mut line).ok()?;
            if line == "\r\n" || line == "\n" {
                break;
            }
            header.push_str(&line);
        }
        let length: usize = header
            .lines()
            .find_map(|l| l.strip_prefix("Content-Length:"))?
            .trim()
            .parse()
            .ok()?;
        let mut body = vec![0u8; length];
        self.reader.read_exact(&mut body).ok()?;
        serde_json::from_slice(&body).ok()
    }

    /// Read notifications until one matches `method` (bounded).
    fn wait_notification(&mut self, method: &str) -> Value {
        for _ in 0..16 {
            let msg = self.read_message().expect("a message");
            if msg["method"].as_str() == Some(method) {
                return msg;
            }
        }
        panic!("no {method} notification arrived");
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

const GOOD_DOC: &str = r#"grammar Smoke version "1.0.0" edition 2 {
  # digit docs
  digit = %x30-39
  pair = digit [digit]
  entry main: pair

  test {
    accept "42"
  }
}
"#;

const BROKEN_DOC: &str = r#"grammar Smoke version "1.0.0" edition 2 {
  digit = %x30-39
  pair = digit ??? [[[
  entry main: pair
}
"#;

#[test]
fn the_scripted_editor_session() {
    let mut c = Client::spawn();

    // 1. initialize: the capabilities reply.
    let init = c.request(
        "initialize",
        json!({"capabilities": {}, "rootUri": "file:///tmp"}),
    );
    assert_eq!(init["result"]["serverInfo"]["name"], "parsanol-lsp");
    assert_eq!(init["result"]["capabilities"]["hoverProvider"], true);
    c.send(&json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));

    // 2. didOpen the good doc: diagnostics arrive and are EMPTY-ish
    //    (a valid grammar compiles: no error diagnostics).
    let uri = "file:///tmp/smoke.parg";
    c.send(&json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": { "textDocument": { "uri": uri, "text": GOOD_DOC } }
    }));
    let diags = c.wait_notification("textDocument/publishDiagnostics");
    let items = diags["params"]["diagnostics"]
        .as_array()
        .expect("diagnostics");
    assert!(
        items.is_empty(),
        "the valid grammar carries no error diagnostics: {items:?}"
    );

    // 3. hover over `digit` (inside `pair = digit [digit]`).
    let hover_at = GOOD_DOC.find("pair = digit").unwrap() + "pair = ".len();
    let hover = c.request(
        "textDocument/hover",
        json!({
            "textDocument": { "uri": uri },
            "position": line_char(GOOD_DOC, hover_at),
        }),
    );
    let hv = hover["result"]["contents"]["value"]
        .as_str()
        .expect("hover");
    assert!(hv.contains("digit = %x30-39"), "the signature: {hv}");
    assert!(hv.contains("digit docs"), "the docs block: {hv}");

    // 4. go-to-definition from the reference.
    let def = c.request(
        "textDocument/definition",
        json!({
            "textDocument": { "uri": uri },
            "position": line_char(GOOD_DOC, hover_at),
        }),
    );
    let dr = &def["result"]["range"];
    assert!(dr.is_object(), "a definition range: {def}");
    assert_eq!(dr["start"]["line"], 2, "the digit def sits on line 2");

    // 5. find-references on the DEFINITION of digit.
    let def_at = GOOD_DOC.find("digit = %x30-39").unwrap();
    let refs = c.request(
        "textDocument/references",
        json!({
            "textDocument": { "uri": uri },
            "position": line_char(GOOD_DOC, def_at),
            "context": { "includeDeclaration": true },
        }),
    );
    let ritems = refs["result"].as_array().expect("references");
    assert!(ritems.len() >= 3, "digit: def + 2 uses: {ritems:?}");

    // 6. completion at `dig|`-ish position: the rule names ride.
    let comp = c.request(
        "textDocument/completion",
        json!({
            "textDocument": { "uri": uri },
            "position": line_char(GOOD_DOC, hover_at),
        }),
    );
    let labels: Vec<&str> = comp["result"]["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter_map(|i| i["label"].as_str())
        .collect();
    // The prefix `digit` filters: only the matching rule rides.
    assert_eq!(
        labels,
        vec!["digit"],
        "prefix-filtered completions: {labels:?}"
    );

    // An empty prefix offers every rule.
    let blank = GOOD_DOC.find("entry main:").unwrap() + "entry main:".len();
    let comp_all = c.request(
        "textDocument/completion",
        json!({
            "textDocument": { "uri": uri },
            "position": line_char(GOOD_DOC, blank),
        }),
    );
    let all_labels: Vec<&str> = comp_all["result"]["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter_map(|i| i["label"].as_str())
        .collect();
    assert!(all_labels.contains(&"digit"));
    assert!(all_labels.contains(&"pair"));
    assert!(
        all_labels.contains(&"accept"),
        "keywords ride: {all_labels:?}"
    );

    // 7. document symbols: the rule list.
    let syms = c.request(
        "textDocument/documentSymbol",
        json!({ "textDocument": { "uri": uri } }),
    );
    let snames: Vec<&str> = syms["result"]
        .as_array()
        .expect("symbols")
        .iter()
        .filter_map(|s| s["name"].as_str())
        .collect();
    assert!(snames.contains(&"digit"));
    assert!(snames.contains(&"pair"));

    // 8. the parse-tree preview: the document's own structure tree.
    let preview = c.request(
        "parg/treePreview",
        json!({ "textDocument": { "uri": uri } }),
    );
    assert_eq!(preview["result"]["ok"], true, "{}", preview);
    let pv = preview["result"]["preview"].as_str().expect("preview");
    assert!(pv.contains("digit"), "the tree names rules: {pv}");

    // 9. didChange to the BROKEN doc: diagnostics carry the ParseWire
    //    position + the expected labels.
    c.send(&json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didChange",
        "params": {
            "textDocument": { "uri": uri },
            "contentChanges": [{ "text": BROKEN_DOC }]
        }
    }));
    let bad = c.wait_notification("textDocument/publishDiagnostics");
    let bitems = bad["params"]["diagnostics"]
        .as_array()
        .expect("diagnostics");
    assert!(!bitems.is_empty(), "the broken grammar carries diagnostics");
    let first = &bitems[0];
    assert_eq!(first["severity"], 1);
    assert!(
        first["message"]
            .as_str()
            .is_some_and(|m| m.contains("grammar does not parse")),
        "the failure wire message: {first}"
    );

    // 10. shutdown + exit.
    let _ = c.request("shutdown", json!(null));
    c.send(&json!({"jsonrpc": "2.0", "method": "exit", "params": null}));
    let status = c.child.wait().expect("exit");
    assert!(status.success() || status.code() == Some(0), "{status}");
}

fn line_char(text: &str, offset: usize) -> Value {
    let before = &text[..offset.min(text.len())];
    let line = before.matches('\n').count();
    let col = before.len() - before.rfind('\n').map(|p| p + 1).unwrap_or(0);
    json!({ "line": line, "character": col })
}
