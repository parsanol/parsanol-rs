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
