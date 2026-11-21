//! The DFA table builder (rs#574 Phase 2, slice B).
//!
//! Builds a [`DfaTable`] for a rule the provability prover accepted.
//! The construction is the classic Thompson NFA over the atom tree,
//! then subset determinization — the provability proof (pairwise
//! disjoint choice FIRST sets, loop-exit disjointness, no recursion)
//! means the determinized table is single-path, so the greedy longest
//! walk is exact.
//!
//! Stage 1 scope: capture-free rules (pure spans — the string/ws/
//! number interiors). Named markers inside provable bodies ride a
//! later slice (accept-state → value-synthesis metadata). The builder
//! is the final authority: anything it cannot express returns `None`
//! and the routing keeps the existing lowering.

use super::dfa_table::DfaTable;
use crate::portable::char_class::CharRanges;
use crate::portable::grammar::{Atom, Grammar, RepetitionTag};

/// One Thompson NFA edge: a byte-class bitmap or epsilon.
#[derive(Debug, Clone)]
enum Edge {
    Epsilon,
    Bytes(Box<[bool; 256]>),
}

#[derive(Default)]
struct Nfa {
    byte_edges: Vec<(u32, [bool; 256], u32)>,
    eps: Vec<(u32, u32)>,
    nodes: u32,
}

impl Nfa {
    fn node(&mut self) -> u32 {
        let id = self.nodes;
        self.nodes += 1;
        id
    }

    fn edge(&mut self, from: u32, edge: Edge, to: u32) {
        match edge {
            Edge::Epsilon => self.eps.push((from, to)),
            Edge::Bytes(bits) => self.byte_edges.push((from, *bits, to)),
        }
    }
}

/// The byte bitmap for one literal string (the byte-sequence chain
/// uses single-byte edges; this is the single-edge form for classes).
fn bits_of(bytes: &[u8]) -> Box<[bool; 256]> {
    let mut bits = [false; 256];
    for &b in bytes {
        bits[b as usize] = true;
    }
    Box::new(bits)
}

/// The single-byte ASCII class for an `Re` atom, or `None` when the
/// pattern spans codepoints the byte-level builder refuses.
fn ascii_class_bits(pattern: &str) -> Option<Box<[bool; 256]>> {
    let ranges = CharRanges::from_pattern(pattern)?;
    let mut bits = [false; 256];
    for (lo, hi) in ranges.codepoint_ranges() {
        // Stage 1: ASCII single-byte codepoints only — a multi-byte
        // codepoint edge is a byte-sequence chain, and a mixed class's
        // UTF-8 prefixes need their own construction (a later slice).
        if lo > 0x7F || hi > 0x7F {
            return None;
        }
        for cp in lo..=hi.min(lo + 0xFFFF) {
            bits[cp as usize] = true;
        }
        if hi > lo + 0xFFFF {
            break;
        }
    }
    Some(Box::new(bits))
}

/// Build the DFA table for `rule_idx`, or `None` when the atom tree
/// carries something the byte-level builder refuses. `in_progress`
/// guards recursion (the provability verdict already refused it — this
/// is the builder's own belt).
pub fn build_rule_dfa(grammar: &Grammar, rule_idx: usize) -> Option<DfaTable> {
    let mut nfa = Nfa::default();
    let (start, accept) = thompson(grammar, rule_idx, &mut nfa, &mut Vec::new())?;
    Some(determinize(&nfa, start, accept))
}

