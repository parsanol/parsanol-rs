//! The recovery/splice spine: the grammar's list structure — the
//! first unbounded repetition (breadth-first from the root) whose body
//! anchors at fixed literals and is not a char matcher. The RECOVERY
//! rounds replay iterations of this body (rs#539); the INCREMENTAL
//! splice grafts previous iterations and re-parses only the edited
//! region (rs#542 slice 2b). Both need the same discovery, and the
//! splice additionally needs the STRUCTURAL PATH from the root to the
//! spine — the wrapper atoms a rebuilt tree must reproduce.

use crate::portable::grammar::{Atom, Grammar};
use std::collections::{HashMap, HashSet, VecDeque};

/// One step on the root→spine path: how a rebuilt tree reproduces the
/// wrapper at this level. The splice (rs#542 slice 2b) consumes the
/// steps to rebuild the wrappers above the spine array; until that
/// lands only the discovery runs, so the step shapes sit quiet.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum SpineStep {
    /// A Named wrapper: rebuild wraps the inner node in `{name: inner}`.
    Named { name: String },
    /// A Sequence: the spine repetition sits at `index`; the sibling
    /// atoms before and after are re-parsed fresh on rebuild (their
    /// fixed literals delimit the container).
    Sequence {
        index: usize,
        siblings_before: Vec<usize>,
        siblings_after: Vec<usize>,
    },
    /// An Alternative: the spine sits in branch `index`. The branch is
    /// confirmed against the previous parse's tree shape before a
    /// splice (a different branch aborts to the full parse — sound).
    Alternative { index: usize },
}

/// The spine body plus the structural path from the root down to it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) struct Spine {
    /// The spine repetition's body atom — the iteration granularity.
    pub body: usize,
    /// Root-to-spine steps; empty when the root itself is the spine
    /// repetition (the classic splice shape).
    pub path: Vec<SpineStep>,
}

/// Substantial body: anchors at fixed literals (":" "," "\"") and is
/// not a char matcher. A bare class run (`*digit`) or an alternative
/// offering a bare class branch (a string's interior run) "matches" at
/// every offset — the trial scan would stall byte-by-byte.
#[allow(dead_code)] // read via substantial() and the splice predicates
fn anchors_iteration(grammar: &Grammar, idx: usize, seen: &mut HashSet<usize>) -> bool {
    if !seen.insert(idx) {
        return false;
    }
    match grammar.get_atom(idx) {
        Some(Atom::Str { .. }) => true,
        Some(Atom::Entity { atom }) | Some(Atom::Named { atom, .. }) => {
            anchors_iteration(grammar, *atom, seen)
        }
        Some(Atom::Sequence { atoms }) | Some(Atom::Alternative { atoms }) => {
            atoms.iter().any(|&a| anchors_iteration(grammar, a, seen))
        }
        Some(Atom::Repetition { atom, .. })
        | Some(Atom::Ignore { atom })
        | Some(Atom::Trivia { atom })
        | Some(Atom::Capture { atom, .. })
        | Some(Atom::Scope { atom }) => anchors_iteration(grammar, *atom, seen),
        _ => false,
    }
}

/// A char matcher: a bare class/regex, or an alternative offering one
/// as a branch. Wrapper atoms resolve through; a Sequence with a fixed
/// literal is never one.
#[allow(dead_code)] // read via substantial()
fn char_matcher_body(grammar: &Grammar, idx: usize, seen: &mut HashSet<usize>) -> bool {
    if !seen.insert(idx) {
        return false;
    }
    match grammar.get_atom(idx) {
        Some(Atom::Re { .. }) | Some(Atom::Dynamic { .. }) => true,
        Some(Atom::Entity { atom }) | Some(Atom::Named { atom, .. }) => {
            char_matcher_body(grammar, *atom, seen)
        }
        Some(Atom::Alternative { atoms }) => {
            atoms.iter().any(|&a| char_matcher_body(grammar, a, seen))
        }
        Some(Atom::Repetition { atom, .. })
        | Some(Atom::Ignore { atom })
        | Some(Atom::Trivia { atom })
        | Some(Atom::Capture { atom, .. })
        | Some(Atom::Scope { atom }) => char_matcher_body(grammar, *atom, seen),
        _ => false,
    }
}

fn substantial(grammar: &Grammar, body: usize) -> bool {
    anchors_iteration(grammar, body, &mut HashSet::new())
        && !char_matcher_body(grammar, body, &mut HashSet::new())
}

