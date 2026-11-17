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
pub enum SpineStep {
    /// A Named wrapper: rebuild wraps the inner node in `{name: inner}`.
    Named {
        /// The wrapper's capture name.
        name: String,
    },
    /// A Sequence: the spine repetition sits at `index`; the sibling
    /// atoms before and after are re-parsed fresh on rebuild (their
    /// fixed literals delimit the container).
    Sequence {
        /// The spine repetition's position among the sequence's atoms.
        index: usize,
        /// The atom ids before the repetition (re-parsed fresh on a
        /// splice rebuild — the container's opening delimiters).
        siblings_before: Vec<usize>,
        /// The atom ids after it (the closing delimiters).
        siblings_after: Vec<usize>,
    },
    /// An Alternative: the spine sits in branch `index`. The branch is
    /// confirmed against the previous parse's tree shape before a
    /// splice (a different branch aborts to the full parse — sound).
    Alternative {
        /// The branch the spine sits behind.
        index: usize,
    },
}

/// The spine body plus the structural path from the root down to it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub struct Spine {
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

#[allow(dead_code)] // read by discover_spine (suite consumes the body today)
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
/// One step per EDGE, keyed on the edge's PARENT: how a rebuilt tree
/// nests at that level. Repetition parents pass through (the bounded
/// `[ x ]` form's tree value is the inner's value).
fn path_for(
    grammar: &Grammar,
    came: &HashMap<usize, (usize, usize)>,
    rep_idx: usize,
) -> Vec<SpineStep> {
    let mut chain: Vec<(usize, usize)> = Vec::new(); // (child, pos_in_parent)
    let mut cursor = rep_idx;
    while let Some(&(parent, pos)) = came.get(&cursor) {
        chain.push((cursor, pos));
        cursor = parent;
    }
    chain.reverse();

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
    path
}