/// The Thompson fragment for one atom: `(start, accept)` node ids.
fn thompson(
    grammar: &Grammar,
    idx: usize,
    nfa: &mut Nfa,
    in_progress: &mut Vec<usize>,
) -> Option<(u32, u32)> {
    if in_progress.contains(&idx) {
        return None;
    }
    let atom = grammar.get_atom(idx)?;
    in_progress.push(idx);
    let fragment = match atom {
        Atom::Str { pattern } => {
            let bytes = pattern.as_bytes();
            if bytes.is_empty() {
                let n = nfa.node();
                Some((n, n))
            } else {
                let mut nodes = Vec::with_capacity(bytes.len() + 1);
                for _ in 0..=bytes.len() {
                    nodes.push(nfa.node());
                }
                for (i, &b) in bytes.iter().enumerate() {
                    nfa.edge(nodes[i], Edge::Bytes(bits_of(&[b])), nodes[i + 1]);
                }
                Some((*nodes.first()?, *nodes.last()?))
            }
        }
        Atom::Re { pattern } => {
            let bits = ascii_class_bits(pattern)?;
            let (s, a) = (nfa.node(), nfa.node());
            nfa.edge(s, Edge::Bytes(bits), a);
            Some((s, a))
        }
        Atom::Sequence { atoms } => {
            // Chain the fragments with epsilon joins.
            let mut acc: Option<(u32, u32)> = None;
            for &child in atoms {
                let (cs, ca) = thompson(grammar, child, nfa, in_progress)?;
                acc = Some(match acc {
                    None => (cs, ca),
                    Some((ps, pa)) => {
                        nfa.edge(pa, Edge::Epsilon, cs);
                        (ps, ca)
                    }
                });
            }
            acc
        }
        Atom::Alternative { atoms } => {
            let (s, a) = (nfa.node(), nfa.node());
            for &branch in atoms {
                let (bs, ba) = thompson(grammar, branch, nfa, in_progress)?;
                nfa.edge(s, Edge::Epsilon, bs);
                nfa.edge(ba, Edge::Epsilon, a);
            }
            Some((s, a))
        }
        Atom::Repetition {
            atom: body,
            min,
            max,
            tag: RepetitionTag::Repetition | RepetitionTag::Maybe,
        } => {
            // Mandatory iterations chained; the optional remainder as
            // epsilon-skipped copies; the unlimited tail as a loop.
            // Nullable bodies were refused by the provability verdict;
            // the builder refuses them again as its own belt.
            let body_states = fragment_states(grammar, *body, 0, &mut Vec::new())?;
            if max.is_some_and(|m| (m.saturating_sub(*min)) as u64 * body_states as u64 > 4096) {
                return None; // the bounded-unroll sanity cap
            }
            let (entry, exit) = (nfa.node(), nfa.node());
            let mut cursor = entry;
            for _ in 0..*min {
                let (bs, ba) = thompson(grammar, *body, nfa, in_progress)?;
                nfa.edge(cursor, Edge::Epsilon, bs);
                cursor = ba;
            }
            match max {
                None => {
                    let (bs, ba) = thompson(grammar, *body, nfa, in_progress)?;
                    if *min == 0 {
                        // The zero-iteration path: without it the start
                        // closure never reaches the exit.
                        nfa.edge(cursor, Edge::Epsilon, exit);
                    }
                    nfa.edge(cursor, Edge::Epsilon, bs);
                    nfa.edge(ba, Edge::Epsilon, bs); // the loop back
                    nfa.edge(ba, Edge::Epsilon, exit);
                }
                Some(m) => {
                    for _ in *min..*m {
                        // epsilon-optional copy
                        let skip = nfa.node();
                        nfa.edge(cursor, Edge::Epsilon, skip);
                        let (bs, ba) = thompson(grammar, *body, nfa, in_progress)?;
                        nfa.edge(cursor, Edge::Epsilon, bs);
                        nfa.edge(ba, Edge::Epsilon, exit);
                        cursor = skip;
                    }
                    nfa.edge(cursor, Edge::Epsilon, exit);
                }
            }
            Some((entry, exit))
        }
        Atom::Named { atom, .. }
        | Atom::Capture { atom, .. }
        | Atom::Scope { atom }
        | Atom::Ignore { atom }
        | Atom::Trivia { atom }
        | Atom::TriviaCapture { atom, .. } => thompson(grammar, *atom, nfa, in_progress),
        Atom::Entity { atom } => thompson(grammar, *atom, nfa, in_progress),
        Atom::Cut => {
            let n = nfa.node();
            Some((n, n))
        }
        // Predicates, constants, host-computed and stateful atoms: the
        // provability verdict refuses these; the builder agrees.
        Atom::Lookahead { .. }
        | Atom::Lookbehind { .. }
        | Atom::Constant { .. }
        | Atom::Dynamic { .. }
        | Atom::Custom { .. }
        | Atom::CustomRef { .. }
        | Atom::StateSet { .. }
        | Atom::StateMatch { .. }
        | Atom::StateSwitch { .. } => None,
    };
    in_progress.pop();
    fragment
}

