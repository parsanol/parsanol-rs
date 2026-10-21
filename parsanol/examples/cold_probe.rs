//! Cold-parse scaling probe (rs#270): parses the same KV corpus with
//! grammar variants that isolate one feature each, printing CPU time.
//! Usage: cargo run --release --example cold_probe [pairs]
use parsanol::portable::parser_dsl::{self, GrammarBuilder, ParsletExt as _};
use parsanol::portable::{AstArena, Grammar, PortableParser};
use std::time::Instant;

fn corpus(pairs: usize) -> String {
    (0..pairs)
        .map(|i| {
            let key: String = std::iter::repeat_n(char::from(b'a' + (i % 26) as u8), 4).collect();
            format!("{key}{i}={i};")
        })
        .collect()
}

/// The keystroke grammar: dynamic() wrappers + capture() store captures.
fn variant_dynamic_capture() -> Grammar {
    use parser_dsl::{capture, dynamic, ref_, seq, str};
    let pair = seq([
        dynamic(capture("k", parser_dsl::re(r"[a-z0-9]+"))),
        dynamic(str("=")),
        dynamic(capture("v", parser_dsl::re(r"[0-9]+"))),
        dynamic(str(";")),
    ]);
    let document = ref_("pair").repeat(1, None).label("pairs");
    GrammarBuilder::new()
        .rule("document", document)
        .rule("pair", pair)
        .build()
}

/// Tree-level named captures, no dynamic wrappers.
fn variant_named() -> Grammar {
    use parser_dsl::{dynamic, re, ref_, seq, str, ParsletExt as _};
    let pair = seq([
        dynamic(re(r"[a-z0-9]+").label("k")),
        dynamic(str("=")),
        dynamic(re(r"[0-9]+").label("v")),
        dynamic(str(";")),
    ]);
    let document = ref_("pair").repeat(1, None).label("pairs");
    GrammarBuilder::new()
        .rule("document", document)
        .rule("pair", pair)
        .build()
}

/// No captures at all: the pair is a bare sequence.
fn variant_plain() -> Grammar {
    use parser_dsl::{dynamic, re, ref_, seq, str};
    let pair = seq([
        dynamic(re(r"[a-z0-9]+")),
        dynamic(str("=")),
        dynamic(re(r"[0-9]+")),
        dynamic(str(";")),
    ]);
    let document = ref_("pair").repeat(1, None).label("pairs");
    GrammarBuilder::new()
        .rule("document", document)
        .rule("pair", pair)
        .build()
}

/// No captures and no dynamic wrappers: everything inline.
fn variant_inline() -> Grammar {
    // All-dynamic elements keep seq's element types homogeneous; the
    // point of this variant is "no captures", not "no dynamic".
    use parser_dsl::{dynamic, re, ref_, seq, str};
    let pair = seq([
        dynamic(re(r"[a-z0-9]+")),
        dynamic(str("=")),
        dynamic(re(r"[0-9]+")),
        dynamic(str(";")),
    ]);
    let document = ref_("pair").repeat(1, None);
    GrammarBuilder::new()
        .rule("document", document)
        .rule("pair", pair)
        .build()
}

fn main() {
    let pairs: usize = std::env::args()
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or(20_000);
    let input = corpus(pairs);
    type Variant = (&'static str, fn() -> Grammar);
    let cases: &[Variant] = &[
        ("dynamic+capture", variant_dynamic_capture),
        ("named-only", variant_named),
        ("plain-seq", variant_plain),
        ("inline", variant_inline),
    ];
    println!("pairs={pairs} bytes={}", input.len());
    for (name, build) in cases {
        let grammar = build();
        let mut arena = AstArena::for_input(input.len() * 4);
        let mut parser = PortableParser::new(&grammar, &input, &mut arena);
        let started = Instant::now();
        let outcome = parser.parse().is_ok();
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        println!("  {name:>16}: {ms:>10.1} ms  parsed={outcome}");
    }
}