/// Every substantial repetition, OUTERMOST first (BFS order), each
/// with its root→repetition structural path. The incremental splice
/// picks the level whose run around the edit is smallest (rs#542):
/// an edit inside a deeply nested member list splices at that list's
/// granularity instead of re-parsing the whole enclosing member.
pub fn discover_spines(grammar: &Grammar) -> Vec<Spine> {
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    // (atom, parent_atom, child_position) — parent None at the root.
    queue.push_back((grammar.root, None::<usize>, 0usize));
    let mut came: HashMap<usize, (usize, usize)> = HashMap::new();
    let mut found: Vec<(usize, usize)> = Vec::new(); // (rep_idx, body)

    while let Some((idx, _parent, _pos)) = queue.pop_front() {
        if !visited.insert(idx) {
            continue;
        }
        match grammar.get_atom(idx) {
            Some(Atom::Repetition { atom, max, .. }) => {
                if max.is_none() && substantial(grammar, *atom) {
                    found.push((idx, *atom));
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

    found
        .into_iter()
        .map(|(rep_idx, body)| Spine {
            body,
            path: path_for(grammar, &came, rep_idx),
        })
        .collect()
}

/// The outermost substantial repetition (BFS first) — the recovery
/// spine. A prefix of `discover_spines`.
pub fn discover_spine(grammar: &Grammar) -> Option<Spine> {
    discover_spines(grammar).into_iter().next()
}

/// The structural path from atom `from` down to repetition atom `to`
/// (BFS, first route wins) — how a rebuilt tree nests when the splice
/// bridges at a NESTED run of a spine body (rs#542 multi-level): the
/// tree surgery descends the previous item by these steps, replaces
/// the list, and reassembles upward.
pub fn path_between(grammar: &Grammar, from: usize, to: usize) -> Option<Vec<SpineStep>> {
    if from == to {
        return Some(Vec::new());
    }
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back(from);
    let mut came: HashMap<usize, (usize, usize)> = HashMap::new();
    let mut hit = false;
    while let Some(idx) = queue.pop_front() {
        if !visited.insert(idx) {
            continue;
        }
        if idx == to {
            hit = true;
            break;
        }
        match grammar.get_atom(idx) {
            Some(Atom::Repetition { atom, .. })
            | Some(Atom::Named { atom, .. })
            | Some(Atom::Entity { atom }) => {
                came.entry(*atom).or_insert((idx, 0));
                queue.push_back(*atom);
            }
            Some(Atom::Ignore { atom })
            | Some(Atom::Trivia { atom })
            | Some(Atom::Capture { atom, .. })
            | Some(Atom::Scope { atom }) => {
                came.entry(*atom).or_insert((idx, 0));
                queue.push_back(*atom);
            }
            Some(Atom::Sequence { atoms }) | Some(Atom::Alternative { atoms }) => {
                for (pos, &a) in atoms.iter().enumerate() {
                    came.entry(a).or_insert((idx, pos));
                    queue.push_back(a);
                }
            }
            _ => {}
        }
    }
    if !hit {
        return None;
    }
    // Walk `to` back to `from`, building the steps keyed on each
    // edge's parent (the same rules as the root path). The stop is
    // `cursor == from`, NOT `came` exhaustion: the grammar is
    // recursive through the spine body itself, so `from` can carry a
    // came entry from a deeper push — walking past it cycles.
    let mut chain: Vec<(usize, usize)> = Vec::new();
    let mut cursor = to;
    while cursor != from {
        let &(parent, pos) = came.get(&cursor)?;
        chain.push((cursor, pos));
        cursor = parent;
    }
    chain.reverse();
    let mut path: Vec<SpineStep> = Vec::new();
    let mut parent = from;
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
    Some(path)
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

    #[test]
    fn spines_orders_outermost_first_with_nested_lists() {
        // Outer member list; each member carries its own nested list.
        // discover_spines[0] is the outer list (the recovery spine);
        // discover_spines[1] is the nested list, its path extending
        // level 0's.
        let mut g = Grammar::new();
        let ws_a = re(&mut g, "[ ]");
        let ws = rep(&mut g, ws_a);
        let key = re(&mut g, "[a-z]+");
        let colon = str_atom(&mut g, ":");
        let comma = str_atom(&mut g, ",");
        let lbrace = str_atom(&mut g, "{");
        let rbrace = str_atom(&mut g, "}");

        // inner list: *("," inner_member); inner_member = [0-9]+
        let num = re(&mut g, "[0-9]+");
        let inner_member = named(&mut g, "num", num);
        let inner_body = g.add_atom(Atom::Sequence {
            atoms: vec![ws, comma, ws, inner_member],
        });
        let inner_list = rep(&mut g, inner_body);
        let object_body = g.add_atom(Atom::Sequence {
            atoms: vec![lbrace, inner_list, rbrace],
        });
        let object = named(&mut g, "object", object_body);

        // outer list: *("," member); member = key ":" object
        let member_body = g.add_atom(Atom::Sequence {
            atoms: vec![key, colon, object],
        });
        let member = named(&mut g, "member", member_body);
        let outer_body = g.add_atom(Atom::Sequence {
            atoms: vec![ws, comma, ws, member],
        });
        let outer_list = rep(&mut g, outer_body);
        let root = g.add_atom(Atom::Sequence {
            atoms: vec![outer_list],
        });
        g.root = root;

        let spines = discover_spines(&g);
        assert_eq!(spines.len(), 2, "outer + nested lists");
        assert_eq!(spines[0].body, outer_body);
        assert_eq!(
            discover_spine(&g).as_ref(),
            spines.first(),
            "recovery spine = the outermost"
        );
        assert_eq!(spines[1].body, inner_body);
        assert!(
            spines[1].path.len() > spines[0].path.len(),
            "the nested path extends the outer one"
        );
        // The shared prefix: the nested path repeats level 0's steps
        // (root → outer list) before descending into the member.
        assert_eq!(
            &spines[1].path[..spines[0].path.len()],
            &spines[0].path[..],
            "nested path shares the outer prefix"
        );

        // path_between from the outer BODY down to the nested rep:
        // the multi-level splice's surgery route. It must reach the
        // nested rep and only descend (no steps for pass-through
        // wrappers). Reps are looked up by body, as the session does
        // (one unbounded repetition per body, shared across nestings).
        let rep_of = |body: usize| {
            g.atoms
                .iter()
                .enumerate()
                .find_map(|(i, a)| match a {
                    Atom::Repetition {
                        atom,
                        max: None,
                        tag: RepetitionTag::Repetition,
                        ..
                    } if *atom == body => Some(i),
                    _ => None,
                })
                .expect("an unbounded rep exists for the body")
        };
        let route = path_between(&g, spines[0].body, rep_of(spines[1].body))
            .expect("the nested rep is reachable from the outer body");
        assert!(!route.is_empty(), "the nested rep is below the outer body");
        // And from the outer body to ITS OWN rep: the recursive
        // grammar (member → value → object → the same list shape)
        // must not cycle the chain walk past the start atom.
        assert_eq!(
            path_between(&g, spines[0].body, rep_of(spines[0].body)),
            None,
            "the outer rep is not a descendant of its own body"
        );
    }
}
