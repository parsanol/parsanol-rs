# SOTA 2027 STATUS — 2026-11-19

## Shipped (merged + measured, all receipts in benchmarks/sota/results/)

**Throughput** (TODO.perf/10-13, rs#574):
- ScanClass/ScanUntil bulk kernels + the scan-dispatch fix (1.5.6/1.5.7):
  1.073 → 0.079 µs/B (the 2026-11-18 quiet-machine re-bake, rs#577).
- Slice 1 (rs#585): the fixpoint determinism prover — exact per-rule
  FIRST sets at the least fixed point, cycles included; the analysis
  surface the automaton routing rides. Emission stayed gated by
  measurement (the json value-choice dispatch = a 13% loss, recorded).
- Slice 2 (rs#586): the dispatch depth gate fell — the charset merge
  absorbed the rs#533 per-char shape; json **0.079 → 0.0599 µs/B
  (−20.6%, every interleaved round wins)**. tree-sitter 0.031: the gap
  is now **1.93×** (was 37× on 2026-11-15). Ahead of Lark LALR (0.366)
  and ANTLR (1.970) by 6-33×.
- Slice 4 SPECCED + filed (rs#589, TODO.perf/13): the mixed-rep bulk
  scan for the string body (the histogram: zero bulk-scan ops in the
  compiled json program; strings ≈ 60% of the corpus bytes run the
  per-char Choice). The build-side tree treatment it needs is the same
  the neg-scan solved — designed, not yet built.

**Constrained decoding** (rs#530): the TokenOracle + StepState
(rs#569), the llguidance-compatible TokenBitmask wire + MaskEmitter +
the generate_relaxed DFS (rs#579 — the dead `{"a"}` rollout recovers
through the colon branch; FSM mask compilers cannot), the throughput
receipt (rs#584: validity 100% on both grammars; the oracle probe
named as the cost center with the arena-reuse lever).

**Recovery** (rs#576): the spine replays the unchanged prefix —
windowed nTED **0.113 → 0.059** (ts 0.023), structure recall
52.8% → 67.5%, 0/60 null recoveries (was 24); diagnostic distance
±24,744 kept (wins vs ts ±56,349).

**Incremental** (rs#577): identity **800/800** across all four corpus
docs, absolute p50/p95 published, splice ratios 2.1-16.6× own-fresh
(>10× on the big docs; the gate's ratio).

**Expressir adoption** (rs#567): the corpus inventory LSP gate is now
**REQUIRED** — byte-exact against the ruby lane across every
capture-START family (expressir-rs #116/#119/#121/#122). The symbols
gate's blocker = parsanol-ruby#535 (the ruby engine's size-cap span
truncation, nine-cell-matrix filed) + family 4.

**Releases**: 1.5.8 AND 1.5.9 trains — every runtime (crates.io,
rubygems, npm + both wasm tiers re-baked, PyPI, ts, py) verified at
each tier. The Ruby FFI gate un-red after two days (#583).

## Deferred + why

- **Throughput ≤ 0.03 (beating tree-sitter)**: 1.93× remains. The
  lever is specced (slice 4, the string-body bulk scan — the largest
  measured block), then the per-rule LR/automaton routing (Phase 2)
  behind the #585 prover. Both need build-side tree treatment that
  the corpus identity gates hold to byte-identical output — the
  mission's own law, which is why they are designed-and-filed rather
  than hot-patched.
- **The symbols gate required**: blocked on the ruby engine fix
  (parsanol-ruby#535) and family 4 (the synthetic ?/self nodes).
- **expressir-rs v2.4.69 crate publish**: the version bump is on
  main; the publish fails on the release workflow's token scoping
  (expressir-rs#120) — an owner-side credential fix, then a rerun.
- **The registry move (TODO.sota/10) + #532 adoption kit**: gated on
  the owner's CI-pin call and the tables, per the ledger.
- **TODO.sota/1 recovery >70% suffix**: met at 90.4% (honest
  accounting; was 92.6% inflated), diagnostic distance won — CLOSED
  by #580's receipts.
