# 12. Recognition/construction split + pooled tree writes (SPLIT MEASURED 2026-11-16)

tree-sitter builds a pooled concrete tree at ~ns per node; parsanol
builds a typed AST with arena interning (hash keys interned per
object). Before cutting, quantify: the receipt names a `--no-ast`
runner measure that parses to end positions only — recognition cost
vs construction cost, published.

## Work

- `--no-ast` mode in the benchmark runner (skip value construction;
  recognition only). Publish the split in the next receipt round.
- Pool the tree writes: reuse arena storage across parses where the
  incremental session already does; measure interning cost per node
  (hash keys are the suspect on JSON objects).
- If construction ≥ ~30% of the total, lean the node encoding
  (InputRef spans over interned strings where lossless).

## Gate

The receipt table gains a recognition-only row per engine; any node
encoding change stays byte-identical under the corpus gates.

## Round 2026-11-16: the split measured — construction is ~11%

A capture-free variant of the json flavor (every `as name` stripped,
baked at the same compiler) runs the rs#533 harness: VM 0.102 →
0.090 µs/B. Tree construction is ~11% of the VM's total on JSON;
**recognition dominates** — the residual 4.3× to tree-sitter lives in
per-char scan dispatch and loop constants (item 11's Scan), not in
node building. Pooled-tree-write work is therefore deprioritized
until the Scan round lands; this measurement method (strip `as`,
bake, harness) is one command to re-run after each engine round.
