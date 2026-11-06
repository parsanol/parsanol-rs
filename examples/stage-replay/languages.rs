//! The per-language replay configuration of the stage gems whose
//! plurimath-testsuite generations carry raw-tree rows (plurimath#540).
//!
//! Each stage gem writes its raw tree from the Ruby engine's parse result,
//! after a language-specific step this module ports:
//!
//!   latex      latexmath Latexmath::RawParser: the declared bindings'
//!              latex_tree capture, with CAPTURE_RENAMES (row_break ->
//!              "\\\\") (scripts/stage_cases.rb).
//!   asciimath  amath Amath.parse: the bare parse (no bindings), its root
//!              capture, then Amath::Renester rebuilding the nested tree of
//!              each flattened expression chain (lib/amath/parser.rb,
//!              lib/amath/renester.rb).
//!   unicode    umath Umath::RawParser: Umath::Preprocessor on the caller
//!              input (lib/umath/preprocessor.rb), the declared bindings'
//!              umath_tree capture, then scripts/unwrap.rb removing the
//!              wrapper captures listed in grammars/umath.wrappers.json.
//!
//! The ports work on the flattened parsanol-tree/v2 value: objects, arrays,
//! strings, null, and InputRef leaves standing for Parsanol::Slice.

#![allow(dead_code)]

use crate::replay::{flatten_shape, is_input_ref};
use serde_json::{json, Map, Value};
use std::collections::HashMap;

/// What a replay needs to know about one language.
pub struct StageLanguage {
    pub name: &'static str,
    /// The capture of the bound (or bare) tree that holds the raw tree.
    pub capture: &'static str,
    /// Apply the declared bindings artifact before taking the capture.
    pub bindings: bool,
    /// Decimal marker -> artifact entry (the gem's DECIMAL_ENTRIES).
    pub decimal_entries: &'static [(&'static str, &'static str)],
    /// PARG capture -> legacy key, written as a String key.
    pub renames: &'static [(&'static str, &'static str)],
    /// The language's transform reads the wrapper table.
    pub wrappers: bool,
    /// The gem's preprocessing of the caller input into the grammar input,
    /// for a language whose refusal rows record the offset in that text
    /// (None when the input has no formula part).
    pub preprocess: Option<fn(&str) -> Option<String>>,
    /// The gem's step from the captured tree to the written raw tree.
    pub transform: Transform,
}

/// The step from the captured tree to the written raw tree.
pub enum Transform {
    None,
    Renest,
    Unwrap,
}

const DECIMAL_ENTRIES: &[(&str, &str)] = &[(".", "dot"), (",", "comma"), ("\u{66b}", "arabic")];

pub const LANGUAGES: &[StageLanguage] = &[
    StageLanguage {
        name: "latex",
        capture: "latex_tree",
        bindings: true,
        decimal_entries: DECIMAL_ENTRIES,
        renames: &[("row_break", "\\\\")],
        wrappers: false,
        preprocess: None,
        transform: Transform::None,
    },
    StageLanguage {
        name: "asciimath",
        capture: "root",
        bindings: false,
        decimal_entries: DECIMAL_ENTRIES,
        renames: &[],
        wrappers: false,
        preprocess: None,
        transform: Transform::Renest,
    },
    StageLanguage {
        name: "unicode",
        capture: "umath_tree",
        bindings: true,
        decimal_entries: DECIMAL_ENTRIES,
        renames: &[],
        wrappers: true,
        preprocess: Some(umath_preprocess),
        transform: Transform::Unwrap,
    },
];

/// The language configuration named `name`.
pub fn stage_language(name: &str) -> &'static StageLanguage {
    LANGUAGES
        .iter()
        .find(|l| l.name == name)
        .unwrap_or_else(|| {
            let known: Vec<&str> = LANGUAGES.iter().map(|l| l.name).collect();
            panic!("unknown language {name} (known: {})", known.join(", "))
        })
}

