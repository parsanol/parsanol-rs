//! Trivia-drain channel discipline (parsanol-ruby#180 round 3):
//! units recorded by a whitespace TriviaCapture attach to the next
//! successful Named exactly once, at their true input offset, even
//! when keyword alternatives contain empty-matching Named captures
//! that drain them before failing (expressir's tABS/tABSTRACT/...
//! keyword ladder), and across top-level retries that would replay
//! memoized outcomes.

use parsanol::portable::arena::AstArena;
use parsanol::portable::ast::AstNode;
use parsanol::portable::grammar::Grammar;
use parsanol::portable::parser::PortableParser;

/// The regression grammar, as wire atoms:
///
/// ```text
/// 0  Re      [ \t]          (ws char)
/// 1  Rep     0,∞ of 0       (ws run)
/// 2  TriviaCapture ws of 1  (the skip wrapper)
/// 3  Named   "spaces" of 4  (empty-matching own capture)
/// 4  Re      [ \t]*         (matches empty!)
/// 5  Named   "kwABS" of 6   (first keyword alternative)
/// 6  Str     "ABS"
/// 7  Named   "word" of 8    (the real branch)
/// 8  Re      [a-z]+
/// 9  Alt     [3+5 seq, 7]   (keyword ladder)
/// 10 Seq     [2, 9]         (wrapper, ladder)
/// 11 Named   "stmt" of 10
/// 12 Seq     [11, 13]       (first entry arm: stmt then "!")
/// 13 Str     "!"
/// 14 Seq     [11, 15]       (second entry arm: stmt then ";")
/// 15 Str     ";"
/// 16 Alt     [12, 14]       (entry: arm 1 fails at the end)
/// ```
fn regression_grammar() -> String {
    // 0  Re "[ \t]"
    // 1  Rep(0, 1, null)             ws+ (skip body)
    // 2  TriviaCapture(1, "space")   the wrapper
    // 3  Rep(0, 0, null)             ws* — empty-capable
    // 4  Named(spaces, 3)            drains then survives a failed branch
    // 5  Str "ABS"
    // 6  Seq[4, 5]
    // 7  Named(kwABS, 6)             keyword rule; its failure restores
    // 8  Re "[a-z]"
    // 9  Rep(8, 1, null)
    // 10 Named(word, 9)
    // 11 Alternative[7, 10]
    // 12 Seq[2, 11]
    // 13 Named(stmt, 12)
    // 14 Named(bang, 15)
    // 15 Str "!"
    // 16 Seq[13, 14]                 entry arm 1 (fails at the tail)
    // 17 Named(semi, 18)
    // 18 Str ";"
    // 19 Seq[13, 17]                entry arm 2 (stmt, semi)
    // 20 Alternative[16, 19]         ROOT
    let atoms = serde_json::json!([
        { "Re": { "pattern": "[ \\t]" } },
        { "Repetition": { "atom": 0, "min": 1, "max": null, "tag": "Repetition" } },
        { "TriviaCapture": { "atom": 1, "rules": [], "whitespace": "space" } },
        { "Repetition": { "atom": 0, "min": 0, "max": null, "tag": "Repetition" } },
        { "Named": { "name": "spaces", "atom": 3 } },
        { "Str": { "pattern": "ABS" } },
        { "Sequence": { "atoms": [4, 5] } },
        { "Named": { "name": "kwABS", "atom": 6 } },
        { "Re": { "pattern": "[a-z]" } },
        { "Repetition": { "atom": 8, "min": 1, "max": null, "tag": "Repetition" } },
        { "Named": { "name": "word", "atom": 9 } },
        { "Alternative": { "atoms": [7, 10] } },
        { "Sequence": { "atoms": [2, 11] } },
        { "Named": { "name": "stmt", "atom": 12 } },
        { "Named": { "name": "bang", "atom": 15 } },
        { "Str": { "pattern": "!" } },
        { "Sequence": { "atoms": [13, 14] } },
        { "Named": { "name": "semi", "atom": 18 } },
        { "Str": { "pattern": ";" } },
        { "Sequence": { "atoms": [13, 17] } },
        { "Alternative": { "atoms": [16, 19] } },
    ]);
    serde_json::json!({ "atoms": atoms, "root": 20 }).to_string()
}

fn comments_of(node: &AstNode, arena: &AstArena, out: &mut Vec<String>) {
    if let AstNode::Hash { pool_index, length } = node {
        for (key, value) in arena.get_hash_items(*pool_index as usize, *length as usize) {
            if key == "comments" {
                if let AstNode::Array { pool_index, length } = value {
                    for unit in arena.get_array(pool_index as usize, length as usize) {
                        if let AstNode::Hash { pool_index, length } = unit {
                            for (kind, text) in
                                arena.get_hash_items(pool_index as usize, length as usize)
                            {
                                let offset = match text {
                                    AstNode::InputRef { offset, .. } => offset,
                                    other => unreachable!("unit text node {other:?}"),
                                };
                                out.push(format!("{kind}:{offset}"));
                            }
                        }
                    }
                }
            }
            comments_of(&value, arena, out);
        }
    } else if let AstNode::Array { pool_index, length } = node {
        for item in arena.get_array(*pool_index as usize, *length as usize) {
            comments_of(&item, arena, out);
        }
    }
}

#[test]
fn a_unit_drained_by_a_failed_branches_empty_named_survives_and_attaches_once() {
    let grammar = Grammar::from_json(&regression_grammar()).expect("grammar compiles");
    let input = " alpha;";
    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    let result = parser.parse_with_end_pos().expect("parses");
    assert_eq!(result.end_pos, input.len());

    let mut units = Vec::new();
    comments_of(&result.value, &arena, &mut units);
    // Exactly one unit, attached to `word` (inside stmt), at its true
    // offset — not lost to the failed keyword branch's empty `spaces`
    // Named, not double-attached across the entry-arm retry.
    assert_eq!(units, vec!["space:0".to_string()]);
}
