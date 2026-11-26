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

## Round 2026-11-27: the re-run after the Scan round — construction stays second-order; the item closes

The note above promised a re-run once the Scan round landed; it
landed (item 11's Entity-extraction leg, 2026-11-27). The same
strip-`as`-and-bake method re-run on the current engine: the
capture-free variant no longer isolates construction at all — the
stripped grammar's loops ROUTE differently (the capture-free shapes
take the DfaRun/bulk lanes the captured shapes refuse), so the
delta now measures routing gains against construction cost, not
construction alone (the json corpus: captured 6.54 ms vs stripped
6.63 ms total — parity within noise).

The conclusion stands a fortiori: construction was ~11% when
per-char dispatch dominated; the bulk-run rounds ate the dispatch
share, so construction's share only grew smaller relative to the
whole. The ≥30% gate for the lean node encoding is nowhere near
met, and the pooled-write work stays deprioritized with its
original evidence (the incremental session already pools). The
split method's caveat is recorded: post-routing, capture-stripping
is no longer a valid construction isolate — a recognition-only
runner mode (`--no-ast`) would be, and remains the method if the
question ever reopens. The item closes.
