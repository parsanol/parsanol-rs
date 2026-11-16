//! Suite runner: evaluates accept/reject/example/capture tests against
//! an artifact with the native portable engine plus the bindings
//! runtime. Port of `Parsanol::PARG::Artifact#run_tests` /
//! `#run_test_list` — failure descriptions are word-compatible with the
//! Ruby runner. `capture` is the grammar-layer phase (raw captures);
//! `example` the bindings layer (bound attribute hash).

use serde_json::Value;

use super::bindings::{is_leaf, leaf_scalar};
use super::{PargArtifact, PargError};
use crate::portable::ast::AstNode;
use crate::portable::capture_state::CaptureValue;
use crate::portable::grammar::{Atom, RepetitionTag};
use crate::portable::parslet_transform::to_parslet_compatible;
use crate::portable::{AstArena, Grammar, PortableParser};

// High-level artifact parsing must reach its rule-recursion guard on native
// callers whose OS/language thread may have a much smaller default stack.
// Results own their data; scoped workers are joined before borrowed input or
// artifacts can expire. Nested recovery parses reuse the same worker.
#[cfg(not(target_arch = "wasm32"))]
thread_local! { static ON_PARSER_STACK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
#[cfg(not(target_arch = "wasm32"))]
struct ParserStackScope;
#[cfg(not(target_arch = "wasm32"))]
impl Drop for ParserStackScope {
    fn drop(&mut self) {
        ON_PARSER_STACK.with(|flag| flag.set(false));
    }
}
pub(crate) fn with_parser_stack<T: Send>(
    operation: impl FnOnce() -> T + Send,
) -> Result<T, String> {
    #[cfg(target_arch = "wasm32")]
    {
        Ok(operation())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        if ON_PARSER_STACK.with(|flag| flag.get()) {
            return Ok(operation());
        }
        std::thread::scope(|scope| {
            std::thread::Builder::new()
                .name("parsanol-artifact".into())
                .stack_size(64 * 1024 * 1024)
                .spawn_scoped(scope, move || {
                    ON_PARSER_STACK.with(|flag| flag.set(true));
                    let _scope = ParserStackScope;
                    operation()
                })
                .map_err(|error| format!("parser worker unavailable: {error}"))?
                .join()
                .map_err(|_| "parser worker panicked".to_string())
        })
    }
}

/// The wire's line/column at a byte offset: line counts newlines,
/// column counts CHARACTERS since the last one — the guide's Slice
/// contract (offsets stay bytes; the Ruby engine's slice.rb is the
/// reference arithmetic).
fn line_column(input: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(input.len());
    let before = &input[..offset];
    let line = before.matches('\n').count() + 1;
    let tail = match before.rfind('\n') {
        Some(index) => &before[index + 1..],
        None => before,
    };
    (line, tail.chars().count() + 1)
}

pub(crate) fn ast_to_value(
    node: &crate::portable::ast::AstNode,
    arena: &AstArena,
    input: &str,
) -> Value {
    use crate::portable::ast::AstNode;
    match node {
        AstNode::Nil => Value::Null,
        AstNode::Bool(b) => Value::Bool(*b),
        AstNode::Int(n) => (*n).into(),
        AstNode::Float(f) => serde_json::Number::from_f64(*f)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        AstNode::StringRef { pool_index } => {
            Value::String(arena.get_string(*pool_index as usize).to_string())
        }
        AstNode::InputRef { offset, length } => {
            let offset = *offset as usize;
            let length = *length as usize;
            let value = input.get(offset..offset + length).unwrap_or_default();
            let (line, column) = line_column(input, offset);
            // Wire contract (Slice parity): offsets are byte positions,
            // line/column are character counts, length is the
            // character count (content.size).
            serde_json::json!({
                "value": value,
                "line": line,
                "column": column,
                "offset": offset,
                "length": value.chars().count(),
            })
        }
        AstNode::Array { pool_index, length } => {
            let items = arena.get_array(*pool_index as usize, *length as usize);
            Value::Array(
                items
                    .iter()
                    .map(|item| ast_to_value(item, arena, input))
                    .collect(),
            )
        }
        AstNode::Hash { pool_index, length } => {
            let pairs = arena.get_hash_items(*pool_index as usize, *length as usize);
            let mut object = serde_json::Map::new();
            for (key, value) in &pairs {
                object.insert(key.clone(), ast_to_value(value, arena, input));
            }
            Value::Object(object)
        }
    }
}

/// Canonical test-value form (parsanol-ruby#216): one equality surface
/// for capture and example expectations. Terminals reduce to their text
/// (native slice wire objects to their `value` field), structure
/// recurses. Value-compatible with Ruby's `Bindings.canonical`.
pub(crate) fn canonical(value: &Value) -> Value {
    match value {
        Value::Number(number) if number.is_f64() => {
            // B6 float canonical form: the Ruby JSON number text (see
            // ruby_json_float), so float expectations and bound floats
            // compare identically on every engine.
            Value::String(super::ruby_json_float(number.as_f64().unwrap()))
        }
        Value::Array(items) => Value::Array(items.iter().map(canonical).collect()),
        Value::Object(_) if is_leaf(value) => leaf_scalar(value).clone(),
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, item)| (key.clone(), canonical(item)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Wire rendering of one committed capture for the recovery envelope:
/// input-backed spans render as the standard slice wire objects, text
/// captures render bare, host-opaque handles render null (not
/// portable).
fn capture_wire(value: &CaptureValue, input: &str) -> Value {
    match value {
        CaptureValue::Span { offset, length } => {
            let offset = *offset;
            let length = *length;
            let text = input.get(offset..offset + length).unwrap_or_default();
            let (line, column) = line_column(input, offset);
            serde_json::json!({
                "value": text, "line": line, "column": column,
                "offset": offset, "length": text.chars().count()
            })
        }
        CaptureValue::Text(t) => Value::String(t.to_string()),
        CaptureValue::Opaque { .. } => Value::Null,
    }
}

/// Deep-copy an AST value from one arena into another: pool-backed
/// nodes (StringRef/Array/Hash) re-intern into the target; span-backed
/// and scalar nodes are arena-free and clone as-is. The recovery
/// rounds' probe values reference the probe parser's arena, but the
/// stash renders through the render arena — values must move across.
fn copy_ast_into_arena(node: &AstNode, source: &AstArena, target: &mut AstArena) -> AstNode {
    match node {
        AstNode::StringRef { pool_index } => {
            let text = source.get_string(*pool_index as usize).to_string();
            target.intern_string(&text)
        }
        AstNode::Array { pool_index, length } => {
            let items = source.get_array(*pool_index as usize, *length as usize);
            let copied: Vec<AstNode> = items
                .iter()
                .map(|item| copy_ast_into_arena(item, source, target))
                .collect();
            let (start, len) = target.store_array(&copied);
            AstNode::Array {
                pool_index: start,
                length: len,
            }
        }
        AstNode::Hash { pool_index, length } => {
            let pairs = source.get_hash_items(*pool_index as usize, *length as usize);
            let copied: Vec<(&str, AstNode)> = pairs
                .iter()
                .map(|(key, item)| (key.as_str(), copy_ast_into_arena(item, source, target)))
                .collect();
            let (start, len) = target.store_hash(&copied);
            AstNode::Hash {
                pool_index: start,
                length: len,
            }
        }
        other => other.clone(),
    }
}

/// Resync rounds per recovery (R2): the floor, plus one round per
/// kilobyte of input. Each round strictly advances the cursor, so the
/// total skip work stays linear in the input; the floor keeps tiny
/// documents' recovery bounded and the per-kilobyte slope keeps
/// heavily corrupted large documents (the 20-site harness rows) from
/// exhausting the budget mid-file.
const MAX_RESYNCS: usize = 8;

/// The recovery envelope for a failed parse: the captures committed
/// before the failure plus the failure wire. Captures of the failing
/// branch may be present — this is best-effort data, annotated as such
/// by the `ok: false` marker.
#[allow(clippy::too_many_arguments)]
fn recovery_shape(
    entry: &str,
    journal: &[(String, CaptureValue)],
    consumed_upto: usize,
    failure_wire: Option<(usize, Vec<String>)>,
    input: &str,
    arena: &mut AstArena,
    partial: Option<(usize, Vec<AstNode>, RepetitionTag)>,
    root_is_named: Option<String>,
    resyncs: Vec<Value>,
    recovered_spans: Vec<(u32, u32)>,
) -> Value {
    let _ = entry;
    let mut captures = serde_json::Map::new();
    for (name, value) in journal {
        // Journal order is capture order — an array per name preserves
        // the per-iteration narrative a rep of same-named captures
        // leaves behind.
        let slot = captures
            .entry(name.clone())
            .or_insert_with(|| Value::Array(Vec::new()));
        if let Value::Array(items) = slot {
            items.push(capture_wire(value, input));
        }
    }
    let error = match failure_wire {
        Some((offset, expected)) => {
            let (line, column) = line_column(input, offset);
            serde_json::json!({
                "offset": offset, "line": line, "column": column, "expected": expected
            })
        }
        None => serde_json::json!({ "offset": input.len(), "expected": [] }),
    };
    // R1.5: the deepest repetition prefix the failed parse matched,
    // shaped exactly like the root would have been (tagged array, Named
    // wrapper when the root rule is named). Null when nothing matched.
    let partial_value = partial.map(|(_end, items, tag)| {
        let tag_str = match tag {
            RepetitionTag::Maybe => ":maybe",
            RepetitionTag::Repetition => ":repetition",
        };
        let (pool, len) = arena.store_tagged_array(tag_str, &items);
        let mut node = AstNode::Array {
            pool_index: pool,
            length: len,
        };
        if let Some(name) = root_is_named {
            let (hp, hl) = arena.store_hash(&[(name.as_str(), node)]);
            node = AstNode::Hash {
                pool_index: hp,
                length: hl,
            };
        }
        let shaped = to_parslet_compatible(&node, arena, input);
        Some(ast_to_value(&shaped, arena, input))
    });
    let spans = recovered_spans
        .iter()
        .map(|(s, e)| serde_json::json!({ "offset": s, "end": e }))
        .collect::<Vec<_>>();
    serde_json::json!({
        "ok": false, "captures": captures, "error": error,
        "consumed_upto": consumed_upto,
        "partial": partial_value,
        "resyncs": resyncs,
        "recovered_spans": spans,
    })
}

impl PargArtifact {
    /// Parse an entry with the native portable engine and return the
    /// parsanol-shape tree as JSON.
    pub fn parse_shape(&self, entry: &str, input: &str) -> Result<Value, PargError> {
        self.parse_shape_with_compatibility(
            entry,
            input,
            crate::portable::parser::ParserCompatibility::Strict,
        )
    }

    /// Parse with an explicit packrat compatibility policy. Strict remains
    /// the default. Parslet selects historical memo ordering and reports
    /// only failures in the selected rejected subtree.
    pub fn parse_shape_with_compatibility(
        &self,
        entry: &str,
        input: &str,
        compatibility: crate::portable::parser::ParserCompatibility,
    ) -> Result<Value, PargError> {
        self.parse_shape_with_reporter(
            entry,
            input,
            compatibility,
            crate::portable::parser::FailureReporter::Default,
        )
    }
    /// Parse with an explicit diagnostic reporter. Deepest retains failures
    /// from successful speculative applications, matching an explicit Ruby
    /// Deepest reporting pass. Acceptance and memo semantics are unchanged.
    pub fn parse_shape_with_reporter(
        &self,
        entry: &str,
        input: &str,
        compatibility: crate::portable::parser::ParserCompatibility,
        reporter: crate::portable::parser::FailureReporter,
    ) -> Result<Value, PargError> {
        with_parser_stack(|| self.parse_shape_inner(entry, input, compatibility, reporter))
            .map_err(PargError::ParseFailed)?
    }

    fn parse_shape_inner(
        &self,
        entry: &str,
        input: &str,
        compatibility: crate::portable::parser::ParserCompatibility,
        reporter: crate::portable::parser::FailureReporter,
    ) -> Result<Value, PargError> {
        let decoded = self.decoded_entry(entry)?;
        // The shaped tree can dwarf the input (large grammars bind few
        // characters per node); size the arena for the tree, not the text.
        let mut arena = AstArena::for_input(input.len().max(1 << 16));
        let mut parser =
            PortableParser::with_analysis(&decoded.grammar, input, &mut arena, &decoded.analysis);
        parser
            .set_ascii_state_match_atoms(&decoded.ascii_matches)
            .map_err(|err| PargError::ParseFailed(err.to_string()))?;
        parser
            .set_compatibility(compatibility)
            .map_err(|err| PargError::ParseFailed(err.to_string()))?;
        parser.set_failure_reporter(reporter);
        if compatibility == crate::portable::parser::ParserCompatibility::Parslet {
            if let Some(identity) = self.legacy_rule_identity(entry) {
                parser.set_legacy_rule_identity(identity);
            }
        }
        let raw = parser.parse().map_err(|err| {
            if err.is_abort() {
                // Resource and runtime aborts are not syntax expectations:
                // the failure wire belongs to whatever branch was active, so
                // rendering it would send users after a phantom `]`.
                return PargError::runtime_abort(err, parser.call_depth_limit_hit());
            }
            match parser.failure_wire() {
                Some((offset, expected)) => {
                    let (line, column) = line_column(input, offset);
                    let ranked = parser
                        .failure_ranks()
                        .iter()
                        .map(|(p, labels)| (*p, labels.clone()))
                        .collect();
                    PargError::ParseWire {
                        offset,
                        line,
                        column,
                        expected,
                        ranked,
                    }
                }
                None => PargError::ParseFailed(err.to_string()),
            }
        })?;
        let shaped = to_parslet_compatible(&raw, &mut arena, input);
        Ok(ast_to_value(&shaped, &arena, input))
    }

    /// Parse and bind in one step, mirroring `Artifact#parse_and_bind`.
    pub fn parse_and_bind(&self, entry: &str, input: &str) -> Result<Value, PargError> {
        let shape = self.parse_shape(entry, input)?;
        self.apply_bindings(entry, &shape)
    }

    /// Best-effort recovery (parsanol-ruby TODO.sota/1, R1): on success
    /// returns `{"ok": true, "shape": ...}` exactly like `parse_shape`;
    /// on failure returns `{"ok": false, "captures": ..., "error": ...}`
    /// where `captures` carries every capture committed before the
    /// failure (spans as the standard slice wire objects) and `error`
    /// is the failure wire (offset/line/column/expected). Zero extra
    /// cost on success paths — the recovery envelope is built only
    /// from state the failed parse already left behind.
    pub fn parse_shape_recovering(&self, entry: &str, input: &str) -> Result<Value, PargError> {
        with_parser_stack(|| self.parse_shape_recovering_inner(entry, input))
            .map_err(PargError::ParseFailed)?
    }
    fn parse_shape_recovering_inner(&self, entry: &str, input: &str) -> Result<Value, PargError> {
        let decoded = self.decoded_entry(entry)?;
        let grammar = &decoded.grammar;
        let ascii_matches = &decoded.ascii_matches;
        let mut arena = AstArena::for_input(input.len().max(1 << 16));
        let mut parser = PortableParser::new(grammar, input, &mut arena);
        parser
            .set_ascii_state_match_atoms(ascii_matches)
            .map_err(|err| PargError::ParseFailed(err.to_string()))?;
        match parser.parse() {
            Ok(_) => {
                let shape = self.parse_shape(entry, input)?;
                Ok(serde_json::json!({ "ok": true, "shape": shape }))
            }
            Err(error) if error.is_abort() => Err(PargError::runtime_abort(
                error,
                parser.call_depth_limit_hit(),
            )),
            Err(_) => {
                // The generational undo log empties the visible store
                // on the failing branch — that rollback IS the
                // correctness mechanism. Recovery re-parses once with
                // journaling enabled: the journal observes every
                // capture, including writes the failed attempt rolled
                // back. Success paths never journal (zero cost).
                let mut render_arena = AstArena::for_input(input.len().max(1 << 16));
                let mut journal_arena = AstArena::for_input(input.len().max(1 << 16));
                let mut journaling = PortableParser::with_analysis(
                    grammar,
                    input,
                    &mut journal_arena,
                    &decoded.analysis,
                );
                journaling
                    .set_ascii_state_match_atoms(ascii_matches)
                    .map_err(|err| PargError::ParseFailed(err.to_string()))?;
                journaling.capture_state_mut().set_journaling(true);
                journaling.set_track_max_end(true);
                journaling.set_collect_partial(true);
                if let Err(error) = journaling.parse() {
                    if error.is_abort() {
                        return Err(PargError::runtime_abort(
                            error,
                            journaling.call_depth_limit_hit(),
                        ));
                    }
                }
                let mut partial = journaling.take_partial_stash();
                let journal: Vec<(String, CaptureValue)> =
                    journaling.capture_state().journal().to_vec();
                let consumed_upto = journaling.max_end_pos();
                let failure_wire = journaling.failure_wire();
                drop(journaling);
                // The stash items carry journal_arena pool indexes; the
                // recovery envelope renders through the render arena.
                // Journaling is dropped above, so the render arena is
                // free for the copy and for the rounds' probe values.
                if let Some((_, items, _)) = partial.as_mut() {
                    let moved: Vec<AstNode> = items
                        .iter()
                        .map(|item| copy_ast_into_arena(item, &journal_arena, &mut render_arena))
                        .collect();
                    *items = moved;
                }
                // R2 (TODO.sota/1), inferred resync — no grammar
                // addition: from the failure, scan forward for the next
                // byte offset where a spine iteration matches; the
                // skipped span becomes an error annotation and parsing
                // continues. Bounded (MAX_RESYNCS rounds, each must
                // advance) so recovery stays linear in the input.
                // The recovery spine: the first UNBOUNDED repetition
                // reachable from the root whose body anchors at fixed
                // literals AND drives rule calls — the list shape. A
                // bare char-class body (json's ws = *( %x20 / ... )) or
                // a char-run rule repetition (`1*digit` inside int) is
                // not a recovery granularity — the trial scan would
                // "match" at every offset and stall byte-by-byte. The
                // walk is breadth-first: the outermost list wins, so a
                // grammar nesting both an object member list and an
                // array element list recovers at the member
                // granularity, which also carries the enclosing
                // object's separators (rs#539 layers 1-2).
                let spine_body = {
                    fn substantial_spine(grammar: &Grammar, root: usize) -> Option<usize> {
                        let mut visited = std::collections::HashSet::new();
                        let mut queue = std::collections::VecDeque::new();
                        queue.push_back(root);
                        while let Some(idx) = queue.pop_front() {
                            if !visited.insert(idx) {
                                continue;
                            }
                            match grammar.get_atom(idx) {
                                Some(Atom::Repetition { atom, max, .. }) => {
                                    // substantial when the body anchors at
                                    // fixed literals (":" "," "\"") and is not a
                                    // char matcher: a bare class run (`*digit`),
                                    // or an alternative offering a bare class
                                    // branch (a string's interior run). Those
                                    // match at (nearly) every offset, so the
                                    // trial scan "finds" them everywhere and
                                    // the rounds stall byte-by-byte (rs#539
                                    // layer 2: the member list must beat
                                    // `1*digit` inside int and the string
                                    // interior). Flat terminal lists (a
                                    // `pair*` of `k=v;` items) have no nested
                                    // rule calls and stay substantial — their
                                    // literals anchor the scan.
                                    fn anchors_iteration(
                                        grammar: &Grammar,
                                        idx: usize,
                                        seen: &mut std::collections::HashSet<usize>,
                                    ) -> bool {
                                        if !seen.insert(idx) {
                                            return false;
                                        }
                                        match grammar.get_atom(idx) {
                                            Some(Atom::Str { .. }) => true,
                                            Some(Atom::Entity { atom })
                                            | Some(Atom::Named { atom, .. }) => {
                                                anchors_iteration(grammar, *atom, seen)
                                            }
                                            Some(Atom::Sequence { atoms })
                                            | Some(Atom::Alternative { atoms }) => atoms
                                                .iter()
                                                .any(|&a| anchors_iteration(grammar, a, seen)),
                                            Some(Atom::Repetition { atom, .. })
                                            | Some(Atom::Ignore { atom })
                                            | Some(Atom::Trivia { atom })
                                            | Some(Atom::Capture { atom, .. })
                                            | Some(Atom::Scope { atom }) => {
                                                anchors_iteration(grammar, *atom, seen)
                                            }
                                            _ => false,
                                        }
                                    }
                                    // A char matcher: a bare class/regex, or an
                                    // alternative offering one as a branch.
                                    // Wrapper atoms resolve through; a Sequence
                                    // with a fixed literal is never one.
                                    fn char_matcher_body(
                                        grammar: &Grammar,
                                        idx: usize,
                                        seen: &mut std::collections::HashSet<usize>,
                                    ) -> bool {
                                        if !seen.insert(idx) {
                                            return false;
                                        }
                                        match grammar.get_atom(idx) {
                                            Some(Atom::Re { .. }) | Some(Atom::Dynamic { .. }) => {
                                                true
                                            }
                                            Some(Atom::Entity { atom })
                                            | Some(Atom::Named { atom, .. }) => {
                                                char_matcher_body(grammar, *atom, seen)
                                            }
                                            Some(Atom::Alternative { atoms }) => atoms
                                                .iter()
                                                .any(|&a| char_matcher_body(grammar, a, seen)),
                                            Some(Atom::Repetition { atom, .. })
                                            | Some(Atom::Ignore { atom })
                                            | Some(Atom::Trivia { atom })
                                            | Some(Atom::Capture { atom, .. })
                                            | Some(Atom::Scope { atom }) => {
                                                char_matcher_body(grammar, *atom, seen)
                                            }
                                            _ => false,
                                        }
                                    }
                                    let substantial = max.is_none()
                                        && anchors_iteration(
                                            grammar,
                                            *atom,
                                            &mut std::collections::HashSet::new(),
                                        )
                                        && !char_matcher_body(
                                            grammar,
                                            *atom,
                                            &mut std::collections::HashSet::new(),
                                        );
                                    if substantial {
                                        return Some(*atom);
                                    }
                                    // bounded (the `[ x ]` form lowers to
                                    // Repetition{max: 1}) or char-level: descend
                                    // into the body for nested spines
                                    queue.push_back(*atom);
                                }
                                Some(Atom::Named { atom, .. })
                                | Some(Atom::Entity { atom })
                                | Some(Atom::Ignore { atom })
                                | Some(Atom::Trivia { atom })
                                | Some(Atom::Capture { atom, .. })
                                | Some(Atom::Scope { atom }) => queue.push_back(*atom),
                                Some(Atom::Sequence { atoms })
                                | Some(Atom::Alternative { atoms }) => {
                                    for a in atoms {
                                        queue.push_back(*a);
                                    }
                                }
                                _ => {}
                            }
                        }
                        None
                    }
                    substantial_spine(grammar, grammar.root)
                };

                let mut resyncs: Vec<Value> = Vec::new();
                // The spine boundary, not the failure offset, anchors
                // the resync scan: junk frequently lies BEFORE the
                // deepest failure (the failed iteration consumed its
                // own prefix), so a scan from the failure offset can
                // never match it. With no matched prefix the boundary
                // is 0 - a declared pattern is usable from the start
                // (rs#304 Gap B, the ruby engine's convergence twin).
                let mut recovered_spans: Vec<(u32, u32)> = Vec::new();
                let resync_budget = MAX_RESYNCS + input.len() / 1024;
                if let Some(body) = spine_body {
                    let stash_was_empty = partial.is_none();
                    let mut fresh_items: Vec<AstNode> = Vec::new();
                    let mut cursor = match partial.as_ref() {
                        Some((end, _, _)) => *end,
                        // No spine-tracked partial: the journal's
                        // consumption boundary is where the good parse
                        // stopped — recovery resumes from THERE, not
                        // from byte 0 (rs#539: non-spine grammars).
                        None => consumed_upto.min(input.len()),
                    };
                    let items: &mut Vec<AstNode> = match partial.as_mut() {
                        Some((_, items, _)) => items,
                        None => &mut fresh_items,
                    };
                    // Declared resync (`resync <rule> = <pattern>`, R2):
                    // the pattern is baked into the entry; the skip spans
                    // are exactly the pattern's matches — the author owns
                    // them, deterministically. The spine body's rule call
                    // resolves through the entry's rule-atom table to the
                    // declared pattern for ITS rule (json's member list
                    // uses `resync member`, not the root's); the
                    // entry-level singular key stays as the root alias.
                    // When no pattern is declared, the inferred trial scan
                    // applies.
                    let decode_resync =
                        |value: &Value| -> Option<Result<(Grammar, Vec<usize>), PargError>> {
                            if value
                                .get("atoms")
                                .and_then(Value::as_array)
                                .is_some_and(|atoms| {
                                    atoms
                                        .iter()
                                        .any(|atom| atom.get("StateMatchAscii").is_some())
                                })
                            {
                                Some(super::decode_ascii_state_grammar(value.clone()))
                            } else {
                                serde_json::from_value::<Grammar>(value.clone())
                                    .ok()
                                    .map(|grammar| Ok((grammar, Vec::new())))
                            }
                        };
                    let spine_rule: Option<String> = spine_body.and_then(|body| {
                        let target = match grammar.get_atom(body) {
                            Some(Atom::Entity { atom }) | Some(Atom::Named { atom, .. }) => {
                                Some(*atom)
                            }
                            Some(Atom::Sequence { atoms }) | Some(Atom::Alternative { atoms }) => {
                                atoms.iter().find_map(|&a| match grammar.get_atom(a) {
                                    Some(Atom::Entity { atom })
                                    | Some(Atom::Named { atom, .. }) => Some(*atom),
                                    _ => None,
                                })
                            }
                            _ => None,
                        }?;
                        self.entry_value(entry, "rule_atoms")?
                            .as_object()?
                            .iter()
                            .find(|(_, index)| index.as_u64() == Some(target as u64))
                            .map(|(name, _)| name.clone())
                    });
                    let declared_grammar = spine_rule
                        .as_ref()
                        .and_then(|rule_name| {
                            self.entry_value(entry, "resyncs")?.get(rule_name).cloned()
                        })
                        .or_else(|| self.entry_value(entry, "resync").cloned())
                        .and_then(|value| decode_resync(&value))
                        .transpose()?;
                    // The pattern parser gets its own arena: the
                    // journaling parser holds arena2 for the whole
                    // recovery pass.
                    let mut pattern_arena = AstArena::for_input(input.len().max(1 << 16));
                    let mut pattern_parser = declared_grammar.as_ref().map(|(grammar, _)| {
                        PortableParser::new(grammar, input, &mut pattern_arena)
                    });
                    if let (Some(parser), Some((_, modes))) =
                        (pattern_parser.as_mut(), declared_grammar.as_ref())
                    {
                        parser
                            .set_ascii_state_match_atoms(modes)
                            .map_err(|err| PargError::ParseFailed(err.to_string()))?;
                    }
                    // The probes run on a clean parser: the journaling
                    // parser collects partial structure, which swallows
                    // inner failures — a probe there reports prefix
                    // "successes" (`,` + the member key, without `":
                    // value"`), so the rounds stall byte-by-byte on junk
                    // spans that are really failed iterations. The probe
                    // arena only holds span-backed values (InputRef reads
                    // the input at render time), so its items join the
                    // journaling stash safely.
                    let mut probe = PortableParser::with_analysis(
                        grammar,
                        input,
                        &mut render_arena,
                        &decoded.analysis,
                    );
                    probe
                        .set_ascii_state_match_atoms(ascii_matches)
                        .map_err(|err| PargError::ParseFailed(err.to_string()))?;
                    'rounds: for _round in 0..resync_budget {
                        // An iteration may already be viable at the
                        // cursor (e.g. right after a declared skip).
                        if let Ok(r) = probe.try_atom_impl(body, cursor, false) {
                            if r.end_pos > cursor {
                                recovered_spans.push((cursor as u32, r.end_pos as u32));
                                // The probe parses into the render arena,
                                // so its shaped values join the stash as-is.
                                items.push(r.value.clone());
                                cursor = r.end_pos;
                                loop {
                                    match probe.try_atom_impl(body, cursor, false) {
                                        Ok(r) if r.end_pos > cursor => {
                                            recovered_spans.push((cursor as u32, r.end_pos as u32));
                                            items.push(r.value.clone());
                                            cursor = r.end_pos;
                                        }
                                        _ => break,
                                    }
                                }
                                if cursor >= input.len() {
                                    break;
                                }
                            }
                        }
                        // Skip phase.
                        if let Some(pattern) = pattern_parser.as_mut() {
                            // Declared: scan for the next pattern match;
                            // the whole match (gap + delimiter) is the
                            // annotated error span.
                            let mut scan = cursor;
                            let mut found = None;
                            while scan < input.len() {
                                if let Ok(r) = pattern.parse_from_pos(scan) {
                                    if r.end_pos > scan {
                                        found = Some(r.end_pos);
                                        break;
                                    }
                                }
                                scan += 1;
                            }
                            let Some(resume) = found else {
                                break 'rounds;
                            };
                            resyncs.push(serde_json::json!({
                                "offset": cursor,
                                "skipped": &input[cursor..resume],
                                "resumed_at": resume,
                            }));
                            cursor = resume;
                        } else {
                            // Inferred: scan for the next viable
                            // iteration start.
                            let mut found = None;
                            let mut scan = cursor;
                            while scan < input.len() {
                                if let Ok(r) = probe.try_atom_impl(body, scan, false) {
                                    if r.end_pos > scan {
                                        found = Some((scan, r.end_pos, r.value.clone()));
                                        break;
                                    }
                                }
                                scan += 1;
                            }
                            let Some((skip_start, resume, item)) = found else {
                                break 'rounds;
                            };
                            resyncs.push(serde_json::json!({
                                "offset": cursor,
                                "skipped": &input[cursor..skip_start],
                                "resumed_at": skip_start,
                            }));
                            items.push(item);
                            recovered_spans.push((skip_start as u32, resume as u32));
                            cursor = resume;
                        }
                    }
                    if let Some((end, _, _)) = partial.as_mut() {
                        *end = cursor;
                    }
                    if stash_was_empty && !fresh_items.is_empty() {
                        partial = Some((cursor, fresh_items, RepetitionTag::Repetition));
                    }
                }
                // The entry shape is wrapped under the root rule's
                // name (the shape contract), so the partial wraps the
                // same way.
                let root_is_named = self.entry_root(entry).map(str::to_string).ok();
                Ok(recovery_shape(
                    entry,
                    &journal,
                    consumed_upto,
                    failure_wire,
                    input,
                    &mut render_arena,
                    partial,
                    root_is_named,
                    resyncs,
                    recovered_spans,
                ))
            }
        }
    }

    /// Run the artifact's embedded tests; empty result means green.
    pub fn run_tests(&self) -> Vec<String> {
        let tests: Vec<Value> = self
            .envelope
            .get("tests")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        self.run_test_list(&tests)
    }

    /// Run an external test list (artifact-shaped test hashes).
    /// The compiler-baked default entry (declaration order, not the
    /// sorted view JSON engines see).
    pub fn default_entry(&self) -> Option<&str> {
        self.envelope.get("default_entry").and_then(Value::as_str)
    }

    /// Evaluate artifact-shaped tests (accept/reject/example); returns
    /// Ruby-word-compatible failure descriptions. Empty means green.
    pub fn run_test_list(&self, tests: &[Value]) -> Vec<String> {
        let fallback = self
            .default_entry()
            .or_else(|| self.entry_names().first().copied())
            .unwrap_or_default()
            .to_string();
        tests
            .iter()
            .filter_map(|test| {
                let object = test.as_object()?;
                let kind = object
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or("accept");
                let input = object
                    .get("input")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let entry = object
                    .get("entry")
                    .and_then(Value::as_str)
                    .unwrap_or(&fallback);
                let outcome = (|| -> Result<(), PargError> {
                    let shape = self.parse_shape(entry, input)?;
                    if kind == "reject" {
                        return Err(PargError::UnexpectedParse(input.to_string()));
                    }
                    if kind == "example" {
                        let bound = self.apply_bindings(entry, &shape)?;
                        let Some(expect) = object.get("expect").and_then(Value::as_object) else {
                            return Ok(());
                        };
                        let mismatched: Vec<(&String, &Value)> = expect
                            .iter()
                            .filter(|(key, value)| {
                                canonical(bound.get(*key).unwrap_or(&Value::Null))
                                    != canonical(value)
                            })
                            .collect();
                        if !mismatched.is_empty() {
                            return Err(PargError::CaptureMismatch {
                                input: input.to_string(),
                                detail: format!("expected captures {mismatched:?}, got {bound}"),
                            });
                        }
                    }
                    if kind == "shape" {
                        // Whole-tree assertion: the expectation is the
                        // canonical flattened tree, not a capture map. This
                        // is the phase that can observe repetition flattening
                        // and sequence merge laws.
                        let expected = canonical(object.get("expect").unwrap_or(&Value::Null));
                        let actual = canonical(&shape);
                        if actual != expected {
                            return Err(PargError::CaptureMismatch {
                                input: input.to_string(),
                                detail: format!("expected shape {expected:?}, got {actual:?}"),
                            });
                        }
                    }
                    if kind == "capture" {
                        let Some(expect) = object.get("expect").and_then(Value::as_object) else {
                            return Ok(());
                        };
                        let captures = super::bindings::collect_captures(&shape);
                        let mismatched: Vec<(&String, &Value)> = expect
                            .iter()
                            .filter(|(key, expected)| {
                                // A present nil capture (for example an absent
                                // optional inside `as`) is a value, not a
                                // missing capture: compare presence first.
                                let actual =
                                    captures.get(key.as_str()).and_then(|values| values.first());
                                !matches!(actual,
                                    Some(actual) if canonical(actual) == canonical(expected))
                            })
                            .collect();
                        if !mismatched.is_empty() {
                            let actuals: serde_json::Map<String, Value> = mismatched
                                .iter()
                                .map(|(key, _)| {
                                    let actual = captures
                                        .get(key.as_str())
                                        .and_then(|values| values.first())
                                        .map(canonical)
                                        .unwrap_or(Value::Null);
                                    ((*key).clone(), actual)
                                })
                                .collect();
                            return Err(PargError::CaptureMismatch {
                                input: input.to_string(),
                                detail: format!(
                                    "expected captures {mismatched:?}, got {actuals:?}"
                                ),
                            });
                        }
                    }
                    Ok(())
                })();
                match outcome {
                    Ok(()) => None,
                    // A reject test passing because parsing failed is green.
                    Err(PargError::ParseFailed(_) | PargError::ParseWire { .. })
                        if kind == "reject" =>
                    {
                        None
                    }
                    Err(err) => Some(format!("test {input:?}: {err}")),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::canonical;
    use crate::PargArtifact;
    use serde_json::Value;

    #[test]
    fn canonical_reduces_slice_wire_objects_to_text() {
        let slice = serde_json::json!({
            "value": "ab", "line": 1, "column": 1, "offset": 0, "length": 2
        });
        assert_eq!(canonical(&slice), Value::String("ab".into()));
    }

    #[test]
    fn canonical_recurses_structure_and_keeps_scalars() {
        let value = serde_json::json!({
            "name": {"value": "ab", "line": 1, "column": 1, "offset": 0, "length": 2},
            "year": 2020,
            "ok": true,
            "items": [{"value": "x", "line": 1, "column": 2, "offset": 1, "length": 1}]
        });
        assert_eq!(
            canonical(&value),
            serde_json::json!({ "name": "ab", "year": 2020, "ok": true, "items": ["x"] })
        );
    }

    #[test]
    fn canonical_leaves_plain_objects_intact() {
        let value = serde_json::json!({ "a": { "b": "c" } });
        assert_eq!(canonical(&value), value);
    }

    #[test]
    fn capture_kind_replays_green_against_ruby_compiled_fixture() {
        let artifact =
            PargArtifact::from_json(include_str!("../../tests/fixtures/parg_capture.json"))
                .expect("ruby-compiled fixture must verify");
        assert!(artifact.run_tests().is_empty());
    }

    #[test]
    fn recovering_returns_the_shape_on_success() {
        let artifact =
            PargArtifact::from_json(include_str!("../../tests/fixtures/parg_capture.json"))
                .expect("ruby-compiled fixture must verify");
        let recovered = artifact
            .parse_shape_recovering("identifier", "AB2020")
            .unwrap();
        assert_eq!(recovered["ok"], Value::Bool(true));
        assert_eq!(
            recovered["shape"],
            artifact.parse_shape("identifier", "AB2020").unwrap()
        );
    }

    #[test]
    fn recovering_reports_committed_captures_on_failure() {
        let artifact =
            PargArtifact::from_json(include_str!("../../tests/fixtures/parg_capture.json"))
                .expect("ruby-compiled fixture must verify");
        // "AB20": `pub` matches "AB", the digits match "2" and "0",
        // then `4digit` wants a third digit at EOF and the sequence
        // fails. The furthest successful match ends at EOF — the
        // failure wire points there too. Rule-level captures are
        // tree-level in the portable engine and do not survive the
        // failed sequence: the recovery envelope is failure forensics
        // (R1), with partial-tree recovery specced as the engine
        // capability in TODO.sota/1.
        let recovered = artifact
            .parse_shape_recovering("identifier", "AB20")
            .unwrap();
        assert_eq!(recovered["ok"], Value::Bool(false));
        assert_eq!(recovered["consumed_upto"], Value::Number(4.into()));
        assert_eq!(recovered["error"]["offset"], Value::Number(4.into()));
        assert!(
            recovered["error"]["expected"]
                .as_array()
                .is_some_and(|e| !e.is_empty()),
            "expected set present: {recovered}"
        );
    }

    #[test]
    fn capture_kind_reports_mismatched_expectations() {
        let artifact =
            PargArtifact::from_json(include_str!("../../tests/fixtures/parg_capture.json"))
                .expect("ruby-compiled fixture must verify");
        let mut tests: Vec<Value> =
            serde_json::from_str(artifact.envelope.get("tests").unwrap().to_string().as_str())
                .unwrap();
        // Corrupt the capture expectation: "AB" -> "ZZ" (same shape).
        for test in tests.iter_mut() {
            if test["kind"] == "capture" {
                test["expect"]["pub"] = Value::String("ZZ".into());
            }
        }
        let failures = artifact.run_test_list(&tests);
        assert_eq!(failures.len(), 1, "{failures:?}");
        assert!(failures[0].contains("expected captures"), "{failures:?}");
    }
}
