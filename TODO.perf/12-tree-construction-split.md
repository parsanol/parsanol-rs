# 12. Recognition/construction split + pooled tree writes

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
