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
- Slice 4 CORRECTED (rs#591, TODO.perf/13): the histogram falsified
  the specced premise — the string body is ALREADY bulk-lowered
  (Span: 1 in the compiled program). The remaining 1.93× lives in the
  TREE-BUILD OPS (85 of 253 instructions = the typed-AST writes) —
  Phase 0's pooled writes / the --no-ast split is the measured next
  lever, then the Phase 2 LR routing behind the #585 prover. The
  mixed-rep detector stays parked for the capture-bearing shapes.
- Phase 0 SHIPPED (2026-11-20): the sample profile corrected the
  histogram's read — the cost was the WRITE MECHANICS (the packrat
  memo's default sip hash + un-pre-sized rehash churn, the per-node
  tag/name intern probes, the split_off Vec per node build, the
  20 000-node arena clamp regrowing the pools). All four fixed; trees
  identical by construction. json **0.0599 → 0.0418 µs/B (−35.0%,
  every interleaved round wins)** — tree-sitter 0.031: the gap is now
  **1.35×**. Receipt:
  benchmarks/sota/results/2026-11-20-memo-pooled-writes.md.

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

**Expressir adoption** (rs#567): CLOSED 2026-11-20 — the corpus
inventory LSP gate is REQUIRED (expressir-rs #122), and the symbols
gate went required + CI-wired (expressir-rs#124) after the true root
cause surfaced: the ruby builder indexed source spans with character
addressing on byte offsets (lutaml/expressir#695, merged); the ruby
slice-pool clobbering (parsanol-ruby#538, 1.5.10) was a real bug but
not this symptom. ruby#535 closed with the decode.

**Releases**: 1.5.8 through **1.5.12** trains — every runtime
(crates.io, rubygems, npm + both wasm tiers re-baked, PyPI, ts, py)
verified at each tier; 1.5.10 carries the slice-pool fix and the
depth-cap raise; **1.5.11 carries the Phase 0 pooled-writes win to
every runtime** (2026-11-20, all five registries verified live);
**1.5.12 carries the mask FFI + the LR-provability prover**
(rs#603+rs#604 — rs#530 CLOSED: the constrained-decoding pipeline
end-to-end with the vLLM processor at 200/200 validity on both
grammars; rs#574 Phase 2's analysis layer proves 55.5% of the
fleet's rules automaton-lowerable, slice B = the automaton backend
(designed in TODO.perf/13)). The ts lane also gained the explicit
failure reporter (ts#89) and the Node WASM-poisoning recovery
(ts#86). The Ruby FFI gate un-red after two days (#583).

**1.5.13 carries slice B stage 1 — the DFA table backend**
(2026-11-21, PRs #611/#612 merged): dense `states×256` DfaTable +
Thompson NFA + subset determinization + the rule-level `DfaRun`
routing. Stage 2 (the routed repetition loops: `build_loop_dfa`,
iteration_accepts envelope mode, the value-1 single-terminal body
restriction) lands in the FOLLOWING train — its implementation was
found uncommitted in the session worktree after #611 merged (the
merge carried stage 1 + the differential tests only; the tests pass
trivially without the routing and become real when it lands).
Trains riding 1.5.13: ruby PR #544 (Cargo.lock), ts PR #91 (vendor
bake + package.json — the mask-FFI glue grew the wasm ABI), py PR
#67 (submodule + 1.5.13.0), manifest last per the registry law.

## Deferred + why

- **Throughput ≤ 0.03 (beating tree-sitter)**: 1.35× remains. The
  profile-ranked next blocks = the VM dispatch loop itself and the
  per-char scan kernels, then the per-rule LR/automaton routing
  (Phase 2) behind the #585 prover. Both need corpus identity gates
  held to byte-identical output — the mission's own law.
- **The symbols gate required**: DONE (expressir-rs#124 merged) —
  the drift was the expressir builder's char/byte span bug
  (lutaml/expressir#695), not a capture gap.
- **expressir-rs v2.4.69 crate publish**: the version bump is on
  main; the publish fails on the release workflow's token scoping
  (expressir-rs#120) — an owner-side credential fix, then a rerun.
- **The registry move (TODO.sota/10) + #532 adoption kit**: gated on
  the owner's CI-pin call and the tables, per the ledger.
- **TODO.sota/1 recovery >70% suffix**: met at 90.4% (honest
  accounting; was 92.6% inflated), diagnostic distance won — CLOSED
  by #580's receipts.
