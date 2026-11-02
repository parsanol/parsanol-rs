//! parsanol-ruby#407: multibyte capture spans must cover whole codepoints.
//!
//! The Ruby-side VM shipped a byte/char unit mismatch at this same grammar
//! shape (quantified negated charset + per-char negated repetition with
//! negative lookaheads): a span stored the match's CHARACTER count where
//! every consumer slices by BYTE, so "Class[éin]" lost é's second
//! continuation byte. The Rust engine's spans are byte-based end to end;
//! this test pins that invariant so the walker, the collapse fold, and the
//! parslet-compat transform can never drift into splitting a codepoint.

use parsanol::portable::arena::AstArena;
use parsanol::portable::ast::AstNode;
use parsanol::portable::grammar::Grammar;
use parsanol::portable::parser::PortableParser;
use parsanol::portable::parslet_transform::to_parslet_compatible;

/// Lutaml::Path-shaped grammar: `segment[inner]` where the segment is a
/// quantified negated charset and the inner text a per-char negated-charset
/// repetition guarded by negative lookaheads (the inner rule excludes `]`
/// so the closing bracket is unambiguous, mirroring how lutaml-path keeps
/// brackets out of its segment text).
fn lpath_shape_grammar_json() -> String {
    let atoms = r#"
    [
      {"Re": {"pattern": "[^\\[\\]:]+"}},
      {"Named": {"name": "name", "atom": 0}},
      {"Str": {"pattern": "["}},
      {"Str": {"pattern": "]"}},
      {"Str": {"pattern": "::"}},
      {"Str": {"pattern": "."}},
      {"Str": {"pattern": "\\"}},
      {"Re": {"pattern": "[^\\]]"}},
      {"Lookahead": {"atom": 4, "positive": false}},
      {"Lookahead": {"atom": 5, "positive": false}},
      {"Lookahead": {"atom": 6, "positive": false}},
      {"Sequence": {"atoms": [8, 9, 10, 7]}},
      {"Str": {"pattern": "\\"}},
      {"Str": {"pattern": "::"}},
      {"Sequence": {"atoms": [12, 13]}},
      {"Str": {"pattern": "\\"}},
      {"Str": {"pattern": "."}},
      {"Sequence": {"atoms": [15, 16]}},
      {"Alternative": {"atoms": [11, 14, 17]}},
      {"Repetition": {"atom": 18, "min": 1, "max": null, "tag": "Repetition"}},
      {"Named": {"name": "content", "atom": 19}},
      {"Sequence": {"atoms": [1, 2, 20, 3]}}
    ]
    "#;
    format!(r#"{{"atoms": {atoms}, "root": 21}}"#)
}

/// Every InputRef span in the tree must land on UTF-8 char boundaries:
/// a span that slices a continuation byte is the #407 corruption shape.
fn assert_spans_char_aligned(node: &AstNode, arena: &AstArena, input: &str) {
    match node {
        AstNode::InputRef { offset, length } => {
            let start = *offset as usize;
            let end = start + *length as usize;
            assert!(
                end <= input.len(),
                "span ({offset}, {length}) exceeds input len {} for {input:?}",
                input.len()
            );
            assert!(
                input.is_char_boundary(start) && input.is_char_boundary(end),
                "span ({offset}, {length}) splits a UTF-8 codepoint in {input:?}"
            );
        }
        AstNode::Array { pool_index, length } => {
            for item in arena.get_array(*pool_index as usize, *length as usize) {
                assert_spans_char_aligned(&item, arena, input);
            }
        }
        AstNode::Hash { pool_index, length } => {
            for (_k, v) in arena.get_hash_items(*pool_index as usize, *length as usize) {
                assert_spans_char_aligned(&v, arena, input);
            }
        }
        _ => {}
    }
}

/// Collect the byte content of every InputRef in the tree.
fn collect_span_bytes(node: &AstNode, arena: &AstArena, input: &str, out: &mut Vec<Vec<u8>>) {
    match node {
        AstNode::InputRef { offset, length } => {
            let start = *offset as usize;
            let end = start + *length as usize;
            out.push(input.as_bytes()[start..end].to_vec());
        }
        AstNode::Array { pool_index, length } => {
            for item in arena.get_array(*pool_index as usize, *length as usize) {
                collect_span_bytes(&item, arena, input, out);
            }
        }
        AstNode::Hash { pool_index, length } => {
            for (_k, v) in arena.get_hash_items(*pool_index as usize, *length as usize) {
                collect_span_bytes(&v, arena, input, out);
            }
        }
        _ => {}
    }
}

fn parse_and_collapse(input: &str) -> (Grammar, AstArena, AstNode) {
    let grammar_json = lpath_shape_grammar_json();
    let grammar = Grammar::from_json(&grammar_json).expect("grammar compiles");
    let mut arena = AstArena::new();
    arena.set_input(input.to_string());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    let raw = parser.parse().expect("parses");
    let collapsed = parsanol::ffi::shared::collapse_ast(&raw, &mut arena);
    (grammar, arena, collapsed)
}

#[test]
fn multibyte_inner_captures_stay_whole() {
    for (inner, input) in [
        ("é", "pkg[é]"),
        ("ü", "pkg[ü]"),
        ("标文本", "pkg[标文本]"),
        ("🙂", "pkg[🙂]"),
        ("éü标🙂in", "pkg[éü标🙂in]"),
        ("éin", "名前[éin]"),
    ] {
        let (_g, arena, collapsed) = parse_and_collapse(input);
        assert_spans_char_aligned(&collapsed, &arena, input);

        let mut spans = Vec::new();
        collect_span_bytes(&collapsed, &arena, input, &mut spans);
        assert!(
            spans.iter().any(|s| s.as_slice() == inner.as_bytes()),
            "inner capture {inner:?} not found byte-exact among spans of {input:?}: {spans:?}"
        );
    }
}

#[test]
fn walker_input_refs_cover_whole_codepoints() {
    // Span audit on the parslet-compat transform for an input whose
    // multibyte chars sit inside both the quantified-charset match and
    // the per-char repetition.
    let input = "cléü[标🙂]";
    let (_g, mut arena, collapsed) = parse_and_collapse(input);
    let transformed = to_parslet_compatible(&collapsed, &mut arena, input);
    assert_spans_char_aligned(&transformed, &arena, input);
}

#[test]
fn parslet_compat_flat_keeps_multibyte_whole() {
    let grammar_json = lpath_shape_grammar_json();
    for input in ["pkg[é]", "pkg[éü标🙂in]", "名前[éin]"] {
        let flat = parsanol::ffi::parse_and_transform_flat(&grammar_json, input).expect("parses");
        // The transformed tree keeps input captures as TAG_STRING
        // (offset, length) wire refs; every referenced span must land on
        // char boundaries and the inner capture must appear byte-exact.
        const TAG_STRING: u64 = 0x04;
        const TAG_HASH_KEY: u64 = 0x09;
        const TAG_INLINE_STRING: u64 = 0x0A;
        const TAG_SYMBOL: u64 = 0x0B;
        let packed_len = |cells: &[u64]| cells[0] as usize;
        let mut spans = Vec::new();
        let mut i = 0;
        while i < flat.len() {
            let consumed = match flat[i] {
                TAG_STRING => {
                    let (offset, length) = (flat[i + 1] as usize, flat[i + 2] as usize);
                    assert!(
                        offset + length <= input.len()
                            && input.is_char_boundary(offset)
                            && input.is_char_boundary(offset + length),
                        "wire span ({offset}, {length}) splits a codepoint or overruns {input:?}"
                    );
                    spans.push(input.as_bytes()[offset..offset + length].to_vec());
                    3
                }
                TAG_HASH_KEY | TAG_INLINE_STRING | TAG_SYMBOL => {
                    // len, then u64 chunks of the payload
                    2 + packed_len(&flat[i + 1..]).div_ceil(8)
                }
                _ => 1,
            };
            i += consumed;
        }
        let expected = &input[input.find('[').expect("bracket") + 1..input.len() - 1];
        assert!(
            spans.iter().any(|s| s.as_slice() == expected.as_bytes()),
            "wire spans {spans:?} lost the inner capture {expected:?} of {input:?}"
        );
    }
}
