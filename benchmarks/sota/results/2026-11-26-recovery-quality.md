# The recovery-quality receipt: parsanol vs tree-sitter (2026-11-26)

The corrupted-json corpus (61 cases: delete/insert/mutate/truncate/unterminated at depths 1/5/20), the byte-identical recovery protocol -- both runners fully consume each document, the span-aware scorer matches recovered spans by containment.

**TOTAL: parsanol suffix contamination 14.9% (diag ±24743) | tree-sitter 70.6% (diag ±56349)** -- 4.7x less error damage; truncation diagnosed to ±1 byte (tree-sitter: ±17-25K).

```
| class | depth | n | parsanol diag± | parsanol suffix | ts diag± | ts suffix |
|---|---|---|---|---|---|---|
| delete | 1 | 4 | 30869 | 0.0% | 30869 | 0.0% |
| delete | 5 | 4 | 26700 | 29.2% | 38769 | 43.4% |
| delete | 20 | 4 | 19426 | 20.8% | 72796 | 94.2% |
| insert | 1 | 4 | 47364 | 0.0% | 47364 | 25.0% |
| insert | 5 | 4 | 53108 | 52.0% | 57227 | 99.9% |
| insert | 20 | 4 | 1021 | 7.9% | 95232 | 98.8% |
| mutate | 1 | 4 | 31124 | 0.0% | 31122 | 49.9% |
| mutate | 5 | 4 | 46536 | 35.4% | 82720 | 95.5% |
| mutate | 20 | 4 | 13412 | 19.9% | 95242 | 96.0% |
| truncate | 1 | 4 | 1 | — | 22655 | — |
| truncate | 5 | 4 | 1 | — | 24832 | — |
| truncate | 20 | 4 | 1 | — | 17332 | — |
| unterminated | 1 | 4 | 9268 | 1.0% | 34336 | 73.7% |
| unterminated | 5 | 4 | 18081 | 13.0% | 49549 | 97.6% |
| unterminated | 20 | 4 | 6 | 0.1% | 40962 | 73.5% |

TOTAL: parsanol suffix 14.9% (diag±24743) | tree-sitter suffix 70.6% (diag±56349)
```

The engine: parsanol-rs @ 1.5.22 (8a11e7f). The repro:
```
runners/parsanol/target/release/run_parsanol recovery corpora/json-corrupted > /tmp/p.txt
runners/ts-rust/target/release/run_ts recovery corpora/json-corrupted > /tmp/t.txt
python3 score_recovery.py corpora/json-corrupted/manifest.json /tmp/p.txt /tmp/t.txt
```
