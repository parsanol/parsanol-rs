# 8. Incremental latency benchmark — SHIPPED

`parsanol/examples/incremental_bench.rs`: ~900 KiB document, initial
full parse, 100 single-line edits spread across the document, each
asserting the incremental tree equals a full re-parse while timing
both. Reports mean/p95/max keystroke latency, the full-reparse
baseline, speedup, retained/dropped entry counts, and session hit
stats.

Two retention bugs it flushed out (fixed in this round):

- The session cache kept its 4 KiB default capacity, so the
  recycling window discarded the early memo entries retention
  needed — retained counts were identically zero. The session now
  sizes its cache to the document.
- Failure entries were retained on the same end-position rule as
  successes; a failure's validity depends on bytes AFTER its
  position (an unknowable lookahead), so any failure near the edit
  boundary could replay stale. Failures are no longer retained.

Measured on a machine at load ~190 (absolute numbers are
load-inflated; ratios hold): mean keystroke re-parse ~1.5–1.7x
faster than the full re-parse, correctness-gated per edit. The
remaining wall is structural: the walker re-walks the entire rule
structure against terminal hits (~2 hits per line). Reaching
single-digit-millisecond keystrokes on documents this size needs
prefix-TREE reuse (splice the unaffected previous AST, reparse only
the suffix) — specced as item 9, unscheduled.
