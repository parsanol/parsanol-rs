// Profiling driver for rs#486: parse the tiny asciichem inputs in a
// loop under the sampler, mirroring the asciichem parse path exactly
// (arena sized to the input, PortableParser::new, parse_with_end_pos,
// then the parslet transform + value conversion).
//
// With --counts: run ONE parse per input and report the allocation
// count/bytes (counting global allocator), then exit — the
// deterministic shape of the fixed cost.

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};

static ALLOCS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);

struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn one_parse(grammar: &parsanol::portable::grammar::Grammar, input: &str) {
    let mut arena = parsanol::portable::AstArena::for_input(input.len());
    let mut parser = parsanol::portable::PortableParser::new(grammar, input, &mut arena);
    let result = parser.parse_with_end_pos();
    black_box(result.expect("parse"));
    let _ = black_box(parser);
}

fn main() {
    let counts = std::env::args().any(|a| a == "--counts");
    let path = std::env::args()
        .nth(1)
        .filter(|a| a != "--counts")
        .expect("usage: eval-profile <asciichem.json> [rounds|--counts]");
    let text = std::fs::read_to_string(&path).expect("artifact file");
    let artifact = parsanol::PargArtifact::from_json(&text).expect("artifact loads");
    let grammar = artifact.grammar("formula").expect("the formula entry");

    let inputs = ["H_2O", "Ca^2+", "SO_4^2-"];

    if counts {
        for input in inputs {
            // Warm all process-lifetime caches (grammar decode, regex
            // cache, scan plans) so the counted parse sees steady state.
            one_parse(&grammar, input);
            ALLOCS.store(0, Ordering::Relaxed);
            BYTES.store(0, Ordering::Relaxed);
            let mut arena = parsanol::portable::AstArena::for_input(input.len());
            let construct_allocs = ALLOCS.load(Ordering::Relaxed);
            let mut parser = parsanol::portable::PortableParser::new(&grammar, input, &mut arena);
            let eval_allocs = ALLOCS.load(Ordering::Relaxed) - construct_allocs;
            let result = parser.parse_with_end_pos();
            black_box(result.expect("parse"));
            let allocs = ALLOCS.load(Ordering::Relaxed);
            let bytes = BYTES.load(Ordering::Relaxed);
            println!(
                "{input}: total {allocs} allocs / {bytes} B (construct {} = arena {}, parser {})",
                construct_allocs + eval_allocs,
                construct_allocs,
                eval_allocs
            );
            #[cfg(feature = "perf-counters")]
            println!("{}", parser.perf_counters_report());
        }
        return;
    }

    // Warm once (assert the parse succeeds; caches warm like the bench).
    for input in inputs {
        let mut arena = parsanol::portable::AstArena::for_input(input.len());
        let mut parser = parsanol::portable::PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse_with_end_pos().is_ok(), "{input} must parse");
    }

    let rounds = std::env::args()
        .nth(2)
        .and_then(|r| r.parse::<u32>().ok())
        .unwrap_or(200_000);
    for _ in 0..rounds {
        for input in inputs {
            let mut arena = parsanol::portable::AstArena::for_input(black_box(input).len());
            let mut parser = parsanol::portable::PortableParser::new(
                black_box(&grammar),
                black_box(input),
                &mut arena,
            );
            black_box(parser.parse_with_end_pos().ok());
        }
    }
    println!("done");
}
