//! Lowers a compiled artifact to decoder-facing grammar forms
//! (ruby#321, ruby#460): XGrammar's EBNF dialect and llguidance's Lark
//! subset. **Rust is the authoritative implementation** (owner
//! directive, 2026-11-12): the wire format is language-neutral JSON,
//! so every runtime lowers identically — Ruby's `Parsanol::PARG::Export`
//! is parity-verified byte-for-byte against this module over the whole
//! corpus (tests/export_parity.rs), and wasm/C-ABI expose the same
//! entry points so TypeScript and Python inherit the lowering without
//! a per-language port.
//!
//! The supported subset lowers 1:1. Everything the target cannot
//! express — lookaround, behind-guards, custom atoms, fold groups —
//! is rejected NAMING the construct: the importer philosophy in
//! reverse, never silently dropped.

use serde_json::Value;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use super::{PargError, SUPPORTED_SHAPE};

/// The constrained-decoding export targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportTarget {
    /// XGrammar's EBNF dialect: `name ::= body`, native `{m,n}`.
    Xgrammar,
    /// llguidance's Lark subset: `name: body`, enters via `start`,
    /// bounded repetitions expand mechanically.
    Llguidance,
}

/// An export failure names the unsupported construct — never silently
/// dropped (the importer philosophy in reverse).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportError {
    /// The full dialect-prefixed message ("xgrammar export: … has no
    /// XGrammar equivalent"); the parity fixture pins it byte-exact.
    pub message: String,
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ExportError {}

/// The bounded-repetition expansion cap for targets without native
/// `{m,n}`: exact copies plus optional copies, rejected loudly past
/// the cap so a pathological bound fails at export time.
pub const MAX_EXPANSION: usize = 64;

impl super::PargArtifact {
    /// Lower the verified artifact to the target's grammar form. `recovery`
    /// lowers each declared `resync <rule> = <pattern>` as an escape
    /// alternative at the repetition spine (ruby#321's partial-mask
    /// story): clean parses, and violate-then-resync parses where the
    /// junk is emitted deliberately and the grammar closes back over
    /// the continuation.
    pub fn export(&self, target: ExportTarget, recovery: bool) -> Result<String, ExportError> {
        export_envelope(&self.envelope, target, recovery)
    }

    /// XGrammar shorthand.
    pub fn export_xgrammar(&self, recovery: bool) -> Result<String, ExportError> {
        self.export(ExportTarget::Xgrammar, recovery)
    }

    /// llguidance shorthand.
    pub fn export_llguidance(&self, recovery: bool) -> Result<String, ExportError> {
        self.export(ExportTarget::Llguidance, recovery)
    }
}

/// The envelope-level primitive: the lowering is a pure function of
/// the envelope (the parity fixture replays hand-built envelopes the
/// same way). Artifact-level entry points verify first; this level is
/// also what the wasm/C-ABI surface exposes.
pub fn export_envelope(
    envelope: &Value,
    target: ExportTarget,
    recovery: bool,
) -> Result<String, ExportError> {
    Lowering {
        envelope,
        target,
        recovery,
        memo: RefCell::new(HashMap::new()),
        in_progress: RefCell::new(HashSet::new()),
    }
    .run()
}

/// The atom walk the two targets share: literals, classes, sequences,
/// alternations, repetitions, and the ASCII-fold expansion. The target
/// supplies the dialect: how one entry renders, how repetitions spell,
/// and the message prefix for unsupported constructs.
struct Lowering<'a> {
    envelope: &'a Value,
    target: ExportTarget,
    recovery: bool,
    // The atom graph is a DAG with shared subtrees: a naive recursive
    // walk re-expands every shared reference along every path —
    // exponential on heavily-shared artifacts. A lowered atom is a
    // pure function of (atoms, index), so the cache makes the walk
    // linear (byte-identical output). Recursive grammars (the
    // self-hosted parg meta-grammar) are rejected NAMING the re-entry:
    // an inline EBNF unfold is infinite in principle. The parity
    // fixture pins both behaviors byte-exact against the Ruby oracle.
    memo: RefCell<HashMap<(usize, usize), String>>,
    in_progress: RefCell<HashSet<(usize, usize)>>,
}

