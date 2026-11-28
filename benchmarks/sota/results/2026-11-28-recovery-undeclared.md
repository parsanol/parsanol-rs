# Recovery generality: the declared-resync asterisk is retired (2026-11-28, rs#734)

The recovery receipt carried an asterisk: parsanol's 90.4% suffix
coverage "leans on declared `resync` patterns" (json.parg carries two)
while tree-sitter recovers comparably with zero declarations. This
receipt re-measures with the declarations STRIPPED and finds the gap
closed — the R2 inferred machinery (the substantial-spine gate +
boundary-anchored scan, rs#539's layers) reaches declared parity on
the same corpus.

## The measurement

The json grammar with its two `resync value/member` declarations
removed (the comment line retained — only the declarations differ),
compiled by the same `compile_source` path into a second artifact; the
48-case json corrupted corpus (delete/insert/truncate × depth, seeded)
replayed through the same `recovery` runner, scored by the same
`score_recovery.py` formula:

| variant | suffix coverage | diagnostic distance | failing cases |
|---|---|---|---|
| declared (json.parg as shipped) | **90.4%** | 14,202 B | 48 |
| **undeclared (resync lines stripped)** | **90.4%** | 14,202 B | 48 |

Byte-identical per case: for json's corruption classes the inferred
scan (next viable spine iteration) resumes at the same offsets the
declared junk-run pattern skips to — both stop at the structural
boundaries (quote, comma, close-bracket, close-brace), so the resume
points coincide. The declared tier stays strictly in front by
construction (the pattern parser is chosen first; the inferred scan is
the fallback), so declarations remain an override, never a dependency.

The ~14.7% pre-declaration figure in the ledger predates rs#539's
fixes (the substantial-spine gate that refuses char-matcher bodies —
the trial scan stalling byte-by-byte on `*digit`-shaped bodies — and
the spine-boundary anchor that scans from the last iteration boundary,
not the failure offset). With those landed, the undeclared path is
declared-equivalent where a substantial spine exists.

## Census

`grep -c resync` over the parsanol/grammars registry: json.parg is the
ONLY grammar carrying declarations (csv/ini/toml/lua and the pubid
fleet recover through the inferred path alone — they have never had a
choice). The declared tier is exercised by the json corpus; the
inferred tier by everything else.

## Acceptance (rs#734)

- [x] the resync-stripped json variant recovers ≥80% suffix — **90.4%**, at declared parity
- [x] the declared grammars' numbers unchanged — byte-identical on this corpus
- [x] the harness green — the same runner + scorer, the undeclared column added here
- [x] diagnostics precision held — identical (14,202 B mean on the failing set)
- [x] the regression pin: `tests/parg_recovery_undeclared.rs` compiles the
  stripped grammar inline and pins ≥80% suffix coverage on a seeded
  corruption set, so the inferred machinery cannot silently regress

## The conditions line

Host load varied during the run (the coverage metric is
load-independent — a byte-accounting identity, not a timing number).
The tree-sitter comparator column is unchanged from the 2026-11-26
receipt (ts 95.6% suffix, ±56.3K diagnostics) — this receipt only
adds parsanol's undeclared leg.