/// The NFA node count one atom's fragment would occupy (the bounded
/// unroll's sanity check, without building it).
fn fragment_states(
    grammar: &Grammar,
    idx: usize,
    depth: usize,
    seen: &mut Vec<usize>,
) -> Option<u32> {
    if depth > 64 || seen.contains(&idx) {
        return None;
    }
    seen.push(idx);
    let atom = grammar.get_atom(idx)?;
    let n = match atom {
        Atom::Str { pattern } => pattern.len().max(1) as u32,
        Atom::Re { pattern } => {
            let _ = ascii_class_bits(pattern)?;
            1
        }
        Atom::Sequence { atoms } | Atom::Alternative { atoms } => {
            let mut total = 0;
            for &child in atoms {
                total += fragment_states(grammar, child, depth + 1, seen)?;
            }
            total + 1
        }
        Atom::Repetition { atom, max, .. } => {
            let body = fragment_states(grammar, *atom, depth + 1, seen)?;
            match max {
                None => body + 2,
                Some(m) => body.saturating_mul((*m).max(1) as u32) + 2,
            }
        }
        Atom::Named { atom, .. }
        | Atom::Capture { atom, .. }
        | Atom::Scope { atom }
        | Atom::Ignore { atom }
        | Atom::Trivia { atom }
        | Atom::TriviaCapture { atom, .. } => fragment_states(grammar, *atom, depth + 1, seen)?,
        Atom::Entity { atom } => fragment_states(grammar, *atom, depth + 1, seen)?,
        Atom::Cut => 1,
        _ => return None,
    };
    seen.pop();
    Some(n)
}

/// Subset determinization over epsilon closures into the dense table.
fn determinize(nfa: &Nfa, start: u32, accept: u32) -> DfaTable {
    determinize_marked(nfa, start, accept, &std::collections::HashSet::new())
}

/// The deterministic table for the UNBOUNDED LOOP of one body atom:
/// star(body) with the body's fragment-accept nodes marked — a DFA
/// state containing a marked node completes one iteration, which the
/// envelope executor turns into the iteration's span. Any `min` is
/// enforced by the executor against the crossing count.
pub fn build_loop_dfa(grammar: &Grammar, body_idx: usize) -> Option<DfaTable> {
    // The body must consume at least one byte: a nullable body makes
    // the loop's iteration boundaries degenerate.
    if !consumes_min_one(grammar, body_idx, 0, &mut Vec::new()) {
        return None;
    }
    let mut nfa = Nfa::default();
    let (bs, ba) = thompson(grammar, body_idx, &mut nfa, &mut Vec::new())?;
    let entry = nfa.node();
    let exit = nfa.node();
    nfa.edge(entry, Edge::Epsilon, bs);
    nfa.edge(ba, Edge::Epsilon, bs); // the loop back
    nfa.edge(ba, Edge::Epsilon, exit);
    let mut marked = std::collections::HashSet::new();
    marked.insert(ba);
    Some(determinize_marked(&nfa, entry, exit, &marked))
}

/// Whether the atom consumes at least one byte on every match path.
fn consumes_min_one(grammar: &Grammar, idx: usize, depth: usize, seen: &mut Vec<usize>) -> bool {
    if depth > 64 {
        return false;
    }
    if seen.contains(&idx) {
        return true; // a cycle back to an in-progress node: assume consuming
    }
    seen.push(idx);
    let ok = (|| {
        let atom = grammar.get_atom(idx)?;
        Some(match atom {
            Atom::Str { pattern } => !pattern.is_empty(),
            Atom::Re { .. } => true,
            Atom::Sequence { atoms } => {
                atoms.iter().any(|&c| consumes_min_one(grammar, c, depth + 1, seen))
            }
            Atom::Alternative { atoms } => {
                atoms.iter().all(|&c| consumes_min_one(grammar, c, depth + 1, seen))
            }
            Atom::Repetition { atom, min, .. } => {
                *min >= 1 && consumes_min_one(grammar, *atom, depth + 1, seen)
            }
            Atom::Named { atom, .. }
            | Atom::Capture { atom, .. }
            | Atom::Scope { atom }
            | Atom::Ignore { atom }
            | Atom::Trivia { atom }
            | Atom::TriviaCapture { atom, .. } => {
                consumes_min_one(grammar, *atom, depth + 1, seen)
            }
            Atom::Entity { atom } => consumes_min_one(grammar, *atom, depth + 1, seen),
            _ => false,
        })
    })();
    seen.pop();
    ok.unwrap_or(false)
}

