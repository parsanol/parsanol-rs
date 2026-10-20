//! Packrat cache memory bound (TODO.sota/6).
//!
//! The cache is the hard-bounded tier: entries (24 B) plus the slot table
//! (4 B/slot, power-of-two) clamp at ~62 MiB regardless of input size
//! (see `DenseCache::for_input`). The arena is a separate, uncapped tier:
//! total parse memory stays input-proportional unless a governor limit is
//! configured with `set_max_memory`.

use parsanol::portable::arena::AstArena;
use parsanol::portable::grammar::Grammar;
use parsanol::portable::parser::PortableParser;

const CACHE_CEILING: usize = 65 * 1024 * 1024;

fn rep_grammar() -> String {
    let atoms = serde_json::json!([
        { "Str": { "pattern": "a" } },
        { "Repetition": { "atom": 0, "min": 0, "max": null, "tag": "Repetition" } },
    ]);
    serde_json::json!({ "atoms": atoms, "root": 1 }).to_string()
}

#[test]
fn giant_input_stays_under_the_cache_ceiling() {
    // 600 KB input: input_len * atom_count sizes the cache from the
    // full packrat surface before parsing starts.
    let input = "a".repeat(600_000);
    let grammar = Grammar::from_json(&rep_grammar()).expect("grammar compiles");
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, &input, &mut arena);

    parser.parse().expect("giant input parses");

    let cache = parser.cache_memory_usage();
    assert!(
        cache > 10 * 1024 * 1024,
        "expected a populated packrat cache on a giant input, got {cache} bytes"
    );
    assert!(
        cache <= CACHE_CEILING,
        "cache exceeded its documented bound: {cache} bytes"
    );
    assert!(
        parser.memory_usage() >= cache,
        "total memory must include the bounded cache tier"
    );
}
