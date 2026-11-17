//! The ByteDispatch cost gate (TODO.perf/11): compile-shape specs.
//!
//! Dispatch tables are a per-NODE win; inside a repetition the choice
//! runs per iteration — often per character (json's `ws = *( %x20 / ... )`)
//! — where the table lookup+jump measured 18x slower than the interleaved
//! charset probes on the rs#533 JSON receipt. The gate emits ByteDispatch
//! only OUTSIDE repetition bodies; these specs pin that shape.
//!
//! Also pins the singleton-class decode: regex-syntax folds `[ ]`-style
//! one-member classes into Literal HIR, which the class decode must
//! still accept (first_set provability and the VM's per-char class fast
//! path both depend on it).

use parsanol::parg::frontend::envelope;
use parsanol::portable::bytecode::compiler::compile;
use parsanol::portable::bytecode::instruction::Instruction;
use parsanol::portable::bytecode::program::Program;

fn compile_program(source: &str) -> Program {
    let envelope = envelope::compile_source(source).expect("grammar compiles");
    let text = serde_json::to_string(&envelope).expect("envelope serializes");
    let artifact = parsanol::PargArtifact::from_json(&text).expect("artifact loads");
    let grammar = artifact.grammar("main").expect("entry decodes");
    compile(grammar).expect("program compiles")
}

fn scan_class_count(program: &Program) -> usize {
    let mut n = 0;
    let mut i = 0;
    while let Some(instr) = program.get_instruction(i) {
        if let Instruction::ScanClass { .. } = instr {
            n += 1;
        }
        i += 1;
    }
    n
}

/// A repetition whose body is a multi-byte-capable class choice (the
/// json string-content shape: ranges, negated classes) lowers to ONE
/// ScanClass bulk run — no per-character choice machinery remains.
#[test]
fn scan_class_lowers_range_choice_repetitions() {
    let source = "grammar Shape version \"1.0.0\" edition 2 {
      ws = *( %x20 / %x09 / %x0A / %x0D )
      content = *( %x20-21 / %x23-5B / %x5D-10FFFF )
      str = %x22 content %x22
      doc = str ws
      entry main: doc
    }";
    let program = compile_program(source);
    assert_eq!(
        scan_class_count(&program),
        1,
        "the range-choice repetition must lower to ONE ScanClass (the bulk exact run)"
    );
    let mut choices = 0;
    let mut i = 0;
    while let Some(instr) = program.get_instruction(i) {
        if let Instruction::Choice { .. } = instr {
            choices += 1;
        }
        i += 1;
    }
    assert_eq!(
        choices, 0,
        "no per-character choice machinery may remain for the class run"
    );

    // Acceptance identical: quoted strings with mixed content parse,
    // non-ASCII rides through, quotes terminate the run.
    let artifact_text = {
        let envelope = envelope::compile_source(source).expect("grammar compiles");
        serde_json::to_string(&envelope).expect("envelope serializes")
    };
    let artifact = parsanol::PargArtifact::from_json(&artifact_text).expect("artifact loads");
    for (input, ok) in [
        ("\"hello\" ", true),
        ("\"a b  c\" ", true),
        ("\"montre\u{e9} \u{4e2d}\" ", true),
        ("\"\" ", true),
        ("\"unterminated ", false),
        ("hello ", false),
    ] {
        let parsed = artifact.parse_shape("main", input).is_ok();
        assert_eq!(
            parsed,
            ok,
            "{input:?} must {}parse",
            if ok { "" } else { "not " }
        );
    }
}

fn dispatch_count(program: &Program) -> usize {
    let mut n = 0;
    let mut i = 0;
    while let Some(instr) = program.get_instruction(i) {
        if let Instruction::ByteDispatch { .. } = instr {
            n += 1;
        }
        i += 1;
    }
    n
}

