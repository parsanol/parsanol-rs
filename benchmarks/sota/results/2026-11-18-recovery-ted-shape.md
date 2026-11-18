# Receipt: the recovery-shape parity — 2026-11-18 (rs#576)

The windowed TED receipt (2026-11-17) recorded the published loss:
parsanol 0.113 vs tree-sitter 0.023, with two structural causes this
slice closes:

1. **The runner scored valid parses as total losses.** 8 of 60 cases
   (the `-1` delete/insert/mutate rows) still parse CLEANLY after the
   corruption (a 1-byte delete inside a string keeps the JSON valid),
   but `run_parsanol trees` read the envelope's `partial` — null on
   success — instead of `shape`. Those cases scored maximum distance
   for a parse that had already succeeded. The runner now reads
   `shape` when the envelope says `ok: true` (the ts hole-punch
   equivalent: a still-valid document recovers to its own clean
   parse).
2. **The stash gate dropped the good prefix.** `stash_partial`
   required a ROOT-level repetition (`start == 0`), and a json member
   list sits behind `{` — so the good prefix never stashed and the
   recovered tree was null for every truncate case (the file ends
   mid-value; the rounds had nothing ahead of them). The recovery
   spine now OWNS the partial: it replays the unchanged prefix
   forward from byte 0 with the same probe the rounds use — identical
   shapes to the clean parse's iterations, the original nesting — and
   the resync rounds continue from the replay end.

## Results (60-case harness, same corpus + seed)

TOTAL windowed nTED: **parsanol 0.059 | tree-sitter 0.023** — the gap
halved (was 0.113); parsanol wins truncate outright at depth 1 and
keeps the unterminated-20 win.

| class | depth | parsanol (rs#576) | parsanol (baseline) | tree-sitter |
|---|---|---|---|---|
| delete | 20 | 0.069 | 0.097 | 0.013 |
| insert | 20 | 0.075 | 0.103 | 0.006 |
| mutate | 20 | 0.081 | 0.125 | 0.019 |
| unterminated | 20 | **0.106** | 0.103 | 0.122 |
| truncate | 20 | **0.069** | 0.147 | 0.050 |

Full grid (n=4 per row): delete 0.000/0.034/0.069 · insert
0.022/0.078/0.075 · mutate 0.031/0.062/0.081 · truncate
**0.062**/0.069/0.069 (ts: 0.072/0.044/0.050) · unterminated
0.056/0.066/0.106 (ts: 0.000/0.019/0.122).

## The recovery-suite deltas (same commit)

The prefix replay changes the suffix accounting honestly: the failed
iteration's consumed bytes were previously counted as covered while
emitting no structure — they now count as junk.

| metric | baseline (rs#539 L2) | rs#576 |
|---|---|---|
| windowed nTED | 0.113 | **0.059** |
| structure recall (span-aware) | 52.8% | **67.5%** |
| suffix recovery | 92.6% | 90.4% (accounting, above) |
| diagnostic distance | ±24,744 | ±24,744 (unchanged) |

The recall jump is the replay itself: the prefix iterations now exist
as recovered spans, so ground-truth containment has structure to
match. Per class (suffix / recall, parsanol): delete-20 95.4% / 96.2%,
insert-20 83.4% / 88.0%, mutate-20 90.4% / 85.9%, unterminated-20
77.0% / 82.1% — unterminated-20's suffix improved (74.3% → 77.0%).

## Provenance

- Harness: `benchmarks/sota/` — the seed-pinned manifest,
  `run_parsanol trees|recovery`, `run_ts trees|recovery`,
  `score_ted.py`, `score_recovery.py`.
- The residual TED gap to ts (0.059 vs 0.023) is the corruption site
  itself: ts hole-punches an ERROR node where the clean tree had a
  subtree, parsanol skips the junk span — the window's one remaining
  structural difference is the skipped iteration, which ts renders as
  an ERROR node (also unlike the clean tree). Closing it further means
  emitting error-node-shaped placeholders for skipped spans — a shape
  decision, not a recovery one.
