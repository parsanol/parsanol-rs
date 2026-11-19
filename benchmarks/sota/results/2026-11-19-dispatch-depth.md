# Receipt: the dispatch depth relaxation — 2026-11-19 (rs#574 slice 2)

The ByteDispatch depth gate (`repetition_depth == 0`) dated from
rs#533, when dispatching json's per-character ws choice inside its
repetition measured 18x slower than interleaved charset probes. The
single-byte charset merge (landed since) absorbs exactly that shape
BEFORE the dispatch eligibility check at any depth — what still
reached the gate inside repetitions = multi-byte-branched choices
(json's value list), where the table wins.

## Interleaved A/B (min-of-15 per round, 6 rounds, M1 Max, load ~19)

parsanol VM, json corpus, 447,801 bytes, totals per round:

| round | main (gate at depth 0) | relaxed (dispatch any depth) |
|---|---|---|
| 1 | 34,124,332 ns | 27,692,085 ns |
| 2 | 33,988,541 | 26,829,665 |
| 3 | 33,813,168 | 27,173,623 |
| 4 | 34,215,376 | 27,316,042 |
| 5 | 33,882,376 | 28,143,749 |
| 6 | 34,068,624 | 27,159,875 |
| **min** | **33,813,168** | **26,829,665** |

**-20.6%** on the min; every round wins. The absolute moves
0.079 → **0.0599 µs/B** — 1.93x behind tree-sitter's 0.031 (was
2.55x), ahead of Lark LALR and ANTLR by 6x+.

## Soundness

Dispatch emission is value-identical by construction (each dispatched
branch re-matches its own lead character); the full suite (577 lib +
all binaries incl. the corpus identity gates) is green, and the
single red seen under load (`anchored_single_char`) fails on
origin/main too and passes on this branch in isolation.

## Gates to 0.03

The remaining 1.93x: the per-value table hop costs where tree-sitter's
LR table fuses shift/reduce — the next slices are the neg-scan idiom
and the string-content runs (TODO.perf/13's sequencing), then the
per-rule automaton routing behind the prover's proof.