/// A disjoint multi-branch value-style choice at statement level
/// dispatches; the same choice shape inside a repetition does not.
#[test]
fn dispatch_only_outside_repetitions() {
    let value = "grammar Shape version \"1.0.0\" edition 2 {
  value = ( %x7B / %x5B / %x22 / number / %i\"true\" / %i\"false\" / %i\"null\" ) as value
  number = ( [ %x2D ] 1*%x30-39 ) as number
  entry main: value
}";
    let program = compile_program(value);
    assert!(
        dispatch_count(&program) >= 1,
        "the statement-level disjoint choice must lower to ByteDispatch"
    );

    let repeated = "grammar Shape version \"1.0.0\" edition 2 {
  value = *( %x7B / %x5B / %x22 / %i\"true\" / %i\"false\" / %i\"null\" ) as value
  entry main: value
}";
    let program = compile_program(repeated);
    assert_eq!(
        dispatch_count(&program),
        0,
        "the same choice inside a repetition must stay interleaved (per-char dispatch measured 18x slower on json ws)"
    );
}

/// Case-insensitive literals (?i:... -> Concat of folded classes) are
/// provable first sets, so they participate in dispatch analysis.
#[test]
fn case_folded_literals_are_provable_first_sets() {
    let source = "grammar Shape version \"1.0.0\" edition 2 {
  value = ( %i\"true\" / %i\"false\" / %i\"null\" ) as value
  entry main: value
}";
    let program = compile_program(source);
    assert!(
        dispatch_count(&program) >= 1,
        "case-folded literal branches are disjoint non-nullable and must dispatch"
    );
}

/// Singleton classes survive the class decode: regex-syntax lowers
/// one-member classes to Literal HIR, and the decode must still accept
/// them (the class fast path and first-set provability depend on it).
#[test]
fn singleton_classes_decode_to_ranges() {
    use parsanol::portable::char_class::CharRanges;
    for (pattern, expected) in [
        ("[ ]", 32u8),
        ("[\"]", 34),
        ("[+]", 43),
        ("[0]", 48),
        ("[e]", 101),
    ] {
        let class = CharRanges::from_pattern(pattern)
            .unwrap_or_else(|| panic!("{pattern} must decode as a class"));
        let first = class.first_bytes();
        assert_eq!(first, vec![expected], "{pattern} first bytes");
    }
}

/// A single-byte choice inside a repetition (json's ws shape) merges to
/// ONE charset — same bytes consumed, same span values, so it is sound
/// at any depth (unlike ByteDispatch). The ws rule compiles with no
/// Choice machinery and the merged grammar accepts exactly the same
/// inputs.
#[test]
fn single_byte_choices_merge_to_charsets_inside_repetitions() {
    let source = "grammar Shape version \"1.0.0\" edition 2 {
      ws = *( %x20 / %x09 / %x0A / %x0D )
      doc = ws \"a\" ws
      entry main: doc
    }";
    let program = compile_program(source);
    let mut spans = 0;
    let mut choices = 0;
    let mut i = 0;
    while let Some(instr) = program.get_instruction(i) {
        match instr {
            Instruction::Span { .. } => spans += 1,
            Instruction::Choice { .. } => choices += 1,
            _ => {}
        }
        i += 1;
    }
    assert_eq!(
        spans, 1,
        "the ws single-byte choice must lower to ONE Span (the bulk run)"
    );
    assert_eq!(
        choices, 0,
        "no per-character choice machinery may remain (the loop's own iteration Choice included — the Span run replaces it)"
    );

    // acceptance identical: the merged grammar still parses 'a' with
    // mixed whitespace and rejects non-whitespace around it
    let artifact_text = {
        let envelope = envelope::compile_source(source).expect("grammar compiles");
        serde_json::to_string(&envelope).expect("envelope serializes")
    };
    let artifact = parsanol::PargArtifact::from_json(&artifact_text).expect("artifact loads");
    for (input, ok) in [
        ("  a  ", true),
        ("\ta\n", true),
        ("b a", false),
        ("a", true),
    ] {
        let parsed = artifact.parse_shape("main", input).is_ok();
        assert_eq!(
            parsed,
            ok,
            "{input:?} must {}parse",
            if ok { "" } else { "not " }
        );
    }
}
