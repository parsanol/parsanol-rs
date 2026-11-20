# Benchmarks

Industry head-to-head receipts for parsanol against established parsers
(tree-sitter, Lark, ANTLR), plus both parsanol engines. The harness,
corpora, traces, corrupted cases, and dated receipts live in
[`benchmarks/sota/`](../benchmarks/sota/); this page publishes the
current tables.

## Methodology

- **Shared corpus.** Every engine parses byte-identical documents
  (hashes committed per receipt).
- **Native runners.** Each engine runs its canonical binding; no
  cross-language handicaps.
- **Full-consumption proof before timing.** Every parse is asserted
  complete and error-free before its timing counts.
- **min-of-N per document** (N=15).
- **The honesty clause.** Losses publish with the same prominence as
  wins, with the profile of where the bytes go.

## Raw parse throughput — JSON corpus (447,801 B, 2026-11-20)

| engine | µs/byte | vs tree-sitter |
|---|---|---|
| tree-sitter 0.25 (rust binding, C automaton) | 0.031 | 1.0× |
| **parsanol 1.5.11 VM (Phase 0 pooled writes)** | **0.0418** | **1.35×** |
| parsanol 1.5.10 VM (dispatch-depth relaxation) | 0.0599 | 1.93× |
| parsanol 1.5.7 VM (Scan campaign, quiet-machine re-bake) | 0.079 | 2.55× |
| parsanol interpreter | 0.364 | 11.7× |
| Lark 1.x LALR (python) | 0.366 | 11.8× |
| ANTLR 4.13.2 (python3 target) | 1.970 | 63× |

The climb since the 1.5.1 baseline (1.073 µs/B): the Scan campaign
(1.5.6/1.5.7 bulk class-run kernels + scan-dispatch inlining) took
1.073 → 0.079; the dispatch-depth relaxation (1.5.10, the charset
merge absorbing the per-char shape) 0.079 → 0.0599; and the Phase 0
pooled-writes round (1.5.11 — the packrat memo's multiply-xor hasher
+ pre-size, cached shape-tag/name interns, value-stack tail-store
instead of per-node `split_off`, the arena pre-size) **0.0599 →
0.0418 (−35%)**. Every step interleaved-A/B'd with byte-identical
trees. The remaining 1.35× is profiled: the VM dispatch loop itself
and the per-char scan kernels, then the per-rule LR/automaton routing
behind the determinism prover — the typed-AST output tree-sitter's
flat CST never produces stays the differentiator.

Receipts: [the pooled-writes receipt](../benchmarks/sota/results/2026-11-20-memo-pooled-writes.md),
[the dispatch-depth receipt](../benchmarks/sota/results/2026-11-19-dispatch-depth.md),
[the quiet-machine re-bake](../benchmarks/sota/results/2026-11-18-receipts-rebake.md).

## Error recovery — corrupted JSON (60 seeded cases, 2026-11-18)

| metric | parsanol (spine prefix replay) | tree-sitter 0.25 |
|---|---|---|
| windowed nTED (±1 KB, ≤80 nodes) | 0.059 (was 0.113) | 0.023 |
| structure recall (span-aware) | **67.5%** (was 52.8%) | — |
| diagnostic distance (lower is better) | **±24,744** | ±56,349 |
| diagnostic on truncation | **±1 byte** | root-ERROR at 0 |
| null recoveries | **0/60** (was 24) | — |

The spine replays the unchanged prefix instead of re-deriving it, so
recovered iterations exist as real spans: structure recall jumped
52.8% → 67.5% and null recoveries fell 24 → 0 in the same round that
halved the windowed TED gap. The residual 0.059 vs 0.023 is the
corruption site itself: ts hole-punches an ERROR node where the clean
tree had a subtree, parsanol skips the junk span — closing it further
means emitting error-node-shaped placeholders for skipped spans (a
shape decision, not a recovery one). parsanol keeps the diagnostic
wins outright: its deepest-failure position tracks the corruption
cause; ts's ERROR regions start far from it.

Receipts: [the recovery-shape receipt](../benchmarks/sota/results/2026-11-18-recovery-ted-shape.md),
[the windowed TED receipt](../benchmarks/sota/results/2026-11-17-recovery-ted.md).

## Keystroke-scale incremental editing (200-keystroke traces, 2026-11-18)

| doc | bytes | parsanol p50 | ts p50 | splice ratio vs own fresh | spliced |
|---|---|---|---|---|---|
| doc0 | 13,290 | 0.66 ms | 0.014 ms | 6.9× | 199/200 |
| doc1 | 281,181 | 7.08 ms | 0.175 ms | 16.1× | 180/200 |
| doc2 | 1,507 | 0.26 ms | 0.006 ms | 2.1× | 197/200 |
| doc3 | 151,823 | 3.48 ms | 0.093 ms | 16.6× | 190/200 |

Tree identity held at **800/800 keystrokes** across all four documents
(zero divergence from a from-scratch parse). The p50 splice runs
2.1–16.6× faster than parsanol's own fresh parse (>10× on the large
docs); the tail stays full-parse-sized by design — structural edits
break the splice's identity guards and fall back to the full parse,
which is what makes the splice safe. tree-sitter's incremental remains
faster in absolute terms on this harness.

Receipt: [the quiet-machine re-bake](../benchmarks/sota/results/2026-11-18-receipts-rebake.md)
(extends the earlier [incremental receipt](../benchmarks/sota/results/2026-11-15-json-incremental.md)).

## Constrained decoding (llguidance-compatible masks, 2026-11-19)

Grammar-valid token masks from the checksummed artifact: validity
**200/200 (100%)** on both grammars (isbn + json); the end-to-end
constrained loop costs 5.8% of a 10 ms/token model forward pass on
isbn. The dead-end rollout (a closed brace with no live continuation)
recovers through the colon branch — FSM mask compilers cannot.
Receipt: [the mask receipt](../benchmarks/sota/results/2026-11-19-constrain-mask-receipt.md).

## Reproducing

See [`benchmarks/sota/README.md`](../benchmarks/sota/README.md) — one
command per suite, every runner asserts full consumption before timing.
