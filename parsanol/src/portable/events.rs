//! Flat event stream: a linear serialization of the parslet-shaped
//! AST for single-return FFI transfer.
//!
//! `parse_with_builder` drives consumer callbacks event-by-event,
//! which costs one Ruby call per event. `linearize_events` instead
//! emits the same event stream into two flat vectors — an integer
//! opcode stream and a string pool — returned to the host language in
//! one call. A consumer can rebuild the exact shaped tree by replaying
//! the stream (see `replay_events`), or attach domain handling
//! directly to the opcodes.
//!
//! Opcodes (each event consumes a fixed number of i64 words):
//!
//! | op | words after opcode | meaning                            |
//! |----|--------------------|------------------------------------|
//! | 0  | 0                  | nil                                |
//! | 1  | 1 (pool index)     | interned string value              |
//! | 2  | 2 (offset, length) | input slice (string with position) |
//! | 3  | 1 (pool index)     | hash key; the value events follow  |
//! | 4  | 0                  | array start                        |
//! | 5  | 0                  | array end                          |
//! | 6  | 0                  | hash start                         |
//! | 7  | 0                  | hash end                           |
//!
//! Hash nodes emit BEGIN_HASH, one KEY event per entry (followed by
//! that entry's value events), END_HASH. The explicit delimiters keep
//! an array of single-key hashes distinct from one multi-key hash.

use super::arena::AstArena;
use super::ast::AstNode;

/// Nil value event.
pub const OP_NIL: i64 = 0;
/// Interned string event; one pool-index word follows.
pub const OP_STR: i64 = 1;
/// Input slice event; offset and length words follow.
pub const OP_SLICE: i64 = 2;
/// Hash key event; one pool-index word follows.
pub const OP_KEY: i64 = 3;
/// Array start.
pub const OP_BEGIN_ARR: i64 = 4;
/// Array end.
pub const OP_END_ARR: i64 = 5;
/// Hash start.
pub const OP_BEGIN_HASH: i64 = 6;
/// Hash end.
pub const OP_END_HASH: i64 = 7;

/// Linearize a shaped AST into (events, string pool).
pub fn linearize_events(node: &AstNode, arena: &mut AstArena) -> (Vec<i64>, Vec<String>) {
    let mut out = Events {
        events: Vec::new(),
        strings: Vec::new(),
        arena,
    };
    out.emit_node(node);
    (out.events, out.strings)
}

struct Events<'a> {
    events: Vec<i64>,
    strings: Vec<String>,
    arena: &'a mut AstArena,
}

impl Events<'_> {
    fn intern(&mut self, s: &str) -> i64 {
        // Linear scan is fine: pools stay small (rule names + literals)
        // and dedup keeps the transfer compact.
        if let Some(idx) = self.strings.iter().position(|p| p == s) {
            return idx as i64;
        }
        self.strings.push(s.to_string());
        (self.strings.len() - 1) as i64
    }

    fn emit_node(&mut self, node: &AstNode) {
        match node {
            AstNode::Nil => self.events.push(OP_NIL),
            AstNode::StringRef { pool_index } => {
                let s = self.arena.get_string(*pool_index as usize).to_string();
                let idx = self.intern(&s);
                self.events.push(OP_STR);
                self.events.push(idx);
            }
            AstNode::InputRef { length: 0, .. } => {
                // Zero-length slices carry no text; the native path
                // renders them as plain "" (no position), so the event
                // stream must too.
                let idx = self.intern("");
                self.events.push(OP_STR);
                self.events.push(idx);
            }
            AstNode::InputRef { offset, length } => {
                self.events.push(OP_SLICE);
                self.events.push(*offset as i64);
                self.events.push(*length as i64);
            }
            AstNode::Array { pool_index, length } => {
                let items = self.arena.get_array(*pool_index as usize, *length as usize);
                self.events.push(OP_BEGIN_ARR);
                for item in items {
                    self.emit_node(&item);
                }
                self.events.push(OP_END_ARR);
            }
            AstNode::Hash { pool_index, length } => {
                let pairs = self
                    .arena
                    .get_hash_items(*pool_index as usize, *length as usize);
                self.events.push(OP_BEGIN_HASH);
                for (key, value) in pairs {
                    let idx = self.intern(&key);
                    self.events.push(OP_KEY);
                    self.events.push(idx);
                    self.emit_node(&value);
                }
                self.events.push(OP_END_HASH);
            }
            other => {
                // Scalar leaves the shaped tree does not produce today;
                // encode as their string form to stay lossless.
                let s = format!("{:?}", other);
                let idx = self.intern(&s);
                self.events.push(OP_STR);
                self.events.push(idx);
            }
        }
    }
}

