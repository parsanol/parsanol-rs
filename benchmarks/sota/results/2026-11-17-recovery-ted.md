# Receipt: the windowed tree-edit-distance scorer — 2026-11-17 (rs#527)

The third recovery instrument (after diagnostic distance and span
recall): ordered-tree edit distance (Zhang-Shasha) between the CLEAN
document's value tree and each engine's RECOVERED tree, both pruned to
a ±1024 B window around the corruption site (≤80 nodes, nearest
first), normalized by the larger tree's node count. Lower is better:
0.0 = the window's structure is intact.

## Results (60-case harness)

TOTAL windowed nTED: **parsanol 0.113 | tree-sitter 0.023** — a loss.

| class | depth | parsanol | tree-sitter |
|---|---|---|---|
| delete | 20 | 0.097 | 0.013 |
| insert | 20 | 0.103 | 0.006 |
| mutate | 20 | 0.125 | 0.019 |
| unterminated | 20 | 0.103 | **0.122** (parsanol wins) |
| truncate | 20 | 0.147 | 0.050 |

## Why the gap is structural

tree-sitter hole-punches: its recovery keeps the ORIGINAL tree and
drops an ERROR region — everything else in the window is
byte-identical to the clean parse, so its nTED sits near zero (many
rows at 0.000). parsanol's recovery REBUILDS at the recovery spine's
granularity (the member-list replays the suffix as spine iterations),
so the recovered window is a flatter structure than the clean parse's
nesting — the same information, differently shaped, and the edit
distance charges for the reshaping.

The one win: unterminated-20 (0.103 vs 0.122) — the deepest
truncation class, where ts's hole-punch leaves a root-sized ERROR that
the window cannot avoid and parsanol's replay reconstructs more of
the window's shape.

## The instrument

- `run_parsanol trees <corrupted-dir> <clean-dir> <manifest> <artifact>`
  — one JSON line per case: the recovery envelope's shaped `partial`
  (the recovered structure under the root name) plus the clean
  document's shape tree.
- `run_ts trees <corrupted-dir> <clean-dir> <manifest>` — named-node
  trees (kinds as-is, ERROR nodes included), same line shape.
- `score_ted.py <manifest> <parsanol-trees> <ts-trees>` — the
  windowed Zhang-Shasha scorer (validated on rename/delete-subtree/
  insert/empty cases: identical trees 0, one rename 1, subtree delete
  = node count of the subtree, insert-subtree = its node count).
- Renames cost 0 on equal kinds, 1 otherwise; inserts/deletes cost 1.

## Reading it honestly

The scorer compares each engine's OWN clean parse to its OWN recovered
parse, so the metric measures how much of the original tree shape the
engine preserves — ts's model (patch the original) is structurally
favored over parsanol's model (rebuild forward from the failure).
Closing that gap is a recovery-shape question: emitting the
unchanged prefix/suffix subtrees in their original nesting (the splice
work of rs#542 slice 2 shares that machinery) would flatten this
number. The instrument now exists to measure it.
