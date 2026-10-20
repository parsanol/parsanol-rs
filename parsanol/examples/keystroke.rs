//! Keystroke latency probe for the retained incremental session.
//!
//! Usage: cargo run --release --example keystroke [pairs] [edits] [tail|random]
//!
//! Builds a KV document, parses it once, then applies single-character
//! digit replacements and reports the median and p95 per-edit
//! retained-reparse time plus cache reuse. `tail` edits the last digit
//! (maximal reuse: the whole prefix is retention-eligible); `random`
//! edits anywhere (the multi-region worst case). This is the
//! TODO.sota/7 measurement: it prints numbers, it does not assert, so it
//! can run on contended machines with the load average recorded
//! separately.
use parsanol::portable::incremental::{Edit, IncrementalParser};
use parsanol::portable::parser_dsl::{self, GrammarBuilder, ParsletExt as _};
use parsanol::portable::Grammar;
use std::io::Write;
use std::time::Instant;

fn kv_grammar() -> Grammar {
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

fn kv_corpus(pairs: usize) -> String {
    (0..pairs)
        .map(|i| {
            let key: String = std::iter::repeat_n(char::from(b'a' + (i % 26) as u8), 4).collect();
            format!("{key}{i}={i};")
        })
        .collect()
}

fn main() {
    let mut args = std::env::args().skip(1);
    let pairs: usize = args.next().and_then(|v| v.parse().ok()).unwrap_or(50_000);
    let edits: u32 = args.next().and_then(|v| v.parse().ok()).unwrap_or(20);
    let mode = args.next().unwrap_or_else(|| "tail".to_string());

    let grammar = kv_grammar();
    let mut session = IncrementalParser::new(&grammar);
    let mut input = kv_corpus(pairs);
    let started = Instant::now();
    session.parse_retained(&input).expect("initial parse");
    println!(
        "corpus mode={mode} pairs={pairs} bytes={} initial_parse_ms={:.3}",
        input.len(),
        started.elapsed().as_secs_f64() * 1000.0
    );
    let _ = std::io::stdout().flush();

    let mut rng: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut times = Vec::with_capacity(edits as usize);
    let mut reused_total = 0usize;
    for _ in 0..edits {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        let offset = if mode == "random" {
            (rng as usize) % input.len()
        } else {
            input.len().saturating_sub(2)
        };
        let current = input.as_bytes()[offset];
        if !current.is_ascii_digit() {
            continue;
        }
        let replacement = b'0' + ((rng >> 16) % 10) as u8;
        let mut next = input.clone();
        next.replace_range(offset..offset + 1, &(replacement as char).to_string());
        input = next;

        let started = Instant::now();
        let result = session
            .parse_with_edit_retained(&input, Edit::replace(offset, 1, 1))
            .expect("retained reparse");
        times.push(started.elapsed().as_secs_f64() * 1000.0);
        reused_total += result.reused_cache_entries;
    }

    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = times[times.len() / 2];
    let p95 = times[(times.len() * 95) / 100];
    let mean_reused = reused_total as f64 / times.len() as f64;
    println!(
        "edits={} median_ms={median:.3} p95_ms={p95:.3} mean_reused_entries={mean_reused:.0} \
         spliced_parses={} arena_bytes={}",
        times.len(),
        session.spliced_parses(),
        session.retained_arena().memory_usage()
    );
    let _ = std::io::stdout().flush();
}
