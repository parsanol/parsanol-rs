//! A single-codepoint terminal (`%x5E`, `%xA0`, `(%x0B / %x0C)`,
//! `(%x3C %x3C)`) compiles to a regex atom whose HIR is a literal, so
//! it never reached the class fast path and was matched by an
//! unanchored search. A failed attempt then scanned to the end of the
//! input whenever the character did not occur later: quadratic under
//! `*(!(r) ANY)`. The atom must be matched anchored at the position.

use parsanol::parg::frontend::envelope;
use parsanol::parg::PargArtifact;
use std::time::{Duration, Instant};

fn load(source: &str) -> PargArtifact {
    let envelope = envelope::compile_source(source).expect("grammar compiles");
    let text = serde_json::to_string(&envelope).expect("envelope serializes");
    PargArtifact::from_json(&text).expect("artifact loads")
}

fn grammar(terminal: &str) -> String {
    format!(
        "grammar G version \"1.0.0\" edition \"v2\" {{\n  r = {terminal} \"x\"\n  \
         doc = *(!(r) ANY)\n  hit = r\n  entry doc: doc\n  entry hit: hit\n}}\n"
    )
}

fn best_of_three(artifact: &PargArtifact, input: &str) -> Duration {
    (0..3)
        .map(|_| {
            let start = Instant::now();
            let _ = artifact.parse_shape("doc", input);
            start.elapsed()
        })
        .min()
        .expect("three runs")
}

#[test]
fn single_codepoint_terminals_scale_linearly_when_absent_from_the_input() {
    // `&` is a single codepoint spelled in hex; none occurs in the input.
    for terminal in ["%x26", "(%x26 / %x27)", "(%x26 %x26)", "%xA0", "%x2028"] {
        let artifact = load(&grammar(terminal));
        let small = best_of_three(&artifact, &"a".repeat(60_000));
        let large = best_of_three(&artifact, &"a".repeat(240_000));
        // Four times the input: linear is ~4x, quadratic ~16x. Allow
        // 2.5x per doubling (6.25x over the two doublings).
        let floor = small.max(Duration::from_millis(1));
        assert!(
            large.as_secs_f64() <= 6.25 * floor.as_secs_f64(),
            "{terminal}: 60K took {small:?} but 240K took {large:?} (not linear)"
        );
    }
}

#[test]
fn hex_terminals_match_exactly_like_their_quoted_forms() {
    let cases = [
        ("%x5E", "\"^\"", "^x", "a^x^x"),
        ("%x26", "\"&\"", "&x", "&&x&x"),
        ("%xA0", "\"\u{a0}\"", "\u{a0}x", "a\u{a0}x"),
        ("%x2028", "\"\u{2028}\"", "\u{2028}x", "\u{2028}x\u{2028}"),
        ("(%x3C %x3C)", "\"<<\"", "<<x", "<<x<<x"),
        (
            "(%x0B / %x0C)",
            "(\"\u{b}\" / \"\u{c}\")",
            "\u{c}x",
            "\u{b}x\u{c}x",
        ),
    ];
    let inputs = ["", "x", "ax", "^", "\u{a0}", "<x", "<<", "ab&x&"];
    for (hex, quoted, hit, run) in cases {
        let by_hex = load(&grammar(hex));
        let by_quote = load(&grammar(quoted));
        for input in inputs.iter().copied().chain([hit, run]) {
            for entry in ["doc", "hit"] {
                let a = by_hex.parse_shape(entry, input);
                let b = by_quote.parse_shape(entry, input);
                assert_eq!(
                    a.is_ok(),
                    b.is_ok(),
                    "{hex} vs {quoted} on {input:?} entry {entry}"
                );
                if let (Ok(a), Ok(b)) = (a, b) {
                    assert_eq!(a, b, "{hex} vs {quoted} on {input:?} entry {entry}");
                }
            }
        }
        assert!(
            by_hex.parse_shape("hit", hit).is_ok(),
            "{hex} must match {hit:?}"
        );
        assert!(
            by_hex.parse_shape("hit", "ab").is_err(),
            "{hex} must not match \"ab\""
        );
        // Present only later in the input: not a match at position 0.
        assert!(
            by_hex.parse_shape("hit", &format!("a{hit}")).is_err(),
            "{hex} must not match a later occurrence"
        );
    }
}
