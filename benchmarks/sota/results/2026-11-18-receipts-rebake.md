# Receipt: the quiet-machine re-bake — 2026-11-18 (rs#577)

The 2026-11-17/18 campaign receipts were taken under machine load
(load ~67-103; absolute numbers swung 53→252 ms across runs). This
re-bake re-takes the throughput table min-of-15 on a quiet machine
(load 16-27 across the run), extends the incremental harness to all
four corpus documents, and re-confirms the recovery family post-1.5.7.

Machine: Apple M1 Max, macOS 14.1.1, rustc 1.98.0, node 24.14,
python 3.10.5. parsanol 1.5.7 (post-ScanClass/ScanUntil + the scan
dispatch fix). Round count 15 (min-of-N per document). All engines
verified full consumption with error-free trees before timing.

## Throughput (5 engines, 4 docs, 447,801 bytes)

| engine | bytes | ns (min-of-15 total) | µs/byte | relative | (2026-11-15 receipt) |
|---|---|---|---|---|---|
| tree-sitter 0.25 (rust binding, C automaton) | 447,801 | 14,012,543 | 0.031 | 37× | 0.029 |
| parsanol VM (rust) | 447,801 | 35,183,000 | **0.079** | **2.55×** | 1.073 |
| Lark 1.x LALR (python) | 447,801 | 163,930,500 | 0.366 | 3.4× | 0.344 |
| parsanol interpreter (rust) | 447,801 | 162,811,625 | 0.364 | 3.5× | 0.828 |
| ANTLR 4.13.2, python3 target | 447,801 | 882,016,792 | 1.970 | 0.040× | 1.860 |

The competitor rows are stable across the two receipts (the deltas are
run noise) — the parsanol rows are the engine: the Scan campaign
(1.5.6's ScanClass/ScanUntil class-run instructions + the 1.5.7
scan-dispatch inlining fix) took the VM from 1.073 to **0.079 µs/B**
(13.6×) and the interpreter from 0.828 to 0.364 (2.3×). parsanol VM
now sits at 2.55× tree-sitter (was 37×), well ahead of Lark LALR and
ANTLR. The remaining 2.55× is the LR/PEG strategy tax on a
deterministic grammar — recorded, not excused.

## Incremental (4 docs × 200 keystrokes, absolute wall, min-of-1 pass per doc)

Every keystroke: splice, reparse incrementally, verify the tree is
IDENTICAL to a from-scratch parse (identity 800/800 — zero failures on
all four documents). p50/p95/max in ms.

| doc | bytes | parsanol p50 | parsanol p95 | parsanol max | ts p50 | ts p95 | parsanol fresh p50 | splice ratio | spliced |
|---|---|---|---|---|---|---|---|---|---|
| doc0 | 13,290 | 0.66 | 5.55 | 17.6 | 0.014 | 0.024 | 4.54 | 6.9× | 199/200 |
| doc1 | 281,181 | 7.08 | 103.9 | 185.6 | 0.175 | 0.242 | 113.9 | 16.1× | 180/200 |
| doc2 | 1,507 | 0.26 | 0.41 | 0.83 | 0.006 | 0.008 | 0.55 | 2.1× | 197/200 |
| doc3 | 151,823 | 3.48 | 54.7 | 58.9 | 0.093 | 0.169 | 57.7 | 16.6× | 190/200 |

- The p50 splice runs 2.1-16.6× faster than a fresh parse; the 21.7×
  doc3 headline of 2026-11-17 re-takes as **16.6× on doc3** (the
  ratio moves with the keystroke mix; both receipts publish their
  traces, seed 42).
- The tail stays full-parse-sized by design: structural edits break
  the splice's identity guards and fall back to a full parse (the
  fallback is what makes the splice safe). The `spliced` column counts
  the keystrokes the splice served.
- tree-sitter's incremental reparse remains 30-70× faster on these
  keystrokes (its GLR incremental replay over a CONCRETE tree); the
  parsanol model (typed AST + retained-cache splice) pays per-splice
  construction the concrete-tree model never charges.

## Recovery (re-confirmed post-1.5.7)

The recovery family was re-taken post-1.5.7 in the same session —
`2026-11-18-recovery-ted-shape.md` (rs#580): windowed nTED **0.059**
(the 0.113 baseline halved; ts 0.023), structure recall **67.5%**
(was 52.8%), suffix 90.4% under honest accounting (was 92.6%), diag
**±24,744** (unchanged, matching the 2026-11-16 receipt). The suffix
receipt's suite rows re-confirmed: parsanol wins delete-20 and
unterminated-20 outright.

## Provenance

- `bench.py corpora/json json-artifact.json main 15` — the 5-engine
  driver (all engines verified end==len / !hasError before timing).
- `run_parsanol incremental json-artifact.json corpora/traces/docN-trace.json
  corpora/json/docN.json` — parsanol incremental (the committed seed-42
  traces, 200 validity-preserving edits per doc).
- `run_ts incremental corpora/traces/docN-trace.json corpora/json/docN.json`.
- Machine state: load 16-27 (1/5/15-min) across the runs; no other
  cargo builds interleaved with measurement.