impl<'a> Lowering<'a> {
    fn run(self) -> Result<String, ExportError> {
        let entries = self
            .envelope
            .get("entries")
            .and_then(Value::as_object)
            .ok_or_else(|| ExportError {
                message: "the artifact carries no entries".into(),
            })?;
        let mut rules = Vec::with_capacity(entries.len());
        for (entry, meta) in entries {
            let grammar = meta.get("grammar").ok_or_else(|| ExportError {
                message: format!("entry {entry}: no grammar section"),
            })?;
            let body = self.lower_entry(entry, grammar)?;
            rules.push(self.render_entry(entry, &body));
        }
        let mut out = String::new();
        out.push_str(&self.prologue()?);
        out.push_str(&rules.join("\n"));
        out.push('\n');
        Ok(out)
    }

    fn prologue(&self) -> Result<String, ExportError> {
        match self.target {
            ExportTarget::Xgrammar => Ok(String::new()),
            // Ruby fetches the key (KeyError when absent); the parity
            // fixture pins the message byte-exact.
            ExportTarget::Llguidance => {
                let entry = self
                    .envelope
                    .get("default_entry")
                    .and_then(Value::as_str)
                    .ok_or_else(|| ExportError {
                        message: "key not found: \"default_entry\"".into(),
                    })?;
                Ok(format!("start: {entry}_entry\n"))
            }
        }
    }

    fn render_entry(&self, entry: &str, body: &str) -> String {
        match self.target {
            ExportTarget::Xgrammar => format!("{entry}_entry ::= {body}"),
            ExportTarget::Llguidance => format!("{entry}_entry: {body}"),
        }
    }