impl StageLanguage {
    /// The written raw tree for `tree`, the captured value.
    pub fn apply(
        &self,
        tree: &Value,
        source: &str,
        wrappers: Option<&HashMap<String, String>>,
    ) -> Value {
        match self.transform {
            Transform::None => tree.clone(),
            Transform::Renest => renest(tree, source),
            Transform::Unwrap => unwrap(
                tree,
                wrappers.expect("unicode replay needs the wrapper table"),
                source,
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// Value kinds, as Ruby sees them: Hash, Array, String, Parsanol::Slice.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Hash,
    Array,
    String,
    Slice,
    Nil,
    Other,
}

fn kind(value: &Value) -> Kind {
    match value {
        Value::Null => Kind::Nil,
        Value::Array(_) => Kind::Array,
        Value::String(_) => Kind::String,
        Value::Object(m) if is_input_ref(m) => Kind::Slice,
        Value::Object(_) => Kind::Hash,
        _ => Kind::Other,
    }
}

fn is_hash(value: &Value) -> bool {
    kind(value) == Kind::Hash
}

fn is_text(k: Kind) -> bool {
    matches!(k, Kind::String | Kind::Slice)
}

/// The text of a string or slice (its value, else its span of `source`,
/// read as `dump_canonical` reads it).
fn text(value: &Value, source: &str) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Object(m) if is_input_ref(m) => match m.get("value").and_then(Value::as_str) {
            Some(text) => text.to_string(),
            None => {
                let offset = m["offset"].as_u64().unwrap() as usize;
                let length = m["length"].as_u64().unwrap() as usize;
                source[offset..].chars().take(length).collect()
            }
        },
        other => panic!("no text for {other}"),
    }
}

/// Parsanol::Slice.new(offset, content): a slice at `offset` with `text`.
fn slice(offset: &Value, text: String) -> Value {
    json!({"offset": offset, "length": text.chars().count(), "value": text})
}

/// `left + right` (Hash#merge for hashes) for two values of one Ruby class.
fn plus(left: &Value, right: &Value, k: Kind, source: &str) -> Value {
    match k {
        Kind::Hash => {
            let mut merged = left.as_object().unwrap().clone();
            merged.extend(right.as_object().unwrap().clone());
            Value::Object(merged)
        }
        Kind::Array => {
            let mut items = left.as_array().unwrap().clone();
            items.extend(right.as_array().unwrap().iter().cloned());
            Value::Array(items)
        }
        Kind::String => Value::String(text(left, source) + &text(right, source)),
        // Parsanol::Slice#+: the left slice's offset, the joined content.
        Kind::Slice => slice(&left["offset"], text(left, source) + &text(right, source)),
        other => panic!("cannot add two values of kind {other:?}"),
    }
}

/// Parsanol::Atoms::CanFlatten#merge_fold, which Amath::Renester#merge
/// also follows (without the duplicate-key warning).
fn merge_fold(left: &Value, right: &Value, source: &str) -> Value {
    let (l, r) = (kind(left), kind(right));
    if l == r {
        return plus(left, right, l, source);
    }
    if is_text(l) && is_text(r) {
        return if r == Kind::Slice {
            right.clone()
        } else {
            left.clone()
        };
    }
    if is_text(r) {
        return left.clone();
    }
    if is_text(l) {
        return right.clone();
    }
    if r == Kind::Hash && l == Kind::Array {
        let mut items = left.as_array().unwrap().clone();
        items.push(right.clone());
        return Value::Array(items);
    }
    if l == Kind::Hash && r == Kind::Array {
        let mut items = vec![left.clone()];
        items.extend(right.as_array().unwrap().iter().cloned());
        return Value::Array(items);
    }
    panic!("Unhandled case when foldr'ing sequence ({l:?}, {r:?}).")
}

// ---------------------------------------------------------------------------
// asciimath: Amath::Renester (lib/amath/renester.rb at the pinned commit).

const CHAIN: &str = "amath_chain";
const FINAL: &str = "amath_end";

fn level(key: &str) -> Option<&'static str> {
    match key {
        "amath_step" => Some("expr"),
        "amath_lr" => Some("left_right"),
        _ => None,
    }
}

/// `tree` with every chain node rebuilt into the nested tree.
pub fn renest(tree: &Value, source: &str) -> Value {
    fn walk(node: &Value, source: &str) -> Value {
        match node {
            Value::Array(items) => Value::Array(items.iter().map(|i| walk(i, source)).collect()),
            Value::Object(map) if !is_input_ref(map) => {
                let rebuilt: Map<String, Value> = map
                    .iter()
                    .map(|(k, v)| (k.clone(), walk(v, source)))
                    .collect();
                match rebuilt.get(CHAIN) {
                    Some(Value::Array(items)) => match renest_chain(items, source) {
                        Value::Object(nested) => nested.into_iter().next().unwrap().1,
                        _ => panic!("amath: a chain without a level"),
                    },
                    Some(other) => panic!("amath: a chain that is not a list: {other}"),
                    None => Value::Object(rebuilt),
                }
            }
            other => other.clone(),
        }
    }
    walk(&flatten_shape(tree), source)
}

