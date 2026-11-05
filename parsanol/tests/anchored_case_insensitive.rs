//! Case-insensitive literals (`%i"note"`, compiled to `(?i:note)`) and
//! alternations of them must be matched anchored at the position, so a
//! failed attempt costs the match length, not the rest of the input.

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

fn best_of_five(artifact: &PargArtifact, input: &str) -> Duration {
    (0..5)
        .map(|_| {
            let start = Instant::now();
            let _ = artifact.parse_shape("doc", input);
            start.elapsed()
        })
        .min()
        .expect("five runs")
}

const ALTERNATION: &str =
    "(%i\"note\" / %i\"tip\" / %i\"warning\" / %i\"caution\" / %i\"important\")";

#[test]
fn case_insensitive_literals_scale_linearly_when_absent_from_the_input() {
    for terminal in ["%i\"note\"", ALTERNATION] {
        let artifact = load(&grammar(terminal));
        // Base64-like lines: they hold the letters of the words and
        // partial matches ("no", "tip" prefixes) but never a full word.
        let line = "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVphYmNkZWZnaGlqa2xtbm9wcXJzdHV2d3h5ejAxMjM0NTY3ODkrLw\n";
        let small = best_of_five(&artifact, &line.repeat(375));
        let large = best_of_five(&artifact, &line.repeat(3000));
        // Eight times the input: linear is ~8x, quadratic ~64x.
        let floor = small.max(Duration::from_millis(1));
        assert!(
            large.as_secs_f64() <= 24.0 * floor.as_secs_f64(),
            "{terminal}: 1x took {small:?} but 8x took {large:?} (not linear)"
        );
    }
}

#[test]
fn case_insensitive_literals_match_exactly_like_their_spelled_out_forms() {
    let word = |w: &str| {
        let letters: Vec<String> = w
            .chars()
            .map(|c| format!("(\"{}\" / \"{}\")", c, c.to_ascii_uppercase()))
            .collect();
        format!("({})", letters.join(" "))
    };
    let spelled_note = word("note");
    let spelled_alt = format!(
        "({})",
        ["note", "tip", "warning", "caution", "important"]
            .iter()
            .map(|w| word(w))
            .collect::<Vec<_>>()
            .join(" / ")
    );
    let inputs = [
        "",
        "x",
        "note",
        "notex",
        "NOTEx",
        "NoTeX",
        "tipx",
        "TIPx",
        "xnotex",
        "ax notex",
        "Warningx",
        "CAUTIONx",
        "importantx",
        "IMPORTANTx",
        "notx",
        "nonotex",
        "tiptipx",
    ];
    for (ci, spelled) in [("%i\"note\"", spelled_note), (ALTERNATION, spelled_alt)] {
        let a = load(&grammar(ci));
        let b = load(&grammar(&spelled));
        for input in inputs {
            for entry in ["doc", "hit"] {
                let ra = a.parse_shape(entry, input);
                let rb = b.parse_shape(entry, input);
                assert_eq!(ra.is_ok(), rb.is_ok(), "{ci} on {input:?} entry {entry}");
                match (ra, rb) {
                    (Ok(x), Ok(y)) => assert_eq!(x, y, "{ci} on {input:?} entry {entry}"),
                    (Err(_), Err(_)) => {}
                    _ => unreachable!(),
                }
            }
        }
    }
    assert!(load(&grammar("%i\"note\""))
        .parse_shape("hit", "NoTex")
        .is_ok());
    assert!(load(&grammar("%i\"note\""))
        .parse_shape("hit", "a NoTex")
        .is_err());
}
