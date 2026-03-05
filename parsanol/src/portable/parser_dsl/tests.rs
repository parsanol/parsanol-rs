use super::*;

#[test]
fn test_simple_grammar() {
    let grammar = GrammarBuilder::new()
        .rule("hello", str("hello"))
        .rule("world", str("world"))
        .build();
    assert_eq!(grammar.atom_count(), 2);
}

#[test]
fn test_sequence_grammar() {
    let grammar = GrammarBuilder::new()
        .rule("greeting", str("hello").then(str("world")))
        .build();
    assert!(grammar.atom_count() > 0);
}

#[test]
fn test_choice_grammar() {
    let grammar = GrammarBuilder::new()
        .rule("op", str("+").or(str("-")))
        .build();
    assert!(grammar.atom_count() > 0);
}

#[test]
fn test_repetition() {
    let grammar = GrammarBuilder::new()
        .rule("digits", re("[0-9]").many1())
        .build();
    assert!(grammar.atom_count() > 0);
}

#[test]
fn test_named() {
    let grammar = GrammarBuilder::new()
        .rule("num", re("[0-9]+").label("value"))
        .build();
    assert!(grammar.atom_count() > 0);
}

#[test]
fn test_recursive_grammar() {
    // Expression grammar (recursive)
    // Using dynamic() for heterogeneous types
    let grammar = GrammarBuilder::new()
        .rule(
            "expr",
            choice(vec![
                dynamic(seq(vec![
                    dynamic(ref_("term")),
                    dynamic(ref_("op")),
                    dynamic(ref_("expr")),
                ])),
                dynamic(ref_("term")),
            ]),
        )
        .rule(
            "term",
            choice(vec![
                dynamic(seq(vec![
                    dynamic(str("(")),
                    dynamic(ref_("expr")),
                    dynamic(str(")")),
                ])),
                dynamic(ref_("number")),
            ]),
        )
        .rule("number", re("[0-9]+"))
        .rule(
            "op",
            choice(vec![
                dynamic(str("+")),
                dynamic(str("-")),
                dynamic(str("*")),
                dynamic(str("/")),
            ]),
        )
        .build();
    assert!(grammar.atom_count() > 0);
}

#[test]
fn test_macro_grammar() {
    let grammar = grammar! {
        "hello" => str("hello"),
        "world" => str("world"),
    };
    assert_eq!(grammar.atom_count(), 2);
}

#[test]
fn test_sequence3_types() {
    // Test Sequence3 with .then() chaining
    let grammar = GrammarBuilder::new()
        .rule("triple", str("a").then(str("b")).then(str("c")))
        .build();
    assert!(grammar.atom_count() > 0);
}

#[test]
fn test_sequence5_types() {
    // Test Sequence5 with .then() chaining
    let grammar = GrammarBuilder::new()
        .rule(
            "quint",
            str("a")
                .then(str("b"))
                .then(str("c"))
                .then(str("d"))
                .then(str("e")),
        )
        .build();
    assert!(grammar.atom_count() > 0);
}

#[test]
fn test_operator_shr_sequence() {
    // Test >> operator for chaining sequences
    use std::ops::Shr;

    // str("a") >> str("b") should produce Sequence2
    // We can then chain with >> to get Sequence3, etc.
    let seq2 = str("a").then(str("b"));
    let seq3 = seq2.shr(str("c"));
    let seq4 = seq3.shr(str("d"));
    let seq5 = seq4.shr(str("e"));

    let grammar = GrammarBuilder::new().rule("seq5", seq5).build();
    assert!(grammar.atom_count() > 0);
}

#[test]
fn test_operator_bitor_alternative() {
    // Test | operator for chaining alternatives
    use std::ops::BitOr;

    // str("a") | str("b") should produce Alternative2 via .or()
    // We can then chain with | to get Alternative3, etc.
    let alt2 = str("a").or(str("b"));
    let alt3 = alt2.bitor(str("c"));
    let alt4 = alt3.bitor(str("d"));
    let alt5 = alt4.bitor(str("e"));

    let grammar = GrammarBuilder::new().rule("alt5", alt5).build();
    assert!(grammar.atom_count() > 0);
}

#[test]
fn test_grammar_import_basic() {
    // Create a simple grammar
    let inner_grammar = GrammarBuilder::new().rule("value", str("hello")).build();

    // Import it into another grammar
    let mut builder = GrammarBuilder::new();
    builder.import(&inner_grammar, Some("inner"));

    // Check that atoms were added
    let import_map = builder.last_import().unwrap();
    assert_eq!(import_map.rule_count, inner_grammar.atom_count());

    // Build the combined grammar
    let combined = builder.build();
    assert_eq!(combined.atom_count(), inner_grammar.atom_count());
}

#[test]
fn test_grammar_import_with_own_rules() {
    // Create a JSON-like value grammar
    let value_grammar = GrammarBuilder::new()
        .rule("null", str("null"))
        .rule("true", str("true"))
        .rule("false", str("false"))
        .build();

    // Import and add new rules (using mutable style)
    let mut builder = GrammarBuilder::new();
    builder.rule_mut("prefix", str("VALUE:"));
    builder.import(&value_grammar, Some("json"));
    builder.rule_mut("wrapped", str("[").then(str("]")));
    let combined = builder.build();

    // Should have atoms from both grammars
    assert!(combined.atom_count() > value_grammar.atom_count());
}

#[test]
fn test_import_map_index_translation() {
    let grammar = GrammarBuilder::new()
        .rule("a", str("a"))
        .rule("b", str("b"))
        .build();

    let mut builder = GrammarBuilder::new();
    builder.rule_mut("x", str("x")); // Add one rule first
    builder.import(&grammar, None);

    let import_map = builder.last_import().unwrap();
    // Old index 0 should map to offset
    assert_eq!(import_map.map_index(0), import_map.offset);
    assert_eq!(import_map.map_index(1), import_map.offset + 1);
}

#[test]
fn test_grammar_import_nested_atoms() {
    // Create a grammar with nested atoms (sequence)
    let nested_grammar = GrammarBuilder::new()
        .rule("pair", str("key").then(str(":")).then(str("value")))
        .rule("options", str("a").or(str("b")).or(str("c")))
        .build();

    // Import into new grammar
    let mut builder = GrammarBuilder::new();
    builder.import(&nested_grammar, Some("nested"));
    builder.rule_mut("main", str("test"));
    let combined = builder.build();

    // All nested atoms should be remapped correctly
    assert!(combined.atom_count() >= nested_grammar.atom_count());
}

#[test]
fn test_import_with_repetition() {
    // Create a grammar with repetition
    let repeat_grammar = GrammarBuilder::new()
        .rule("digits", re("[0-9]+"))
        .rule("many", ref_("digits").repeat(1, None))
        .build();

    let mut builder = GrammarBuilder::new();
    builder.import(&repeat_grammar, Some("rep"));
    let combined = builder.build();

    // Repetition indices should be remapped
    assert!(combined.atom_count() >= repeat_grammar.atom_count());
}