    fn label(&self) -> &'static str {
        match self.target {
            ExportTarget::Xgrammar => "XGrammar",
            ExportTarget::Llguidance => "llguidance",
        }
    }

    // The Ruby lowering's Error message spells the full dialect label
    // ("xgrammar export: … has no XGrammar equivalent"); the parity
    // fixture pins the message byte-exact.
    fn dialect(&self) -> &'static str {
        match self.target {
            ExportTarget::Xgrammar => "xgrammar export",
            ExportTarget::Llguidance => "llguidance export",
        }
    }

    fn unsupported(&self, what: &str) -> ExportError {
        ExportError {
            message: format!(
                "{}: {what} has no {} equivalent",
                self.dialect(),
                self.label()
            ),
        }
    }

    // The recovery wrap mirrors the engines' resync contract: the root
    // must be a repetition spine (the same shape resync recovery itself
    // requires), and each declared resync pattern becomes an
    // alternative of the spine body — junk between complete iterations
    // is exactly what the recovery path consumes and resumes. Inferred
    // (undeclared) policies are the trial scan and have no closed form
    // here: the plain grammar ships.
    fn lower_entry(&self, entry: &str, grammar: &Value) -> Result<String, ExportError> {
        let atoms = grammar
            .get("atoms")
            .and_then(Value::as_array)
            .ok_or_else(|| ExportError {
                message: format!("entry {entry}: no atoms array"),
            })?;
        let root = grammar
            .get("root")
            .and_then(Value::as_u64)
            .ok_or_else(|| ExportError {
                message: format!("entry {entry}: no root index"),
            })? as usize;
        let body = self.lower(atoms, root)?;
        if !self.recovery {
            return Ok(body);
        }
        let Some(pattern) = self
            .envelope
            .get("entries")
            .and_then(|e| e.get(entry))
            .and_then(|e| e.get("resync"))
        else {
            return Ok(body);
        };
        if self.repetition_spine(atoms, root).is_none() {
            return Err(
                self.unsupported("recovery export: the entry root is not a repetition spine")
            );
        }
        let pattern_atoms = pattern
            .get("atoms")
            .and_then(Value::as_array)
            .ok_or_else(|| ExportError {
                message: format!("entry {entry}: the resync pattern carries no atoms"),
            })?;
        let pattern_root =
            pattern
                .get("root")
                .and_then(Value::as_u64)
                .ok_or_else(|| ExportError {
                    message: format!("entry {entry}: the resync pattern carries no root"),
                })? as usize;
        let junk = self.lower(pattern_atoms, pattern_root)?;
        // A spine item is a body, optionally preceded by one junk span:
        // the engines scan forward to the first pattern match, consume
        // it as one annotated span, and resume with the next iteration
        // — which may be absent (trailing junk ends the parse,
        // recovered).
        Ok(format!("( {body} | ( {junk} ) ( {body} )? )*"))
    }

    // The lowerable body of the spine's repetition: Named/Entity
    // unwrapping down to the Repetition atom, unbounded (max null).
    // None when the root is not that shape.
    fn repetition_spine(&self, atoms: &[Value], index: usize) -> Option<usize> {
        let atom = atoms.get(index)?;
        let (tag, fields) = atom.as_object()?.iter().next()?;
        match tag.as_str() {
            "Named" | "Entity" => {
                let inner = fields.get("atom")?.as_u64()? as usize;
                self.repetition_spine(atoms, inner)
            }
            "Repetition" => {
                let unbounded = fields.get("max").is_none_or(Value::is_null);
                if !unbounded {
                    return None;
                }
                fields.get("atom")?.as_u64().map(|a| a as usize)
            }
            _ => None,
        }
    }

    fn lower(&self, atoms: &[Value], index: usize) -> Result<String, ExportError> {
        let key = (atoms.as_ptr() as usize, index);
        if self.in_progress.borrow().contains(&key) {
            return Err(ExportError {
                message: format!(
                    "{}: recursive grammar: atom {index} re-enters while being lowered \
                     (inline EBNF cannot carry recursive rules)",
                    self.dialect()
                ),
            });
        }
        if let Some(hit) = self.memo.borrow().get(&key) {
            return Ok(hit.clone());
        }
        self.in_progress.borrow_mut().insert(key);
        let out = self.lower_uncached(atoms, index);
        self.in_progress.borrow_mut().remove(&key);
        let out = out?;
        self.memo.borrow_mut().insert(key, out.clone());
        Ok(out)
    }

    fn lower_uncached(&self, atoms: &[Value], index: usize) -> Result<String, ExportError> {
        let atom = atoms.get(index).ok_or_else(|| ExportError {
            message: format!("atom index {index} out of range"),
        })?;
        let (tag, fields) = atom
            .as_object()
            .and_then(|o| o.iter().next())
            .ok_or_else(|| ExportError {
                message: format!("atom {index}: empty envelope (expected a single-tag object)"),
            })?;
        match tag.as_str() {
            "Str" => {
                let pattern = fields.get("pattern").and_then(Value::as_str).unwrap_or("");
                Ok(format!("\"{}\"", escape_literal(pattern)))
            }
            "Re" => {
                let pattern = fields.get("pattern").and_then(Value::as_str).unwrap_or("");
                self.re_class(pattern)
            }
            "Sequence" => {
                let children = children(fields)?;
                let parts = children
                    .iter()
                    .map(|&c| self.lower(atoms, c))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(parts.join(" "))
            }
            "Alternative" => {
                let children = children(fields)?;
                let parts = children
                    .iter()
                    .map(|&c| self.lower(atoms, c))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(format!("( {} )", parts.join(" | ")))
            }
            "Repetition" => {
                let child =
                    fields
                        .get("atom")
                        .and_then(Value::as_u64)
                        .ok_or_else(|| ExportError {
                            message: format!("atom {index}: repetition without a body"),
                        })? as usize;
                let min = fields.get("min").and_then(Value::as_u64).unwrap_or(0) as usize;
                let max = fields
                    .get("max")
                    .and_then(Value::as_u64)
                    .map(|m| m as usize);
                let body = self.lower(atoms, child)?;
                self.suffix(&body, min, max)
            }
            "Maybe" => {
                let child =
                    fields
                        .get("atom")
                        .and_then(Value::as_u64)
                        .ok_or_else(|| ExportError {
                            message: format!("atom {index}: maybe without a body"),
                        })? as usize;
                let body = self.lower(atoms, child)?;
                self.suffix(&body, 0, Some(1))
            }
            "Named" | "Entity" => {
                let child =
                    fields
                        .get("atom")
                        .and_then(Value::as_u64)
                        .ok_or_else(|| ExportError {
                            message: format!("atom {index}: {tag} without a body"),
                        })? as usize;
                Ok(format!("( {} )", self.lower(atoms, child)?))
            }
            "Lookahead" => Err(self.unsupported("lookahead (!x / &x)")),
            "BehindGuard" => Err(self.unsupported("behind-guard (!<x / &<x)")),
            "Custom" => Err(self.unsupported("custom atom")),
            "Constant" => Err(self.unsupported("constant atom")),
            other => Err(self.unsupported(&format!("atom {other}"))),
        }
    }

    // Bounded repetition {m,n}: XGrammar spells it natively; llguidance
    // expands mechanically (exact copies, then optional copies up to
    // the bound, or a `*` tail when unbounded) — exact, never
    // approximated, and rejected loudly past the expansion cap so a
    // pathological bound fails at export time.
    fn suffix(&self, body: &str, min: usize, max: Option<usize>) -> Result<String, ExportError> {
        if min == 0 && max.is_none() {
            return Ok(format!("{}*", wrap(body)));
        }
        if min == 1 && max.is_none() {
            return Ok(format!("{}+", wrap(body)));
        }
        if min == 0 && max == Some(1) {
            return Ok(format!("{}?", wrap(body)));
        }
        if self.target == ExportTarget::Xgrammar {
            return Ok(format!(
                "{}{{{},{}}}",
                wrap(body),
                min,
                max.map_or(String::new(), |m| m.to_string())
            ));
        }
        self.expand(body, min, max)
    }

    fn expand(&self, body: &str, min: usize, max: Option<usize>) -> Result<String, ExportError> {
        let cap = max.unwrap_or(min);
        if cap > MAX_EXPANSION {
            return Err(self.unsupported(&format!(
                "bounded repetition {{{min},{}}} exceeds the {MAX_EXPANSION}-item expansion cap",
                max.map_or("n".to_string(), |m| m.to_string())
            )));
        }
        let mut parts = vec![wrap(body); min];
        match max {
            None => {
                if min > 0 {
                    parts.push(format!("{}*", wrap(body)));
                }
            }
            Some(max) => {
                parts.extend(std::iter::repeat_n(format!("{}?", wrap(body)), max - min));
            }
        }
        Ok(parts.join(" "))
    }

    // A Re pattern the class form cannot carry is unsupported rather
    // than approximated — EXCEPT the ASCII fold literal: (?i:...)
    // expands mechanically to per-character [Xx] classes, which IS the
    // simple-case-fold semantics for ASCII. A non-ASCII fold group
    // needs a fold table the target does not have.
    fn re_class(&self, pattern: &str) -> Result<String, ExportError> {
        // Categories have no EBNF-class spelling: they must be rejected
        // naming the construct, never emitted raw (the target's lexer
        // rejects them, which fails the round trip obscurely instead of
        // at export time).
        if pattern.contains("\\p{") {
            return Err(self.unsupported(&format!("unicode category {pattern:?}")));
        }
        if pattern.starts_with('[') && pattern.ends_with(']') {
            return de_escape_class(pattern);
        }
        let folded = pattern
            .strip_prefix("(?i:")
            .and_then(|rest| rest.strip_suffix(')'));
        if let Some(folded) = folded {
            return self.expand_fold(folded, pattern);
        }
        Err(self.unsupported(&format!("regex pattern {pattern:?}")))
    }

    fn expand_fold(&self, text: &str, pattern: &str) -> Result<String, ExportError> {
        if !text.is_ascii() {
            return Err(self.unsupported(&format!(
                "non-ASCII fold group {pattern:?} -- the target has no fold table"
            )));
        }
        let mut out = String::with_capacity(text.len() * 2);
        for ch in text.chars() {
            if ch.is_ascii_alphabetic() {
                out.push_str(&format!(
                    "[{}{}]",
                    ch.to_ascii_uppercase(),
                    ch.to_ascii_lowercase()
                ));
            } else if ch == '"' {
                out.push_str("\\\"");
            } else {
                out.push(ch);
            }
        }
        Ok(out)
    }
}

