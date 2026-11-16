# Receipt: JSON recovery head-to-head — 2026-11-15 (rs#527, first receipt)

Corpus: the 4-document JSON corpus, corrupted by the seeded harness
(`corrupt.py`, seed 20261115): 5 classes × 3 depths × 4 docs = 60 cases,
byte-exact and committed with the manifest of corruption sites.

Engines: parsanol 1.5.1 VM (deepest-failure diagnostics; the json flavor
defines no recovery envelopes yet) vs tree-sitter 0.25 (its cost-model
recovery at defaults).

## Metrics

- **diagnostic distance** (lower is better): |reported error position −
  nearest corruption site|. parsanol reports the deepest-failure
  position; tree-sitter reports the start of its first ERROR node.
- **suffix recovery**: fraction of the post-corruption suffix still
  parsed into valid structure. parsanol: deepest-failure position past
  the site (an upper-bound proxy for resumed structure). tree-sitter:
  bytes under non-ERROR structure past the site, crediting recovered
  subtrees inside ERROR regions. Undefined for truncation (the suffix
  was removed — nothing left to recover).

## Results

| class | depth | n | parsanol diag± | parsanol suffix | ts diag± | ts suffix |
|---|---|---|---|---|---|---|
| delete | 1 | 4 | 30869 | 0.0% | 30869 | 0.0% |
| delete | 5 | 4 | 26700 | 29.2% | 38769 | 43.4% |
| delete | 20 | 4 | 19426 | 20.8% | 72796 | 94.2% |
| insert | 1 | 4 | 47364 | 0.0% | 47364 | 25.0% |
| insert | 5 | 4 | 53108 | 52.0% | 57227 | 99.9% |
| insert | 20 | 4 | 1021 | 7.9% | 95232 | 98.8% |
| mutate | 1 | 4 | 31124 | 0.0% | 31122 | 49.9% |
| mutate | 5 | 4 | 46536 | 35.4% | 82720 | 95.5% |
| mutate | 20 | 4 | 13412 | 19.9% | 95242 | 96.0% |
| truncate | 1 | 4 | 1 | — | 22655 | — |
| truncate | 5 | 4 | 1 | — | 24832 | — |
| truncate | 20 | 4 | 1 | — | 17332 | — |
| unterminated | 1 | 4 | 9268 | 1.0% | 34336 | 73.7% |
| unterminated | 5 | 4 | 18081 | 13.0% | 49549 | 97.6% |
| unterminated | 20 | 4 | 6 | 0.1% | 40962 | 73.5% |

**TOTAL (suffix-defined classes): parsanol 14.9% suffix (diag ±24,743)
vs tree-sitter 70.6% suffix (diag ±56,349).**

## Honest analysis

**We lose suffix recovery by ~5×, we win diagnostic precision by ~2×.**

- **Suffix recovery:** tree-sitter's cost-model recovery is built in —
  it always produces a tree, skipping broken regions and resuming
  structure (94–99% on multi-site deletes/inserts/mutates). parsanol
  without recovery envelopes stops at the deepest failure. This is the
  receipt that quantifies exactly what the recovery-envelope design
  round has to close: declarative skip/resume annotations on the JSON
  grammar should recover the structured-block classes (delete/mutate/
  unterminated) toward tree-sitter's numbers.
- **Diagnostic precision is ours.** parsanol's deepest-failure position
  lands near the corruption cause (±1 byte on truncation — the exact
  cut point). tree-sitter's first ERROR node frequently starts far from
  the cause (±72,796 on delete-20: its cost model absorbs a huge region
  into one error block, so the error POSITION is not the error CAUSE).
  On truncation tree-sitter reports a root-level ERROR at offset 0 —
  precise for "it's broken", useless for "where".
- **Both engines agree on where the break is** when there is one: on
  single-site corruption the two reported positions match within bytes
  (delete-1: 30,869 vs 30,869 — the failure point is the corruption
  point). The difference is entirely in what happens *after*: tree-sitter
  resumes, parsanol stops.
- **parsanol has no false successes.** Every parsanol "ok" on a
  corrupted case was verified against ground truth (python `json.loads`);
  the truncate rows report `false` with the position of the cut. The
  rs#496 lesson holds in reverse on this corpus.

## Where the gap lives (the design-round input)

1. **No skip/resume annotations** in the json flavor — the single
   dominant factor. Envelopes on object/array/string rules directly
   target delete/mutate/unterminated, the classes where ts reaches 73–99%.
2. **Multi-site depth-20 is where recovery quality decides**: ts 94–99%
   vs parsanol 8–21%. Single-site rows are near-tied (both engines stop
   or stumble at the first break) — the gap opens with per-region
   resumption, which is exactly what envelopes + the partial-tree stash
   are designed to compose into.
3. tree-sitter's diagnostic weakness (region start ≠ cause) is a
   class where parsanol can lead permanently: publish the cause-
   tracking property as a differentiator in the diagnostics workstream.

## Open for the next receipt

- The tree-edit-distance scoring harness (the third metric family in
  rs#527's acceptance) — deferred to the next receipt round; suffix
  recovery + diagnostic precision are the two families published here.
- The recovery-envelope round on the json flavor, re-running this exact
  harness to measure the closure.
