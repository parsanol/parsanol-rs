# 3. Codegen — shared-prefix split (SHIPPED); JIT (deferred)

## Shipped: compile-level shared-prefix split

Alternatives whose every branch is a sequence starting with the same
atom compile to `<prefix> <choice over tails>`: the deterministic PEG
prefix matches once instead of once per branch, and BYTE_DISPATCH
runs at the post-prefix position where the tails' lead sets are
disjoint — the shared prefix's bytes polluted every branch's union,
which measured as 35 of 63 alternatives blocked in the EXPRESS
grammar.

The split is compile-level only (the grammar is untouched) and each
branch rebuilds its original value envelope (prefix value + tail
values folded by BuildSeq), so trees are identical to the unsplit
compilation — gated by a tree-parity test and the differential
suites. Tails without provably disjoint lead sets keep the
interleaved Choice chain over the same bodies.

A grammar-level rewrite (mutating the atoms) was tried first and
REVERTED: it created nested sequences the value model does not
flatten, changing trees. The compile-level form has no such problem.

## Deferred: cranelift JIT

The original sketch (compile each opcode to cranelift IR once per
grammar, cache native artifacts alongside the TODO.perf/1 artifact)
stays unscheduled: the measured bottleneck on real corpora is
packrat backtracking, not opcode dispatch, and the prefix split
removed the dispatch blocker the JIT was meant to solve. Revisit only
with a fresh profile that shows dispatch dominating again.

## Revisit check 2026-09-24: condition NOT met, stays deferred

Ran the mandated fresh profile (`examples/prof_dispatch.rs`, rs#100
counters, 911 KiB KV corpus):

- walker: 360,004 atom dispatches (0.4/byte), each atom dispatched
  exactly once per pair — zero repeated work;
- bytecode VM: 600,006 instructions (0.66/byte), backtracks=1.

Dispatch is executed once per grammar position and is O(document);
backtracking — the measured bottleneck — is absent on this shape and
remains the target of the incremental work (TODO.perf/8/9). A cranelift
backend accelerates dispatch, which is not the bottleneck; the revisit
condition ("a fresh profile that shows dispatch dominating") is not
met. Stays deferred. The probe example ships so the next revisit is a
one-command rerun.

## Revisit check 2026-11-12: operand-specialized dispatch measured NEGATIVE (jit#458)

The mandated fresh profile ran on a different shape — the iso corpus
grammar through the artifact envelope (jit_profile.rs, this round's
harness): 200 real accept inputs, 220,800 dispatches (1,104 per
identifier, ~5.5/byte — dispatch-dominant by count, unlike KV's
0.4/byte). Str terminals carried 46.3% of dispatches. The revisit
condition ("dispatch dominating") was MET this time, so the
operand-specialized dispatch was implemented three ways behind a
`jit` feature and measured (min-of-9, release, identical inputs):

| design | best-of-3 wall (200 inputs) | vs plain |
|---|---|---|
| plain interpreter | 14.12 ms | — |
| v1: StrPlan table + eval method | 20.87 ms | +48% |
| v2: tuple copy + second index | (worse than v1) | — |
| v3: inlined, split field borrows, scalar snapshot | 21.94 ms | +55% |

All under the feature flag; 541/541 lib tests and the parser battery
green in both configs before measuring.

**Why it loses (the mechanism, for the next revisit):** in a release
build LLVM already fuses `get_atom` (a Vec index) + the `Atom::Str`
enum match + `parse_str` (first-byte reject + memcmp) into the same
code the "specialized" path would generate — the dispatch the profile
counts is not dispatch COST. The plan table then ADDS indirection:
`Vec<Option<StrPlan>>` scatters the operands into cold cloned
allocations (String + Box per atom) while the generic path's pattern
lives in the grammar arena's hot string pool. Same lesson as the
dense-memo negative (item 7): pre-computed side structures pay cold
memory for work the optimizer already fused.

The remaining levers the profile actually suggests: packrat
backtracking (items 8/9) and arena memory layout — NOT dispatch
specialization. Stays closed; the harness ships so the next revisit
re-runs in one command.