fn determinize_marked(
    nfa: &Nfa,
    start: u32,
    accept: u32,
    marked: &std::collections::HashSet<u32>,
) -> DfaTable {
    // Epsilon closures per NFA node.
    let mut closures: Vec<Vec<u32>> = vec![Vec::new(); nfa.nodes as usize];
    for node in 0..nfa.nodes {
        let mut stack = vec![node];
        let mut seen = vec![false; nfa.nodes as usize];
        seen[node as usize] = true;
        while let Some(current) = stack.pop() {
            closures[node as usize].push(current);
            for (from, to) in &nfa.eps {
                if *from == current && !seen[*to as usize] {
                    seen[*to as usize] = true;
                    stack.push(*to);
                }
            }
        }
        closures[node as usize].sort_unstable();
    }

    let close = |set: &[u32]| -> Vec<u32> {
        let mut merged: Vec<u32> = set
            .iter()
            .flat_map(|n| closures[*n as usize].iter().copied())
            .collect();
        merged.sort_unstable();
        merged.dedup();
        merged
    };

    let mut ids: HashMap<Vec<u32>, u32> = HashMap::new();
    let mut subsets: Vec<Vec<u32>> = Vec::new();
    let mut transitions: Vec<u32> = Vec::new();
    let mut accepts: Vec<bool> = Vec::new();

    let initial = close(&[start]);
    ids.insert(initial.clone(), 0);
    subsets.push(initial);
    accepts.push(subsets[0].contains(&accept));
    transitions.extend(std::iter::repeat_n(u32::MAX, 256));
    let mut iteration_accepts: Vec<bool> = Vec::new();
    iteration_accepts.push(subsets[0].iter().any(|n| marked.contains(n)));

    let mut worklist = 0usize;
    while worklist < subsets.len() {
        let subset = subsets[worklist].clone();
        worklist += 1;
        for byte in 0u32..256 {
            let moved: Vec<u32> = subset
                .iter()
                .filter_map(|n| {
                    nfa.byte_edges
                        .iter()
                        .filter(|(from, bits, _)| *from == *n && bits[byte as usize])
                        .map(|(_, _, to)| *to)
                        .next()
                })
                .collect();
            if moved.is_empty() {
                continue;
            }
            let closed = close(&moved);
            let next = match ids.get(&closed) {
                Some(&id) => id,
                None => {
                    let id = subsets.len() as u32;
                    ids.insert(closed.clone(), id);
                    subsets.push(closed);
                    accepts.push(subsets[id as usize].contains(&accept));
                    iteration_accepts.push(subsets[id as usize].iter().any(|n| marked.contains(n)));
                    transitions.extend(std::iter::repeat_n(u32::MAX, 256));
                    id
                }
            };
            transitions[(worklist - 1) * 256 + byte as usize] = next;
        }
    }

    DfaTable {
        transitions,
        accepts,
        iteration_accepts,
        start: 0,
        state_count: subsets.len() as u32,
    }
}

