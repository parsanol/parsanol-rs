# Benchmarks

Industry head-to-head receipts for parsanol against established parsers
(tree-sitter, Lark, ANTLR), plus both parsanol engines. The harness,
corpora, traces, corrupted cases, and dated receipts live in
[`benchmarks/sota/`](../benchmarks/sota/); this page publishes the
current tables.

## Methodology

- **Shared corpus.** Every engine parses byte-identical documents
  (hashes committed per receipt).
- **Native runners.** Each engine runs its canonical binding; no
  cross-language handicaps.
- **Full-consumption proof before timing.** Every parse is asserted
  complete and error-free before its timing counts.
- **min-of-N per document** (N=15).
- **The honesty clause.** Losses publish with the same prominence as
  wins, with the profile of where the bytes go.

## Raw parse throughput — JSON corpus (447,801 B, 2026-11-15)

| engine | µs/byte | relative |
|---|---|---|
| tree-sitter 0.25 (rust binding) | 0.029 | 37× |
| parsanol VM — charset-merge build (2026-11-16) | 0.074 | 14.7× |
| Lark 1.x LALR (python) | 0.344 | 3.1× |
| parsanol interpreter — charset-merge build | 0.409 | 2.0× |
| parsanol 1.5.1 interpreter (1.5.1 release) | 0.828 | 1.30× |
| parsanol 1.5.1 VM (1.5.1 release) | 1.073 | 1.0 (baseline) |
| ANTLR 4.13.2 (python3 target) | 1.860 | 0.58× |

The Phase-0 hot-path round (deferred success-path diagnostics,
periodic timeout sampling, Arc'd regex caches) is profiled and
receipted in full: [the hot-path receipt](../benchmarks/sota/results/2026-11-16-vm-hotpath.md).

parsanol beats ANTLR by 1.7×; the LR-family engines (tree-sitter,
Lark LALR) lead on deterministic grammars. The named lever: when a
choice set is first-set disjoint (derivable at compile time), ordered
choice collapses to indexed dispatch — the determinism fast path.
Details and analysis: [the JSON receipt](../benchmarks/sota/results/2026-11-15-json.md).

## Error recovery — corrupted JSON (60 seeded cases, 2026-11-16)

| metric | parsanol (suffix replay) | tree-sitter 0.25 |
|---|---|---|
| suffix recovery (mean) | **92.6%** | 95.6% |
| diagnostic distance (mean, lower is better) | **±24,744** | ±56,349 |
| diagnostic on truncation | **±1 byte** | root-ERROR at 0 |

parsanol wins diagnostic precision (its deepest-failure position tracks
the corruption cause; tree-sitter's ERROR regions start far from the
cause) and closes the suffix gap from 14.7% to 92.6% — delete-20 rows
win outright (96.2% vs 94.2%). Span-aware structure recall: 52.8% vs
72.2% — tree-sitter's recovery tree keeps coarse non-error containers
(one surviving root covers every ground-truth span; parsanol emits
fine-grained per-iteration spans). Windowed tree-edit distance
(±1 KB, ≤80 nodes): parsanol 0.113 vs tree-sitter 0.023 — a published
loss: ts hole-punches the original tree (everything outside the ERROR
region is byte-identical to its clean parse) while parsanol rebuilds
forward at the recovery spine's granularity; unterminated-20 is the
one win (0.103 vs 0.122). Receipts:
[the suffix-replay receipt](../benchmarks/sota/results/2026-11-16-recovery-suffix.md),
[the TED receipt](../benchmarks/sota/results/2026-11-17-recovery-ted.md);
history: [the layer-1 receipt](../benchmarks/sota/results/2026-11-16-recovery-anchor.md).

## Keystroke-scale incremental editing (200-keystroke traces, 2026-11-15)

| engine | 13.4 KB doc p50 | 151.9 KB doc p50 | speedup vs own fresh |
|---|---|---|---|
| tree-sitter incremental | 12.5 µs | 84.4 µs | 34× / 57× |
| parsanol incremental | 10.17 ms | 108.3 ms | 0.93× / 1.16× |

parsanol's incremental path is correct (tree identity held at 400/400
keystrokes) but currently delivers no win: the memo retention rule
drops the root entry on any length change, which for value grammars
invalidates everything (`reused 0/200` on the large doc). The
innermost-container splice retention is the named fix. Details:
[the incremental receipt](../benchmarks/sota/results/2026-11-15-json-incremental.md).

## Reproducing

See [`benchmarks/sota/README.md`](../benchmarks/sota/README.md) — one
command per suite, every runner asserts full consumption before timing.
