//! The diagnostics-latency receipt (rs#529): diagnostics-on-keystroke
//! over the self-hosting grammar's own source — PgMeta parsing PARG
//! parsing itself, the hardest internal dogfood.

use parsanol_lsp::server::Server;
use std::time::Instant;

const SELF_SOURCE: &str = include_str!("../artifacts/parg.parg");

#[test]
fn diagnostics_on_keystroke_over_the_self_hosting_grammar() {
    let server = Server::new();
    // Warm: the artifact is embedded; the first diagnostic includes the
    // one-time shape-tree build.
    let t0 = Instant::now();
    let warm = server.diagnostics(SELF_SOURCE);
    let warm_us = t0.elapsed().as_millis();
    let has_errors = !warm.is_empty();
    println!(
        "diagnostics on {} bytes (self-hosting source): {} ms; {} diagnostics",
        SELF_SOURCE.len(),
        warm_us,
        warm.len()
    );
    // The keystroke budget: an editor re-runs this per change.
    assert!(
        warm_us < 2000,
        "the diagnostics pass must stay inside the keystroke budget: {warm_us} ms"
    );

    // Steady state: five more passes, the median.
    let mut times = Vec::new();
    for _ in 0..5 {
        let t = Instant::now();
        let _ = server.diagnostics(SELF_SOURCE);
        times.push(t.elapsed().as_millis());
    }
    times.sort();
    println!("steady-state median: {} ms", times[2]);
    assert!(times[2] < 2000, "the median must stay inside the budget");
    // The self-source parses or fails with the wire — either way the
    // receipt records which.
    println!("self-parse produced diagnostics: {has_errors}");
}
