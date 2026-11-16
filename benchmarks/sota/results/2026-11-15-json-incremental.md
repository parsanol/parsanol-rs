# Receipt: incremental editing head-to-head — 2026-11-15 (rs#528, first receipt)

Workload: 200-keystroke validity-preserving editing traces (seeded,
committed under `corpora/traces/`; the generator is `gen_trace.py`) over
two JSON documents — doc0 (13.4 KB) and doc3 (151.9 KB). Every
intermediate state is valid JSON, verified at trace-generation time.

Per keystroke: splice the text, reparse incrementally, then verify the
incremental tree is IDENTICAL to a from-scratch parse of the same text
(deep content equality for parsanol; a (kind, range, missing) postorder
hash for tree-sitter). All 200 steps × 4 runs = identity-clean, zero
failures.

tree-sitter note: the old tree must be `edit()`ed with the change range
before reuse — without it the reused boundaries are stale and the tree
is silently wrong (our first ts run produced "identity fails at step 0";
that was harness bookkeeping, fixed — the engine requires it).

## Results (per-keystroke reparse latency)

| engine | doc0 (13.4 KB) p50 | doc0 p95 | doc3 (151.9 KB) p50 | doc3 p95 | identity |
|---|---|---|---|---|---|
| tree-sitter incremental | 12.5 µs | 19.3 µs | 84.4 µs | 96.0 µs | 200/200 |
| tree-sitter from-scratch | 423.6 µs | 444.8 µs | 4.80 ms | 4.94 ms | — |
| parsanol incremental | 10.17 ms | 10.61 ms | 108.3 ms | 111.6 ms | 200/200 |
| parsanol from-scratch | 10.95 ms | 11.62 ms | 125.6 ms | 131.8 ms | — |

Incremental speedup vs own from-scratch:

| engine | doc0 | doc3 |
|---|---|---|
| tree-sitter | 34× | 57× |
| parsanol | 0.93× (a loss) | 1.16× (≈ nothing) |

## Honest analysis

**parsanol's incremental path is correct but currently ineffective on
JSON; tree-sitter's is the industry franchise for a reason.**

- **parsanol from-scratch baseline is also wrong-sized**: a fresh parse
  of the edited 13.4 KB doc costs ~11 ms in this runner path, while the
  pure-parse receipts measure the same document at ~14 µs. The delta is
  the runner's per-step arena + input setup (set_input clones the text,
  the arena is rebuilt per step) — measuring harness cost, not engine
  cost. Even discounting that, the incremental session is not faster
  than the fresh path on doc0 (0.93×) and only marginally on doc3
  (1.16×).

  Runner-cost caveat recorded: the from-scratch rows include arena
  construction; a fairer from-scratch baseline would reuse the arena.
  The incremental rows time only `parse_with_edit_retained`.

- **The cache retains nothing on doc3: `reused 0` across all 200
  steps.** The retention rule drops the root entry whenever the input
  length changed (the root must consume the whole new input). For a
  value grammar like JSON, the root hash/array spans the entire
  document — so EVERY keystroke invalidates the entire memo window.
  This is the single dominant lever: retention must splice at the
  innermost unchanged container (the prefix-splice design in
  TODO.perf/9), not just at the root.

- **On doc0, `reused 111/step` but no latency win**: the retained-path
  bookkeeping (snapshot adoption, arena budget checks, dirty-region
  tracking) costs what the memo saves. The machinery needs the same
  treatment the VM crossover got (rs#512): profile, then cut the
  per-step fixed costs.

- **tree-sitter's 34–57× is the bar.** Its cost: the old tree must be
  edited with byte+point ranges (the editor's obligation), after which
  reuse is nearly free (12 µs on 13 KB).

## Where the gap lives (the tuning-round input)

1. **Innermost-container splice retention** — kill the root-drop rule
   for value grammars: keep the memo window from the first unchanged
   container inward; splice the edited spine only. Directly targets the
   `reused 0` failure.
2. **Per-step fixed costs** — snapshot adoption and budget checks in
   the retained path; measure with allocation counters, then cut.
3. **From-scratch baseline hygiene** — the runner's own setup cost must
   not be charged to the engine (next receipt separates them).

## Open for the next receipt

- The tuning fixes above, landed with allocation/counters receipts
  (the issue's final acceptance item), re-run on the same traces.
- ASCiiDoc-scale traces (the editor-market workload) once a flavor with
  envelopes lands; JSON is the v1 corpus.