fn children(fields: &Value) -> Result<Vec<usize>, ExportError> {
    fields
        .get("atoms")
        .and_then(Value::as_array)
        .and_then(|atoms| {
            atoms
                .iter()
                .map(|a| a.as_u64().map(|v| v as usize))
                .collect::<Option<Vec<_>>>()
        })
        .ok_or_else(|| ExportError {
            message: "a sequence/alternative without an atom index list".into(),
        })
}

fn wrap(body: &str) -> String {
    let bare = (body.starts_with('"')
        && body.ends_with('"')
        && body.len() > 1
        && !body[1..body.len() - 1].contains('"'))
        || (body.starts_with('[')
            && body.ends_with(']')
            && body.len() > 1
            && !body[1..body.len() - 1].contains(']'))
        || (!body.is_empty()
            && body
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && body.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
    if bare {
        body.to_string()
    } else {
        format!("( {body} )")
    }
}

// The Ruby lowering escapes, byte-exact (the parity fixture is the
// oracle): a backslash becomes FOUR backslashes (`\` → `\\\\` — the
// target lexer reads it as one literal backslash through two escapes),
// a quote becomes backslash-backslash-backslash-quote (`"` → `\\\"`).
fn escape_literal(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len());
    for ch in pattern.chars() {
        match ch {
            '\\' => out.push_str("\\\\\\\\"),
            '"' => out.push_str("\\\\\\\""),
            other => out.push(other),
        }
    }
    out
}

