//! parsanol-ruby#83 minimal shape: rep(1) >> maybe(seq) under a named
//! capture — native collapses the single-element repetition array.
use parsanol::portable::arena::AstArena;
use parsanol::portable::grammar::{Atom, Grammar, RepetitionTag};
use parsanol::portable::parser::PortableParser;

fn build() -> Grammar {
    let mut g = Grammar::new();
    let text = g.add_atom(Atom::Re {
        pattern: "[^\\n]+".to_string(),
    });
    let lb = g.add_atom(Atom::Str {
        pattern: "\n".to_string(),
    });
    // line = text.as(:text) >> lb.as(:line_break)
    let nt = g.add_atom(Atom::Named {
        name: "text".into(),
        atom: text,
    });
    let nl = g.add_atom(Atom::Named {
        name: "line_break".into(),
        atom: lb,
    });
    let line = g.add_atom(Atom::Sequence {
        atoms: vec![nt, nl],
    });
    // rep1 = line.repeat(1)
    let rep1 = g.add_atom(Atom::Repetition {
        atom: line,
        min: 1,
        max: None,
        tag: RepetitionTag::Repetition,
    });
    // inner2 = line.repeat(1,1) >> lb.as(:line_break)   (the maybe body)
    let lb2 = g.add_atom(Atom::Named {
        name: "line_break".into(),
        atom: lb,
    });
    let rep11 = g.add_atom(Atom::Repetition {
        atom: line,
        min: 1,
        max: Some(1),
        tag: RepetitionTag::Repetition,
    });
    let seq2 = g.add_atom(Atom::Sequence {
        atoms: vec![rep11, lb2],
    });
    let maybe2 = g.add_atom(Atom::Repetition {
        atom: seq2,
        min: 0,
        max: Some(1),
        tag: RepetitionTag::Maybe,
    });
    // lines = seq(rep1, maybe2) as :lines
    let seq = g.add_atom(Atom::Sequence {
        atoms: vec![rep1, maybe2],
    });
    let named = g.add_atom(Atom::Named {
        name: "lines".into(),
        atom: seq,
    });
    g.root = named;
    g
}

fn dump(label: &str, node: &parsanol::portable::ast::AstNode, arena: &AstArena, input: &str) {
    use parsanol::portable::ast::AstNode;
    match node {
        AstNode::InputRef { offset, length } => {
            print!(
                "{}({:?})",
                label,
                &input[*offset as usize..*offset as usize + *length as usize]
            )
        }
        AstNode::StringRef { pool_index } => {
            print!("{}({:?})", label, arena.get_string(*pool_index as usize))
        }
        AstNode::Array { pool_index, length } => {
            print!("{}[", label);
            for c in arena.get_array(*pool_index as usize, *length as usize) {
                dump("", &c, arena, input);
                print!(" ");
            }
            print!("]")
        }
        AstNode::Hash { pool_index, length } => {
            print!("{}{{", label);
            for (k, v) in arena.get_hash_items(*pool_index as usize, *length as usize) {
                print!("{}=>", k);
                dump("", &v, arena, input);
                print!(" ")
            }
            print!("}}")
        }
        other => print!("{}({:?})", label, other),
    }
}

fn main() {
    let grammar = build();
    let input = " image::pic.png[caption, 200]\n";
    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    let raw = parser.parse().expect("parse");
    dump("RAW: ", &raw, &arena, input);
    println!();

    let mut arena2 = AstArena::new();
    let mut parser = PortableParser::new(&grammar, input, &mut arena2);
    let raw2 = parser.parse().expect("parse");
    let norm =
        parsanol::portable::parslet_transform::to_parslet_compatible(&raw2, &mut arena2, input);
    dump("NORM: ", &norm, &arena2, input);
    println!();
}