use std::collections::HashMap;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::grammar::Grammar;

    /// rule = "ab" — a two-byte chain.
    fn grammar_ab() -> Grammar {
        let mut g = Grammar::new();
        let a = g.add_atom(Atom::Str {
            pattern: "a".into(),
        });
        let b = g.add_atom(Atom::Str {
            pattern: "b".into(),
        });
        let body = g.add_atom(Atom::Sequence { atoms: vec![a, b] });
        let rule = g.add_atom(Atom::Named {
            name: "ab".into(),
            atom: body,
        });
        g.root = rule;
        g
    }

    /// rule = ("a" | "b")* — an unbounded loop over disjoint branches.
    fn grammar_ab_star() -> Grammar {
        let mut g = Grammar::new();
        let a = g.add_atom(Atom::Str {
            pattern: "a".into(),
        });
        let b = g.add_atom(Atom::Str {
            pattern: "b".into(),
        });
        let alt = g.add_atom(Atom::Alternative { atoms: vec![a, b] });
        let body = g.add_atom(Atom::Repetition {
            atom: alt,
            min: 0,
            max: None,
            tag: RepetitionTag::Repetition,
        });
        let rule = g.add_atom(Atom::Named {
            name: "abs".into(),
            atom: body,
        });
        g.root = rule;
        g
    }

    /// Walk the whole input and report the last accept end.
    fn walk(table: &DfaTable, input: &[u8]) -> Option<usize> {
        let mut state = table.start;
        let mut last = if table.is_accept(state) {
            Some(0usize)
        } else {
            None
        };
        for (i, &byte) in input.iter().enumerate() {
            match table.step(state, byte) {
                Some(next) => {
                    state = next;
                    if table.is_accept(state) {
                        last = Some(i + 1);
                    }
                }
                None => break,
            }
        }
        last
    }

    #[test]
    fn builds_the_byte_chain_language() {
        let g = grammar_ab();
        let table = build_rule_dfa(&g, g.root).expect("the chain builds");
        assert_eq!(walk(&table, b"ab"), Some(2));
        assert_eq!(walk(&table, b"ax"), None, "stuck past 'a' — no accept");
        assert_eq!(walk(&table, b"a"), None, "partial — no accept yet");
    }

    #[test]
    fn builds_the_loop_language_greedy() {
        let g = grammar_ab_star();
        let table = build_rule_dfa(&g, g.root).expect("the loop builds");
        assert_eq!(walk(&table, b""), Some(0), "min 0: the empty match accepts");
        assert_eq!(walk(&table, b"abab"), Some(4), "the greedy longest");
        assert_eq!(walk(&table, b"abx"), Some(2), "stops at the stuck byte");
    }

    #[test]
    fn refuses_predicates_and_recursion() {
        let mut g = Grammar::new();
        let a = g.add_atom(Atom::Str {
            pattern: "a".into(),
        });
        let la = g.add_atom(Atom::Lookahead {
            atom: a,
            positive: true,
        });
        let body = g.add_atom(Atom::Sequence { atoms: vec![la, a] });
        let rule = g.add_atom(Atom::Named {
            name: "p".into(),
            atom: body,
        });
        g.root = rule;
        assert!(build_rule_dfa(&g, rule).is_none(), "a lookahead refuses");

        let mut g = Grammar::new();
        let slot = g.add_atom(Atom::Sequence { atoms: vec![] });
        let s_x = g.add_atom(Atom::Str {
            pattern: "x".into(),
        });
        let ent = g.add_atom(Atom::Entity { atom: slot });
        let rep = g.add_atom(Atom::Repetition {
            atom: ent,
            min: 0,
            max: Some(1),
            tag: RepetitionTag::Maybe,
        });
        g.atoms[slot] = Atom::Sequence {
            atoms: vec![s_x, rep],
        };
        assert!(build_rule_dfa(&g, slot).is_none(), "recursion refuses");
    }
}

#[cfg(test)]
mod executor_tests {
    use super::*;
    use crate::portable::arena::AstArena;
    use crate::portable::bytecode::instruction::Instruction;
    use crate::portable::bytecode::program::Program;
    use crate::portable::bytecode::vm::parse_with_vm;

    /// The executor-level identity: a program of [DfaRun, End] parses
    /// the same prefix the builder table accepts, consuming exactly the
    /// greedy longest accept and pushing one span.
    #[test]
    fn dfa_run_consumes_the_longest_accept() {
        let mut g = Grammar::new();
        let a = g.add_atom(Atom::Str {
            pattern: "a".into(),
        });
        let b = g.add_atom(Atom::Str {
            pattern: "b".into(),
        });
        let alt = g.add_atom(Atom::Alternative { atoms: vec![a, b] });
        let body = g.add_atom(Atom::Repetition {
            atom: alt,
            min: 1,
            max: None,
            tag: RepetitionTag::Repetition,
        });
        let rule = g.add_atom(Atom::Named {
            name: "run".into(),
            atom: body,
        });
        g.root = rule;
        let table = build_rule_dfa(&g, rule).expect("the loop builds");

        let mut program = Program::new();
        let idx = program.add_dfa_table(table);
        program.add_instruction(Instruction::dfa_run(idx, 1));
        program.add_instruction(Instruction::End);

        let mut arena = AstArena::new();
        let result = parse_with_vm(&program, "ababxx", &mut arena).expect("the parse succeeds");
        assert_eq!(result.end_pos, 4, "the greedy longest accept, xx refused");
        let span = result.value;
        let (offset, length) = match span {
            crate::portable::ast::AstNode::InputRef { offset, length } => (offset, length),
            other => panic!("expected the span value, got {other:?}"),
        };
        assert_eq!((offset, length), (0, 4));

        // Refusal: no accept reachable.
        let mut arena = AstArena::new();
        assert!(
            parse_with_vm(&program, "xx", &mut arena).is_err(),
            "no accept state — the rule fails and the parse rejects"
        );
    }
}
