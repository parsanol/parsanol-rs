//! Explicit portable state matching: source HTML tag comparison and scope.
use parsanol::parg::{checksum_hex, PargArtifact};
use parsanol::portable::{AstArena, Atom, Grammar, PortableParser};
use serde_json::{json, Value};
use std::ffi::{CStr, CString};

fn tag_grammar() -> (Grammar, usize) {
    let mut g = Grammar::new();
    let root = g.add_atom(Atom::Str {
        pattern: String::new(),
    });
    let open = g.add_atom(Atom::Str {
        pattern: "<".into(),
    });
    let name = g.add_atom(Atom::Re {
        pattern: "[A-Za-z][A-Za-z0-9:._-]*".into(),
    });
    let capture = g.add_atom(Atom::Capture {
        name: "tag".into(),
        atom: name,
    });
    let end = g.add_atom(Atom::Str {
        pattern: ">".into(),
    });
    let text = g.add_atom(Atom::Str {
        pattern: "x".into(),
    });
    let child = g.add_atom(Atom::Alternative {
        atoms: vec![root, text],
    });
    let body = g.add_atom(Atom::Repetition {
        atom: child,
        min: 0,
        max: None,
        tag: Default::default(),
    });
    let close = g.add_atom(Atom::Str {
        pattern: "</".into(),
    });
    let compare = g.add_atom(Atom::StateMatch { slot: "tag".into() });
    let sequence = g.add_atom(Atom::Sequence {
        atoms: vec![open, capture, end, body, close, compare, end],
    });
    g.atoms[root] = Atom::Scope { atom: sequence };
    g.root = root;
    (g, compare)
}
fn artifact(ascii: bool) -> String {
    let (g, index) = tag_grammar();
    let mut grammar = serde_json::to_value(g).unwrap();
    if ascii {
        let payload = grammar["atoms"][index]["StateMatch"].take();
        grammar["atoms"][index] = json!({"StateMatchAscii":payload});
    }
    let mut value = json!({"shape":"parsanol-tree/v2","edition":2,"grammar":"StateAsciiSourceProbe","entries":{"main":{"root":"main","grammar":grammar,"bindings":[]}},"tests":[]});
    value["checksum"] = checksum_hex(&value).unwrap().into();
    serde_json::to_string(&value).unwrap()
}
#[test]
fn exact_public_atom_shape_and_mode_changes_preserve_semantics() {
    let (g, index) = tag_grammar();
    let mut arena = AstArena::new();
    let mut p = PortableParser::new(&g, "<Tag>x</tAG>", &mut arena);
    assert!(p.parse().is_err());
    p.set_ascii_state_match_atoms(&[index]).unwrap();
    assert!(p.parse().is_ok());
    p.set_ascii_state_match_atoms(&[]).unwrap();
    assert!(p.parse().is_err());
    assert!(p.set_ascii_state_match_atoms(&[0]).is_err());
}
#[test]
fn explicit_wire_mode_covers_case_boundary_nested_scope_and_recovery() {
    let text = artifact(true);
    let a = PargArtifact::from_json(&text).unwrap();
    assert!(
        a.grammar("main").is_err(),
        "legacy extraction must not drop mode"
    );
    for input in ["<Tag>x</tAG>", "<A><B>x</b></a>", "<A.x_1>x</a.X_1>"] {
        assert!(a.parse_shape("main", input).is_ok(), "{input}");
        assert_eq!(a.parse_shape_recovering("main", input).unwrap()["ok"], true);
    }
    for input in ["<Tag>x</Taggy>", "<A><B>x</a></b>", "<A>x</>", "<A>x</A "] {
        assert!(a.parse_shape("main", input).is_err(), "{input}");
    }
    assert!(a
        .parse_shape_with_compatibility(
            "main",
            "<A><B>x</b></a>",
            parsanol::portable::parser::ParserCompatibility::Parslet
        )
        .is_ok());
    assert!(
        a.grammar("main").is_err(),
        "execution must retain the explicit wire mode"
    );
    let exact = PargArtifact::from_json(&artifact(false)).unwrap();
    assert!(exact.parse_shape("main", "<Tag>x</tAG>").is_err());
    assert!(exact.parse_shape("main", "<Tag>x</Tag>").is_ok());
}
#[test]
fn ascii_policy_does_not_apply_unicode_case_folding() {
    let mut g = Grammar::new();
    let set = g.add_atom(Atom::StateSet {
        slot: "s".into(),
        value: Some("é".into()),
        expr: None,
    });
    let read = g.add_atom(Atom::StateMatch { slot: "s".into() });
    g.root = g.add_atom(Atom::Sequence {
        atoms: vec![set, read],
    });
    for (input, valid) in [("é", true), ("É", false)] {
        let mut arena = AstArena::new();
        let mut p = PortableParser::new(&g, input, &mut arena);
        p.set_ascii_state_match_atoms(&[read]).unwrap();
        assert_eq!(p.parse().is_ok(), valid);
    }
}
#[test]
fn c_abi_keeps_the_explicit_operation_and_owned_result() {
    let text = artifact(true);
    let a = PargArtifact::from_json(&text).unwrap();
    let input = "<Outer><In>x</iN></oUTER>";
    let expected = a.parse_shape("main", input).unwrap();
    let artifact = CString::new(text).unwrap();
    let entry = CString::new("main").unwrap();
    let input = CString::new(input).unwrap();
    let pointer = unsafe {
        parsanol::ffi::c::parsanol_parg_parse_with_compatibility(
            artifact.as_ptr(),
            entry.as_ptr(),
            input.as_ptr(),
            0,
        )
    };
    assert!(!pointer.is_null());
    let actual: Value =
        serde_json::from_slice(unsafe { CStr::from_ptr(pointer) }.to_bytes()).unwrap();
    unsafe { parsanol::ffi::c::parsanol_parg_free(pointer) };
    assert_eq!(actual, json!({"ok":true,"shape":expected}));
}
