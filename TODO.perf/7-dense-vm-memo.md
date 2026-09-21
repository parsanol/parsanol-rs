# 7. Dense rule-call memo for the VM — measured negative, not shipped

> **2026-09-21 status:** the memo's VALUE problem was solved from the
> other direction — selective rule memoization (#113, shipped in
> 0.8.4) makes the HashMap memo net-positive for dynamic-free rule
> subtrees in mixed grammars. The dense-table idea below stays
> negative as measured; the hashbrown memo it benchmarks is now the
> production path.

Hypothesis: the VM's `HashMap<(usize, usize), MemoEntry>` rule-call
memo costs a tuple hash plus hashbrown overhead per memoized call,
and a DenseCache-style open-addressed table (inline keys, linear
probing) would close part of the gap that makes giant grammars route
to the tree-walker.

Measured (same-conditions A/B on benches/large-input.rs, interleaved
runs under load):

- Preallocated dense table (one slot per input byte): 1.8x REGRESSION
  on 64 KB inputs — a fresh VM is built per parse, so the table's
  slot-array memset faults hundreds of KiB of cold pages before the
  first instruction executes.
- Grown dense table (start 1K slots, double + rehash): still 0.87–0.98x
  on 64 KB and 0.32x on 2 KB — growth rehashing costs more than
  hashbrown's allocation path at these scales.

Conclusion: hashbrown's group-probe HashMap is already competitive at
VM memo sizes; the walker-vs-VM gap on giant inputs lives elsewhere
(candidate levers if it ever matters again: memo value cloning
(Option<AstNode> deep copies), backtrack-frame traffic, not the map).
Reverted; the HashMap memo stays.
