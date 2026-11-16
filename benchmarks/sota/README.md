# Industry benchmark suite (rs#526)

Honest head-to-head receipts for parsanol against established industry
parsers, on shared corpora with shared methodology. Losses are published
with the same prominence as wins — a benchmark suite that only shows
victories is marketing, not measurement.

## Methodology

- **Shared corpus.** Every engine parses the same byte-exact documents
  (hashes in each receipt). No engine parses a friendlier subset.
- **Native runners.** Each engine runs from its canonical host language
  binding; no cross-language FFI handicaps.
- **Full-consumption proof before timing.** Every runner asserts the
  parse covers the whole document with no errors before any timing is
  accepted (`end == len`, `!hasError`, or the language equivalent). A
  bench that silently short-parses is worthless (the rs#496 lesson).
- **min-of-N per document**, N=15; totals aggregate per-document minima.
- **Receipts are dated and immutable.** New hardware or engine versions
  produce new receipts; old ones are never overwritten.

## Layout

- `corpora/<lang>/` — shared corpus documents per language.
- `json-artifact.json` etc. — the compiled parsanol grammar artifacts.
- `runners/` — per-engine runners, same line protocol
  (`<doc> <bytes> <ns>` per doc, `TOTAL <bytes> <ns>`):
  - `runners/parsanol/` — the parsanol runner (`vm|interpreter` engines).
  - `runners/ts-rust/` — tree-sitter via the Rust binding.
  - `runners/run_lark.py` — Lark LALR via the Python binding.
  - `run_tree_sitter.js` — node binding runner (kept for reference: the
    node binding throws `Invalid argument` on inputs ≥ ~150 KB, so the
    Rust binding is the benchmark path).
- `bench.py <corpus> <artifact> <entry> <rounds>` — the driver; emits
  the receipts markdown table.
- `results/` — dated receipts.

## Running

```
# build runners
(cd runners/parsanol && cargo build --release)
(cd runners/ts-rust && cargo build --release)

# JSON suite
python3 bench.py corpora/json json-artifact.json main 15
```

## Honesty clause

Every receipt publishes all rows, including losses, with profiles and
analysis of where the bytes go. The first JSON receipt shows parsanol
behind both Lark LALR and tree-sitter; that gap is the perf workstream's
baseline (see the receipt for analysis).
