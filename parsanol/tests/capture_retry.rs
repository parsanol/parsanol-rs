//! Capture writes must be re-applied when memoized values are reused
//! after a failed scoped branch or an isolated lookahead.

use parsanol::portable::dynamic::{register_dynamic_callback, DynamicCallback, DynamicContext};
use parsanol::portable::{AstArena, Atom, Grammar, PortableParser};

struct ClosingTag;

impl DynamicCallback for ClosingTag {
    fn resolve(&self, context: &DynamicContext) -> Option<Atom> {
        let tag = context.captures.get("tag")?;
        Some(Atom::Str {
            pattern: format!("</{}>", tag.get_text(context.input())),
        })
    }

    fn description(&self) -> &str {
        "close the captured opening tag"
    }
}

fn literal(grammar: &mut Grammar, pattern: &str) -> usize {
    grammar.add_atom(Atom::Str {
        pattern: pattern.to_string(),
    })
}

fn tagged_grammar(lookahead: bool) -> (Grammar, usize, usize) {
    let mut grammar = Grammar::new();
    let left = literal(&mut grammar, "<");
    let name_body = grammar.add_atom(Atom::Re {
        pattern: "[a-z]+".to_string(),
    });
    let name = grammar.add_atom(Atom::Entity { atom: name_body });
    let capture = grammar.add_atom(Atom::Capture {
        name: "tag".to_string(),
        atom: name,
    });
    let right = literal(&mut grammar, ">");
    let opening = grammar.add_atom(Atom::Sequence {
        atoms: vec![left, capture, right],
    });
    let content = literal(&mut grammar, "x");
    let close = grammar.add_atom(Atom::Dynamic {
        callback_id: register_dynamic_callback(Box::new(ClosingTag)),
    });
    let body = grammar.add_atom(Atom::Sequence {
        atoms: vec![opening, content, close],
    });
    let retry = if lookahead {
        let peek = grammar.add_atom(Atom::Lookahead {
            atom: opening,
            positive: true,
        });
        grammar.add_atom(Atom::Sequence {
            atoms: vec![peek, body],
        })
    } else {
        let wrong_content = literal(&mut grammar, "q");
        let first = grammar.add_atom(Atom::Sequence {
            atoms: vec![opening, wrong_content, close],
        });
        let first = grammar.add_atom(Atom::Scope { atom: first });
        let second = grammar.add_atom(Atom::Scope { atom: body });
        grammar.add_atom(Atom::Alternative {
            atoms: vec![first, second],
        })
    };
    grammar.root = grammar.add_atom(Atom::Scope { atom: retry });
    (grammar, name, name_body)
}

#[test]
fn shared_capture_is_reapplied_after_failed_scoped_branch() {
    let (grammar, name, name_body) = tagged_grammar(false);
    for input in ["<i>x</i>", "<div>x</div>"] {
        let mut arena = AstArena::new();
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        parser.enable_profiling();
        let result = parser
            .parse_with_end_pos()
            .expect("the retried opening must restore the tag for the callback");
        assert_eq!(result.end_pos, input.len());
        assert!(parser.capture_state().get("tag").is_none());

        // Re-executing the capture must retain memoization of its pure
        // inner value: both branches ask for the name, but its regex
        // body is evaluated only once.
        let counts = parser.dispatch_counts().unwrap();
        assert_eq!(counts[name], 2);
        assert_eq!(counts[name_body], 1);
    }
}

#[test]
fn shared_capture_is_reapplied_after_lookahead_discards_it() {
    let (grammar, _, _) = tagged_grammar(true);
    let input = "<i>x</i>";
    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    let result = parser
        .parse_with_end_pos()
        .expect("lookahead must not suppress the later real capture write");
    assert_eq!(result.end_pos, input.len());
    assert!(parser.capture_state().get("tag").is_none());
}

#[test]
fn shared_capture_still_rejects_a_different_closing_tag() {
    let (grammar, _, _) = tagged_grammar(false);
    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, "<i>x</b>", &mut arena);
    assert!(parser.parse().is_err());
    assert!(parser.capture_state().get("tag").is_none());
}

#[test]
fn capture_isolating_scope_remains_memoized() {
    let mut grammar = Grammar::new();
    let name = literal(&mut grammar, "i");
    let capture = grammar.add_atom(Atom::Capture {
        name: "tag".to_string(),
        atom: name,
    });
    let isolated = grammar.add_atom(Atom::Scope { atom: capture });
    let wrong = literal(&mut grammar, "!");
    let right = literal(&mut grammar, "x");
    let first = grammar.add_atom(Atom::Sequence {
        atoms: vec![isolated, wrong],
    });
    let second = grammar.add_atom(Atom::Sequence {
        atoms: vec![isolated, right],
    });
    grammar.root = grammar.add_atom(Atom::Alternative {
        atoms: vec![first, second],
    });
    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, "ix", &mut arena);
    parser.enable_profiling();
    parser.parse().expect("the second branch must match");
    assert!(parser.capture_state().get("tag").is_none());
    let counts = parser.dispatch_counts().unwrap();
    assert_eq!(counts[isolated], 2);
    assert_eq!(counts[capture], 1);
}