// Class endpoints ÿ / \u{10ffff} spell as the literal characters — the
// targets' classes are UTF-8 text and accept them; emitting the
// escapes raw fails the target's lexer instead. A codepoint the UTF-8
// class text cannot carry (out of range, surrogate) rejects exactly as
// the Ruby lowering's pack("U") does — the fixture pins the message.
fn de_escape_class(pattern: &str) -> Result<String, ExportError> {
    // \u{hex} and \uXXXX forms.
    let mut out = String::with_capacity(pattern.len());
    let mut chars = pattern.char_indices().peekable();
    while let Some((idx, ch)) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        let rest = &pattern[idx + 1..];
        if let Some(rest) = rest.strip_prefix('u') {
            if let Some(rest) = rest.strip_prefix('{') {
                if let Some(close) = rest.find('}') {
                    if let Ok(code) = u32::from_str_radix(rest[..close].trim(), 16) {
                        match char::from_u32(code) {
                            Some(decoded) => {
                                out.push(decoded);
                                // The iterator sits after `\`; the full
                                // spelling is `u{` + hex + `}`.
                                for _ in 0..close + 3 {
                                    chars.next();
                                }
                                continue;
                            }
                            None => {
                                return Err(ExportError {
                                    message: "invalid byte sequence in UTF-8".into(),
                                })
                            }
                        }
                    }
                }
            } else if rest.len() >= 4 && rest[..4].chars().all(|c| c.is_ascii_hexdigit()) {
                if let Ok(code) = u32::from_str_radix(&rest[..4], 16) {
                    match char::from_u32(code) {
                        Some(decoded) => {
                            out.push(decoded);
                            // The iterator sits after `\`; the full
                            // spelling is `u` + 4 hex digits.
                            for _ in 0..5 {
                                chars.next();
                            }
                            continue;
                        }
                        None => {
                            return Err(ExportError {
                                message: "invalid byte sequence in UTF-8".into(),
                            })
                        }
                    }
                }
            }
        }
        out.push(ch);
    }
    Ok(out)
}

/// Validate the envelope shape an export consumes (the wire the
/// lowering reads): shape tag, entries map, atom arrays.
pub fn validate_exportable(envelope: &Value) -> Result<(), PargError> {
    let shape = envelope
        .get("shape")
        .and_then(Value::as_str)
        .ok_or(PargError::InvalidEnvelope("shape"))?;
    if shape != SUPPORTED_SHAPE {
        return Err(PargError::UnsupportedShape(shape.to_string()));
    }
    if !envelope.get("entries").is_some_and(Value::is_object) {
        return Err(PargError::InvalidEnvelope("entries"));
    }
    Ok(())
}