fn renest_chain(items: &[Value], source: &str) -> Value {
    let mut tree = match items.last() {
        Some(Value::Object(last)) if !is_input_ref(last) && last.contains_key(FINAL) => {
            last[FINAL].clone()
        }
        _ => Value::String(String::new()),
    };
    for item in items.iter().rev() {
        let Value::Object(item) = item else { continue };
        if is_input_ref(item) {
            continue;
        }
        for (key, part) in item {
            if let Some(level) = level(key) {
                tree = json!({ level: fold(part, &tree, source) });
            }
        }
    }
    tree
}

fn fold(left: &Value, right: &Value, source: &str) -> Value {
    if left.is_null() {
        return if right.is_null() {
            Value::String(String::new())
        } else {
            right.clone()
        };
    }
    if right.is_null() {
        return left.clone();
    }
    merge_fold(left, right, source)
}

// ---------------------------------------------------------------------------
// unicode: umath scripts/unwrap.rb (UmathUnwrap) at the pinned commit, with
// Parsanol::Atoms::CanFlatten#merge_fold, #foldl and #flatten_repetition.

/// The legacy tree for `tree`, a raw tree of umath's normalized grammar.
pub fn unwrap(tree: &Value, table: &HashMap<String, String>, source: &str) -> Value {
    fn call(value: &Value, table: &HashMap<String, String>, source: &str) -> Value {
        match value {
            Value::Array(items) => {
                let items: Vec<Value> = items.iter().map(|v| call(v, table, source)).collect();
                flatten_repetition(&items, true, source)
            }
            Value::Object(map) if !is_input_ref(map) => unwrap_hash(map, table, source),
            other => other.clone(),
        }
    }
    fn unwrap_hash(
        map: &Map<String, Value>,
        table: &HashMap<String, String>,
        source: &str,
    ) -> Value {
        if !map.keys().any(|k| table.contains_key(k)) {
            return Value::Object(
                map.iter()
                    .map(|(k, v)| (k.clone(), call(v, table, source)))
                    .collect(),
            );
        }
        let mut parts: Vec<Value> = Vec::new();
        let mut run: Option<Map<String, Value>> = None;
        let mut only_nil = true;
        for (k, v) in map {
            match table.get(k) {
                Some(wrapper) => {
                    if let Some(r) = run.take() {
                        parts.push(Value::Object(r));
                    }
                    only_nil &= wrapper == "nil";
                    parts.push(part(wrapper, call(v, table, source)));
                }
                None => {
                    only_nil = false;
                    run.get_or_insert_with(Map::new)
                        .insert(k.clone(), call(v, table, source));
                }
            }
        }
        if let Some(r) = run {
            parts.push(Value::Object(r));
        }
        if only_nil {
            return Value::Null;
        }
        foldl(&parts, |l, r| merge_fold(l, r, source))
    }
    call(&flatten_shape(tree), table, source)
}

fn part(wrapper: &str, value: Value) -> Value {
    let empty = matches!(&value, Value::Array(a) if a.is_empty());
    match wrapper {
        "text" if value.is_null() || empty => Value::String(String::new()),
        "text" => value,
        "empty" => Value::String(String::new()),
        "list" if empty => Value::String(String::new()),
        "list" => value,
        "nil" => Value::Null,
        other => panic!("unknown wrapper kind {other}"),
    }
}

/// CanFlatten#foldl: "" for no element, else a left fold (nil elements
/// included, as Ruby passes them to the block).
fn foldl(list: &[Value], merge: impl Fn(&Value, &Value) -> Value) -> Value {
    let Some((first, rest)) = list.split_first() else {
        return Value::String(String::new());
    };
    rest.iter().fold(first.clone(), |acc, e| merge(&acc, e))
}