/// Replay an event stream back into an arena tree. The inverse of
/// `linearize_events`; used for conformance testing and as the
/// reference consumer.
pub fn replay_events(events: &[i64], strings: &[String], arena: &mut AstArena) -> AstNode {
    let mut pos = 0usize;
    let node = replay_value(events, strings, arena, &mut pos);
    debug_assert_eq!(pos, events.len(), "event stream must be fully consumed");
    node
}

fn replay_value(
    events: &[i64],
    strings: &[String],
    arena: &mut AstArena,
    pos: &mut usize,
) -> AstNode {
    let op = events[*pos];
    *pos += 1;
    match op {
        OP_NIL => AstNode::Nil,
        OP_STR => {
            let idx = events[*pos] as usize;
            *pos += 1;
            let s = strings[idx].clone();
            arena.intern_string(&s)
        }
        OP_SLICE => {
            let offset = events[*pos] as u32;
            let length = events[*pos + 1] as u32;
            *pos += 2;
            arena.input_ref(offset as usize, length as usize)
        }
        OP_BEGIN_HASH => {
            let mut pairs: Vec<(String, AstNode)> = Vec::new();
            while events[*pos] != OP_END_HASH {
                debug_assert_eq!(events[*pos], OP_KEY, "hash entries must start with KEY");
                *pos += 1;
                let idx = events[*pos] as usize;
                *pos += 1;
                let key = strings[idx].clone();
                let value = replay_value(events, strings, arena, pos);
                pairs.push((key, value));
            }
            *pos += 1;
            build_hash(arena, pairs)
        }
        OP_BEGIN_ARR => {
            let mut items: Vec<AstNode> = Vec::new();
            while events[*pos] != OP_END_ARR {
                items.push(replay_value(events, strings, arena, pos));
            }
            *pos += 1;
            arena.alloc_array(items)
        }
        other => panic!("unknown event opcode {other} at position {}", *pos - 1),
    }
}