/// Breadth-first: the outermost list wins — a grammar nesting both an
/// object member list and an array element list recovers/splices at
/// the member granularity (it also carries the enclosing object's
/// separators). The path records how the root reaches the spine; an
/// alternative on the path is recorded as a branch index the splice
/// confirms against the previous tree.
pub(crate) fn discover_spine(grammar: &Grammar) -> Option<Spine> {
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    // (atom, parent_atom, child_position) — parent None at the root.
    queue.push_back((grammar.root, None::<usize>, 0usize));
    let mut came: HashMap<usize, (usize, usize)> = HashMap::new();
    let mut substantial_at: HashMap<usize, usize> = HashMap::new();

    while let Some((idx, _parent, _pos)) = queue.pop_front() {
        if !visited.insert(idx) {
            continue;
        }
        match grammar.get_atom(idx) {
            Some(Atom::Repetition { atom, max, .. }) => {
                if max.is_none() && substantial(grammar, *atom) {
                    substantial_at.insert(idx, *atom);
                    // The FIRST hit wins (BFS order decides), stop.
                    break;
                }
                came.entry(*atom).or_insert((idx, 0));
                queue.push_back((*atom, Some(idx), 0));
            }
            Some(Atom::Named { atom, .. }) => {
                came.entry(*atom).or_insert((idx, 0));
                queue.push_back((*atom, Some(idx), 0));
            }
            Some(Atom::Entity { atom }) => {
                came.entry(*atom).or_insert((idx, 0));
                queue.push_back((*atom, Some(idx), 0));
            }
            Some(Atom::Ignore { atom })
            | Some(Atom::Trivia { atom })
            | Some(Atom::Capture { atom, .. })
            | Some(Atom::Scope { atom }) => {
                came.entry(*atom).or_insert((idx, 0));
                queue.push_back((*atom, Some(idx), 0));
            }
            Some(Atom::Sequence { atoms }) => {
                for (pos, &a) in atoms.iter().enumerate() {
                    came.entry(a).or_insert((idx, pos));
                    queue.push_back((a, Some(idx), pos));
                }
            }
            Some(Atom::Alternative { atoms }) => {
                for (pos, &a) in atoms.iter().enumerate() {
                    came.entry(a).or_insert((idx, pos));
                    queue.push_back((a, Some(idx), pos));
                }
            }
            _ => {}
        }
    }

    let (rep_idx, body) = {
        let (idx, atom) = substantial_at.iter().next()?;
        (*idx, *atom)
    };
    // The BFS break lands on the first-substantial repetition; walk
    // the came[] chain back to the root, building the path forward.
    let mut chain: Vec<(usize, usize)> = Vec::new(); // (child, pos_in_parent)
    let mut cursor = rep_idx;
    loop {
        match came.get(&cursor) {
            Some(&(parent, pos)) => {
                chain.push((cursor, pos));
                cursor = parent;
            }
            None => break, // the root
        }
    }
    chain.reverse();

    // One step per EDGE, keyed on the edge's PARENT: how the rebuilt
    // tree nests at that level. Repetition parents pass through (the
    // bounded `[ x ]` form's tree value is the inner's value).
    let mut path: Vec<SpineStep> = Vec::new();
    let mut parent = grammar.root;
    for (child, pos) in &chain {
        match grammar.get_atom(parent) {
            Some(Atom::Named { name, .. }) => {
                path.push(SpineStep::Named { name: name.clone() });
            }
            Some(Atom::Entity { .. })
            | Some(Atom::Ignore { .. })
            | Some(Atom::Trivia { .. })
            | Some(Atom::Capture { .. })
            | Some(Atom::Scope { .. }) => {}
            Some(Atom::Sequence { atoms }) => {
                path.push(SpineStep::Sequence {
                    index: *pos,
                    siblings_before: atoms[..*pos].to_vec(),
                    siblings_after: atoms[*pos + 1..].to_vec(),
                });
            }
            Some(Atom::Alternative { .. }) => {
                path.push(SpineStep::Alternative { index: *pos });
            }
            Some(Atom::Repetition { .. }) => {}
            _ => {}
        }
        parent = *child;
    }
    Some(Spine { body, path })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::grammar::RepetitionTag;

    fn rep(g: &mut Grammar, body: usize) -> usize {
        g.add_atom(Atom::Repetition {
            atom: body,
            min: 0,
            max: None,
            tag: RepetitionTag::Repetition,
        })
    }
    fn re(g: &mut Grammar, pattern: &str) -> usize {
        g.add_atom(Atom::Re {
            pattern: pattern.to_string(),
        })
    }
    fn str_atom(g: &mut Grammar, s: &str) -> usize {
        g.add_atom(Atom::Str {
            pattern: s.to_string(),
        })
    }
    fn named(g: &mut Grammar, name: &str, atom: usize) -> usize {
        g.add_atom(Atom::Named {
            name: name.to_string(),
            atom,
        })
    }

    #[test]
    fn root_spine_has_an_empty_path() {
        // document = pair* (the classic splice shape).
        let mut g = Grammar::new();
        let key = re(&mut g, "[a-z]+");
        let eq = str_atom(&mut g, "=");
        let val = re(&mut g, "[0-9]+");
        let nl = str_atom(&mut g, "\n");
        let pair = g.add_atom(Atom::Sequence {
            atoms: vec![key, eq, val, nl],
        });
        let root = rep(&mut g, pair);
        g.root = root;

        let spine = discover_spine(&g).expect("kv has a spine");
        assert_eq!(spine.body, pair);
        assert!(spine.path.is_empty(), "the root repetition: no path");
    }

    #[test]
    fn nested_member_list_carries_the_structural_path() {
        // json-shaped: root = Sequence[ws, Named(value), ws]; value =
        // Alternative[Named(object)]; object = Sequence["{", ws,
        // Repetition{max:1}[Sequence[Named(member), rep]]] as object;
        // rep = Sequence[ws, ",", ws, Named(member)].
        let mut g = Grammar::new();
        let a = re(&mut g, "[ ]");
        let ws_body = rep(&mut g, a);
        let comma = str_atom(&mut g, ",");
        let m0 = re(&mut g, "[a-z]+");
        let member = named(&mut g, "member", m0);
        let list_body = g.add_atom(Atom::Sequence {
            atoms: vec![ws_body, comma, ws_body, member],
        });
        let list = rep(&mut g, list_body);
        let bracketed = rep(&mut g, list); // max 1 via a wrapping rep? no —
                                           // bounded form: Repetition{max: Some(1)}
        let bounded = g.add_atom(Atom::Repetition {
            atom: list,
            min: 0,
            max: Some(1),
            tag: RepetitionTag::Repetition,
        });
        let _ = bracketed;
        let lbrace = str_atom(&mut g, "{");
        let rbrace = str_atom(&mut g, "}");
        let object_body = g.add_atom(Atom::Sequence {
            atoms: vec![lbrace, ws_body, bounded, ws_body, rbrace],
        });
        let object = named(&mut g, "object", object_body);
        let value = named(&mut g, "value", object);
        let root = g.add_atom(Atom::Sequence {
            atoms: vec![ws_body, value, ws_body],
        });
        g.root = root;

        let spine = discover_spine(&g).expect("the member list is the spine");
        assert_eq!(spine.body, list_body);
        // The exact rebuild recipe: root Sequence (index 1, the ws
        // siblings) → the value Named → the object Named → the object
        // Sequence ("{" + ws before, ws + "}" after); the bounded
        // [ member *("," member) ] passes through without a step.
        assert_eq!(
            spine.path,
            vec![
                SpineStep::Sequence {
                    index: 1,
                    siblings_before: vec![ws_body],
                    siblings_after: vec![ws_body],
                },
                SpineStep::Named {
                    name: "value".to_string(),
                },
                SpineStep::Named {
                    name: "object".to_string(),
                },
                SpineStep::Sequence {
                    index: 2,
                    siblings_before: vec![lbrace, ws_body],
                    siblings_after: vec![ws_body, rbrace],
                },
            ],
            "the exact structural path"
        );
    }

    #[test]
    fn char_runs_do_not_hijack_the_spine() {
        // int = %x30 / (%x31-39 *digit) inside a member-list grammar:
        // the digit run must not become the spine.
        let mut g = Grammar::new();
        let digit = re(&mut g, "[0-9]");
        let digits = rep(&mut g, digit);
        let zero = re(&mut g, "[0]");
        let int = g.add_atom(Atom::Alternative {
            atoms: vec![zero, digits],
        });
        let comma = str_atom(&mut g, ",");
        let item = named(&mut g, "item", int);
        let list_body = g.add_atom(Atom::Sequence {
            atoms: vec![comma, item],
        });
        let list = rep(&mut g, list_body);
        g.root = list;

        let spine = discover_spine(&g).expect("the list wins");
        assert_eq!(spine.body, list_body);
    }
}
