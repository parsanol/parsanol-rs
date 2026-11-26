//! The item-6 A/B harness: the 32-byte-window driver (the shipped
//! scan) against a local 16-byte-chunk reference on long runs.
//! Interleaved min-of-N; the host's absolute numbers are meaningless
//! (the load) — the DELTA between the drivers is the receipt's
//! measurement.
//!
//!   cargo run --release --example wide_scan_ab -- <megabytes> <rounds>

use parsanol::portable::scan::ScanPlan;
use std::time::Instant;

/// The 16-byte-chunk reference: the same membership logic stepping one
/// block at a time (the pre-window shape).
fn narrow_run(plan: &ScanPlan, input: &[u8], from: usize) -> usize {
    let mut pos = from;
    for chunk in input[from..].chunks_exact(16) {
        let all = chunk.iter().all(|&b| plan.contains(b));
        if !all {
            for (i, &b) in chunk.iter().enumerate() {
                if !plan.contains(b) {
                    return pos + i;
                }
            }
            unreachable!();
        }
        pos += 16;
    }
    while pos < input.len() && plan.contains(input[pos]) {
        pos += 1;
    }
    pos
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mb: usize = args.get(1).and_then(|v| v.parse().ok()).unwrap_or(8);
    let rounds: u32 = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(20);
    // A [a-z] run over most of the buffer, with a terminating space.
    let len = mb * 1024 * 1024;
    let mut input = vec![b'a' + (b'z' - b'a'); len];
    input[len / 2] = b' ';
    let plan = ScanPlan::from_membership(|b| (0x61..=0x7A).contains(&b));

    let mut best_wide = u64::MAX;
    let mut best_narrow = u64::MAX;
    for _ in 0..rounds {
        let t = Instant::now();
        let w = plan.scan_run(&input, 0);
        let dw = t.elapsed().as_nanos() as u64;
        let t = Instant::now();
        let n = narrow_run(&plan, &input, 0);
        let dn = t.elapsed().as_nanos() as u64;
        assert_eq!(w, n, "the drivers must agree on the run end");
        best_wide = best_wide.min(dw);
        best_narrow = best_narrow.min(dn);
    }
    println!(
        "wide(best) {best_wide}ns  narrow(best) {best_narrow}ns  ratio {:.3}",
        best_wide as f64 / best_narrow as f64
    );
}
