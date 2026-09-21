# 7. Dense rule-call memo for the VM — measured negative, not shipped

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
