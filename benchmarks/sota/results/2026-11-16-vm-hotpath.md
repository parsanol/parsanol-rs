# Receipt: VM hot-path round — 2026-11-16 (rs#533 follow-up, TODO.perf/10)

Three constant-factor fixes to the VM, each named by a profile of the
rs#533 JSON harness (`sample` on the live runner), each verified
byte-identical: the full crate suite (633 tests) green, the diagnostics
across all 60 corrupted cases diff-identical to the pre-patch engine,
the packrat-differential and vm diagnostics specs untouched.

## The three findings and fixes

1. **Success-path diagnostics (91% of the first profile).**
   `track_failure_with_context` built the `Expected` label — a
   `format!` with char-set scans and realloc churn — at EVERY failed
   alternative. Valid JSON fails ~2–6 branches per value node, so the
   engine was formatting strings it always throws away. Fix: the hot
   path records `(position, ip)` pairs only (`record_failure_ip`);
   labels materialize once at parse end (`materialize_deferred`),
   deduplicating exactly like the eager path. The position-0
   first-failure guard is mirrored (the eager path dropped it; so does
   the deferred one).

2. **Per-iteration wall-clock (28% of the second profile).** The VM
   called `Instant::elapsed` (a `clock_gettime`) every loop iteration
   for the 30 s timeout guard. Fix: sample every 4096 instructions —
   pathological inputs are still caught at sub-millisecond latency.

3. **Regex cache deep clones (39% of the third profile).** Every
   `Regex` instruction executed `get_or_compile(pattern)` to validate,
   then DISCARDED the compiled regex — and the cache returned it by
   deep clone (meta `Regex` carries a capture pool; `regex::Regex`
   likewise). JSON lowers classes and keyword spellings to per-char
   Regex instructions, so this ran per character. Fix: both caches
   store `Arc` — a clone is a refcount bump.

## Results (same corpus, same harness, min-of-15)

| build | VM µs/B | interpreter µs/B |
|---|---|---|
| baseline (1.5.1 release, crates.io) | 1.086 | 0.828 |
| + deferred diagnostics | 0.747 | 0.869 |
| + Arc anchored cache + periodic timeout | 0.413 | 0.592 |
| + Arc plain regex cache | **0.174** | **0.484** |

Cumulative: **VM 6.2×, interpreter 1.7×** — with zero grammar-language
or algorithm change. Context: Lark LALR 0.344, tree-sitter 0.029.
The VM now beats every Python engine in the table and carries the
typed-AST output tree-sitter doesn't build. The remaining 6× to
tree-sitter is exactly TODO.perf/11–13 (determinism dispatch, scan
kernels, automaton codegen): the profile's survivors are per-char
class dispatch and TLS hash lookups, both of which collapse under
Phase 1.

## Notes

- The first-profile figure (91%) inflated by allocator stalls inside
  the format path; the honest wall-clock split is the table above.
- The parg_corpus sibling-checkout failure in the suite is
  pre-existing on clean main (the pubid-grammar json-seal vs CI-pin
  mismatch, owner decision open) — verified by stash on unpatched
  main before attributing anything to this round.