fn build_hash(arena: &mut AstArena, pairs: Vec<(String, AstNode)>) -> AstNode {
    let refs: Vec<(&str, AstNode)> = pairs.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
    let (pool_index, length) = arena.store_hash(&refs);
    AstNode::Hash { pool_index, length }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::parser::PortableParser;
    use crate::portable::parser_dsl::{dynamic, re, seq, str, GrammarBuilder, ParsletExt};
    use crate::portable::to_parslet_compatible;

    fn assert_roundtrip(grammar: &crate::portable::Grammar, input: &str) {
        let mut arena = AstArena::for_input(input.len());
        arena.set_input(input.to_string());
        let mut parser = PortableParser::new(grammar, input, &mut arena);
        let raw = parser.parse().unwrap();
        let shaped = to_parslet_compatible(&raw, &mut arena, input);

        let (events, strings) = linearize_events(&shaped, &mut arena);
        let replayed = replay_events(&events, &strings, &mut arena);

        assert!(
            semantic_eq(&shaped, &replayed, &arena),
            "event round-trip must be lossless:\n  shaped:   {:?}\n  replayed: {:?}",
            DebugNode(&shaped, &arena, input),
            DebugNode(&replayed, &arena, input),
        );
    }

    fn semantic_eq(a: &AstNode, b: &AstNode, arena: &AstArena) -> bool {
        match (a, b) {
            (AstNode::Nil, AstNode::Nil) => true,
            (AstNode::Bool(x), AstNode::Bool(y)) => x == y,
            (AstNode::Int(x), AstNode::Int(y)) => x == y,
            (AstNode::StringRef { pool_index: x }, AstNode::StringRef { pool_index: y }) => {
                arena.get_string(*x as usize) == arena.get_string(*y as usize)
            }
            // Zero-length slices normalize to plain "" at emission.
            (AstNode::StringRef { pool_index: x }, AstNode::InputRef { length: 0, .. })
            | (AstNode::InputRef { length: 0, .. }, AstNode::StringRef { pool_index: x }) => {
                arena.get_string(*x as usize).is_empty()
            }
            (
                AstNode::InputRef {
                    offset: x,
                    length: xl,
                },
                AstNode::InputRef {
                    offset: y,
                    length: yl,
                },
            ) => x == y && xl == yl,
            (
                AstNode::Array {
                    pool_index: xp,
                    length: xl,
                },
                AstNode::Array {
                    pool_index: yp,
                    length: yl,
                },
            ) => {
                let xa = arena.get_array(*xp as usize, *xl as usize);
                let ya = arena.get_array(*yp as usize, *yl as usize);
                xa.len() == ya.len()
                    && xa
                        .iter()
                        .zip(ya.iter())
                        .all(|(x, y)| semantic_eq(x, y, arena))
            }
            (
                AstNode::Hash {
                    pool_index: xp,
                    length: xl,
                },
                AstNode::Hash {
                    pool_index: yp,
                    length: yl,
                },
            ) => {
                let xh = arena.get_hash_items(*xp as usize, *xl as usize);
                let yh = arena.get_hash_items(*yp as usize, *yl as usize);
                xh.len() == yh.len()
                    && xh
                        .iter()
                        .zip(yh.iter())
                        .all(|((xk, xv), (yk, yv))| xk == yk && semantic_eq(xv, yv, arena))
            }
            _ => false,
        }
    }

    struct DebugNode<'a>(&'a AstNode, &'a AstArena, &'a str);

    impl std::fmt::Debug for DebugNode<'_> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            let (node, arena, input) = (self.0, self.1, self.2);
            match node {
                AstNode::Nil => write!(f, "nil"),
                AstNode::StringRef { pool_index } => {
                    write!(f, "str({:?})", arena.get_string(*pool_index as usize))
                }
                AstNode::InputRef { offset, length } => write!(
                    f,
                    "in({:?})",
                    &input[*offset as usize..(*offset + *length) as usize]
                ),
                AstNode::Array { pool_index, length } => {
                    let items = arena.get_array(*pool_index as usize, *length as usize);
                    write!(f, "[")?;
                    for (i, item) in items.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{:?}", DebugNode(item, arena, input))?;
                    }
                    write!(f, "]")
                }
                AstNode::Hash { pool_index, length } => {
                    let pairs = arena.get_hash_items(*pool_index as usize, *length as usize);
                    write!(f, "{{")?;
                    for (i, (k, v)) in pairs.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{}: {:?}", k, DebugNode(v, arena, input))?;
                    }
                    write!(f, "}}")
                }
                other => write!(f, "{:?}", other),
            }
        }
    }

    #[test]
    fn roundtrip_simple_capture() {
        let grammar = GrammarBuilder::new()
            .rule(
                "schema",
                seq(vec![
                    dynamic(str("SCHEMA ")),
                    dynamic(re("[a-z]+").label("name")),
                    dynamic(str(";")),
                ]),
            )
            .build();
        assert_roundtrip(&grammar, "SCHEMA test;");
    }

    #[test]
    fn roundtrip_repetition_and_nesting() {
        let grammar = GrammarBuilder::new()
            .rule(
                "entity",
                seq(vec![
                    dynamic(str("ENTITY ")),
                    dynamic(re("[a-z_]+").label("name")),
                    dynamic(str(" ")),
                    dynamic(
                        seq(vec![dynamic(re("[a-z]+").label("attr")), dynamic(str(","))])
                            .label("attrs")
                            .repeat(0, None),
                    ),
                ]),
            )
            .build();
        assert_roundtrip(&grammar, "ENTITY point x,y,z,");
        assert_roundtrip(&grammar, "ENTITY point ");
    }

    #[test]
    fn roundtrip_empty_and_maybe() {
        let grammar = GrammarBuilder::new()
            .rule(
                "test",
                seq(vec![
                    dynamic(str("E")),
                    dynamic(seq(vec![str("A").optional(), str("B").optional()]).label("m")),
                    dynamic(str("z")),
                ]),
            )
            .build();
        assert_roundtrip(&grammar, "Ez");
        assert_roundtrip(&grammar, "EABz");
    }
}
