//! The wire-atom vocabulary contract: every Atom variant's serde tag
//! and field names, pinned. These strings ARE the compatibility
//! contract — old engines reject unknown tags loudly, but a RENAME
//! would make new engines reject old artifacts' grammars (and
//! vice versa) silently-by-design. A change here must be a conscious
//! shape decision (PARSANOL-SHAPE-v2.md's grammar twin).

use parsanol::portable::grammar::{Atom, LookSpec, RepetitionTag};
use serde_json::json;

#[test]
fn atom_wire_tags_are_pinned() {
    let cases: Vec<(Atom, serde_json::Value)> = vec![
        (
            Atom::Str {
                pattern: "a".into(),
            },
            json!({"Str": {"pattern": "a"}}),
        ),
        (
            Atom::Re {
                pattern: "[a]".into(),
            },
            json!({"Re": {"pattern": "[a]"}}),
        ),
        (
            Atom::Sequence { atoms: vec![0, 1] },
            json!({"Sequence": {"atoms": [0, 1]}}),
        ),
        (
            Atom::Alternative { atoms: vec![2] },
            json!({"Alternative": {"atoms": [2]}}),
        ),
        (
            Atom::Repetition {
                atom: 3,
                min: 0,
                max: Some(1),
                tag: RepetitionTag::Maybe,
            },
            json!({"Repetition": {"atom": 3, "min": 0, "max": 1, "tag": "Maybe"}}),
        ),
        (
            Atom::Named {
                name: "x".into(),
                atom: 4,
            },
            json!({"Named": {"name": "x", "atom": 4}}),
        ),
        (Atom::Entity { atom: 5 }, json!({"Entity": {"atom": 5}})),
        (
            Atom::Lookahead {
                atom: 6,
                positive: false,
            },
            json!({"Lookahead": {"atom": 6, "positive": false}}),
        ),
        (Atom::Cut, json!("Cut")),
        (Atom::Ignore { atom: 7 }, json!({"Ignore": {"atom": 7}})),
        (Atom::Trivia { atom: 8 }, json!({"Trivia": {"atom": 8}})),
        (
            Atom::TriviaCapture {
                atom: 9,
                rules: vec![("--".to_string(), "line".to_string())],
                whitespace: Some("space".to_string()),
            },
            json!({"TriviaCapture": {"atom": 9,
                     "rules": [["--", "line"]], "whitespace": "space"}}),
        ),
        (
            Atom::Constant {
                value: parsanol::portable::grammar::ConstantValue::Int(7),
            },
            json!({"Constant": {"value": {"Int": 7}}}),
        ),
        (
            Atom::Lookbehind {
                look: LookSpec::Regex {
                    source: "[a]".into(),
                },
                positive: true,
            },
            json!({"Lookbehind": {"look": {"Regex": {"source": "[a]"}}, "positive": true}}),
        ),
        (
            Atom::Lookbehind {
                look: LookSpec::Literal {
                    count: 2,
                    pattern: "ab".into(),
                },
                positive: false,
            },
            json!({"Lookbehind": {"look": {"Literal": {"count": 2, "pattern": "ab"}},
                     "positive": false}}),
        ),
    ];

    for (atom, expected) in cases {
        let wire = serde_json::to_value(&atom).expect("serializes");
        assert_eq!(wire, expected, "wire shape drifted for {atom:?}");
        // and the round trip restores the same atom
        let back: Atom = serde_json::from_value(expected.clone()).expect("deserializes");
        assert_eq!(back, atom, "round trip drifted for {atom:?}");
    }
}

// The dynamic/state family rides the same vocabulary; pin their tags
// too — StateSwitch arms and the Custom/CustomRef pair are the
// Ruby-tier contract (#129).
#[test]
fn dynamic_atom_wire_tags_are_pinned() {
    let dynamic = Atom::Dynamic { callback_id: 11 };
    assert_eq!(
        serde_json::to_value(&dynamic).unwrap(),
        json!({"Dynamic": {"callback_id": 11}})
    );
    let set = Atom::StateSet {
        slot: "fence".into(),
        value: Some("1".into()),
        expr: None,
    };
    assert_eq!(
        serde_json::to_value(&set).unwrap(),
        json!({"StateSet": {"slot": "fence", "value": "1", "expr": null}})
    );
    let sm = Atom::StateMatch {
        slot: "fence".into(),
    };
    assert_eq!(
        serde_json::to_value(&sm).unwrap(),
        json!({"StateMatch": {"slot": "fence"}})
    );
    let sw = Atom::StateSwitch {
        slot: "fence".into(),
        arms: [("1".to_string(), "rule_a".to_string())]
            .into_iter()
            .collect(),
        default: None,
    };
    let wire = serde_json::to_value(&sw).unwrap();
    assert_eq!(wire["StateSwitch"]["slot"], json!("fence"));
    let cr = Atom::CustomRef { name: "c".into() };
    assert_eq!(
        serde_json::to_value(&cr).unwrap(),
        json!({"CustomRef": {"name": "c"}})
    );
}