fn flatten_repetition(list: &[Value], named: bool, source: &str) -> Value {
    if list.iter().any(is_hash) {
        return Value::Array(list.iter().filter(|e| is_hash(e)).cloned().collect());
    }
    if list.iter().any(Value::is_array) {
        return Value::Array(
            list.iter()
                .filter_map(Value::as_array)
                .flat_map(|a| a.iter().cloned())
                .collect(),
        );
    }
    if named && list.is_empty() {
        return Value::Array(Vec::new());
    }
    // foldl(list.compact) { |s, e| s + e }: String#+ gives a String (a Slice
    // converts with to_str) and Slice#+ a Slice at its own offset, so the
    // first element decides the result's class.
    let present: Vec<&Value> = list.iter().filter(|e| !e.is_null()).collect();
    let Some(first) = present.first() else {
        return Value::String(String::new());
    };
    let content: String = present.iter().map(|e| text(e, source)).collect();
    match kind(first) {
        Kind::Slice => slice(&first["offset"], content),
        _ => Value::String(content),
    }
}

// ---------------------------------------------------------------------------
// unicode: Umath::Preprocessor.call(input).text (lib/umath/preprocessor.rb at
// the pinned commit), step for step.

const PEN_OPEN: &str = "\u{270e}(";
const PROTECTED_HASH: &str = "\"replacement\"";
const COMMENT_OPEN: &str = "&#x2af7;";
const COMMENT_CLOSE: &str = "&#x2af8;";

/// The grammar input umath parses for `input`, or None without a formula.
pub fn umath_preprocess(input: &str) -> Option<String> {
    let protected: Vec<String> = input.split('\n').map(protect_pen_group).collect();
    let protected = protected.join("\n");
    // Ruby String#split drops trailing empty fields.
    let mut parts: Vec<&str> = protected.split('#').collect();
    while parts.last() == Some(&"") {
        parts.pop();
    }
    let formula = parts.first()?.replace(PROTECTED_HASH, "#");
    let mut text = encode_entities(&formula);
    text = text.replace("&#x26;", "&");
    text = text.replace("&#x22;", "\"");
    text = remove_comment(&text);
    text = text.replace("\\\\", "\\");
    text = expand_u_escapes(&text);
    // Ruby String#strip; every character outside printable ASCII is encoded
    // by now, so only spaces can lead or trail.
    Some(text.trim_matches(' ').to_string())
}

fn protect_pen_group(line: &str) -> String {
    let Some(start) = line.find(PEN_OPEN) else {
        return line.to_string();
    };
    let Some(close) = line.rfind(')') else {
        return line.to_string();
    };
    if close < start + PEN_OPEN.len() {
        return line.to_string();
    }
    match line[start + PEN_OPEN.len()..].find('#') {
        Some(hash) if start + PEN_OPEN.len() + hash < close => format!(
            "{}{}{}",
            &line[..start],
            line[start..=close].replace('#', PROTECTED_HASH),
            &line[close + 1..]
        ),
        _ => line.to_string(),
    }
}

fn remove_comment(text: &str) -> String {
    let Some(start) = text.find(COMMENT_OPEN) else {
        return text.to_string();
    };
    match text.rfind(COMMENT_CLOSE) {
        Some(close) if close >= start + COMMENT_OPEN.len() => {
            format!("{}{}", &text[..start], &text[close + COMMENT_CLOSE.len()..])
        }
        _ => text.to_string(),
    }
}

fn encode_entities(text: &str) -> String {
    // The basic characters <>'"& first, then everything outside printable
    // ASCII; the first pass only writes printable ASCII.
    text.chars()
        .map(|c| {
            if matches!(c, '<' | '>' | '\'' | '"' | '&') || !(' '..='~').contains(&c) {
                format!("&#x{:x};", c as u32)
            } else {
                c.to_string()
            }
        })
        .collect()
}

/// gsub(/\\u([\da-fA-F]{1,5})\w{0,5}/) { "&#x#{$1};" } on ASCII text.
fn expand_u_escapes(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && bytes.get(i + 1) == Some(&b'u') {
            let hex_start = i + 2;
            let mut j = hex_start;
            while j < bytes.len() && j - hex_start < 5 && bytes[j].is_ascii_hexdigit() {
                j += 1;
            }
            if j > hex_start {
                let hex = &text[hex_start..j];
                let mut k = j;
                while k < bytes.len()
                    && k - j < 5
                    && (bytes[k].is_ascii_alphanumeric() || bytes[k] == b'_')
                {
                    k += 1;
                }
                out.push_str(&format!("&#x{hex};"));
                i = k;
                continue;
            }
        }
        let c = text[i..].chars().next().unwrap();
        out.push(c);
        i += c.len_utf8();
    }
    out
}
