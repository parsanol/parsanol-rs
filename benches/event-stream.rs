//! Event-stream pipeline benchmark: parse -> parslet-normalize ->
//! linearize -> replay. Guards the throughput of the flat event
//! encoding consumed by external model builders.

use std::time::Instant;

use parsanol::portable::events::{linearize_events, replay_events};
use parsanol::portable::parser_dsl::{dynamic, re, ref_, seq, str, GrammarBuilder, ParsletExt};
use parsanol::portable::to_parslet_compatible;
use parsanol::portable::{AstArena, Grammar, PortableParser};

fn fixture_grammar() -> Grammar {
    // Record-shaped grammar with repetitions, optionals and nested
    // sequences — the shapes the event stream has to carry cheaply.
    GrammarBuilder::new()
        .rule("file", dynamic(ref_("record")).repeat(1, None))
        .rule(
            "record",
            seq(vec![
                dynamic(str("REC ")),
                dynamic(re("[a-z]+").label("name")),
                dynamic(str(" ")),
                dynamic(
                    seq(vec![
                        dynamic(re("[a-z]")).label("field"),
                        dynamic(str(",")).label("sep"),
                    ])
                    .label("fields")
                    .repeat(1, None),
                ),
            ])
            .label("record"),
        )
        .build()
}

fn fixture_input(records: usize) -> String {
    let mut out = String::with_capacity(records * 24);
    for i in 0..records {
        let digits: String = (0..6)
            .map(|d| char::from(b'a' + ((i >> (d * 3)) % 26) as u8))
            .collect();
        out.push_str(&format!("REC {digits} a,b,c,d,"));
    }
    out
}

fn main() {
    let input = fixture_input(20_000);
    println!("input: {} KB", input.len() / 1024);

    let grammar = fixture_grammar();
    let runs = 3;
    let mut parse_s = f64::MAX;
    let mut linearize_s = f64::MAX;
    let mut replay_s = f64::MAX;
    let mut event_count = 0usize;

    for _ in 0..runs {
        let mut arena = AstArena::for_input(input.len());
        arena.set_input(input.clone());
        let t0 = Instant::now();
        {
            let mut parser = PortableParser::new(&grammar, &input, &mut arena);
            let raw = parser.parse().expect("parse");
            let t1 = Instant::now();
            parse_s = parse_s.min(t1.duration_since(t0).as_secs_f64());

            let shaped = to_parslet_compatible(&raw, &mut arena, &input);
            let t2 = Instant::now();
            let (events, strings) = linearize_events(&shaped, &mut arena);
            let t3 = Instant::now();
            event_count = events.len();
            let _replayed = replay_events(&events, &strings, &mut arena);
            let t4 = Instant::now();
            linearize_s = linearize_s.min(t3.duration_since(t2).as_secs_f64());
            replay_s = replay_s.min(t4.duration_since(t3).as_secs_f64());
        }
    }

    let mb = input.len() as f64 / 1_048_576.0;
    println!(
        "parse {:8.1} ms | linearize {:6.1} ms ({} events) | replay {:6.1} ms",
        parse_s * 1e3,
        linearize_s * 1e3,
        event_count,
        replay_s * 1e3
    );
    println!(
        "event pipeline throughput: {:6.1} MB/s (parse+linearize+replay over {:.2} MB)",
        mb / (parse_s + linearize_s + replay_s),
        mb
    );
}
