# The string-scan shape receipt (2026-11-24, option A — rs#574)

The wire change: quote-delimited named rules emit ONE whole-string
span; the per-char iteration spans leave the v2 trees. Landed in
parsanol-rs#666, released in the 1.5.18 train (crates e58cff3).

## The A/B (interleaved, min-of-15, same host, same session)

Control = the v1.5.17 tag binary; treatment = the shape branch (the
commit that became #666). The json corpus (doc0-3, 447,801 bytes
TOTAL), `run_parsanol <artifact> main <corpus> 1 <engine>`:

| lane        | shape      | 1.5.17      | delta |
|-------------|-----------|-------------|-------|
| vm          | 7.92 ms   | 18.52 ms    | −57%  |
| interpreter | 57.3 ms   | 181.0 ms    | −68%  |

**0.0177 µs/B on the vm lane** (7.9229 ms / 447,801 B) — through the
0.03 gate; the projection was ~0.028. The 1.5.15 standing was 0.0372
µs/B = 1.05x of tree-sitter (0.031); the shape puts the vm lane at
**~0.5x tree-sitter — ahead ~1.75x on the json row.**

## What moved

- The interior repetition's per-char dispatch+fold (~18 ns/char
  classic, ~25-45 ns/char routed) → one escape-aware kernel scan.
- The value noise left with the spans: ONE InputRef per string rule
  visit (quotes folded in) instead of per-char children.
- The interpreter benefits more (−68%) — its per-char choice loop was
  the dominant cost.

## The tiers

- crates.io parsanol 1.5.18 (e58cff3).
- rubygems parsanol 1.5.18: the string_span conformance case GREEN on
  the shipped gem (the content assertion).
- npm parsanol 1.5.18: both wasm tiers rebuilt via the candidate
  recipe; the corpus gate 32/32 on the vendored wasm (the tarball
  ships that vendor); @parsanol/wasm 1.5.18 auto-published by the
  release-triggered workflow.
- PyPI parsanol 1.5.18.0: the ride merged (the armv7-musl wheel leg
  lost to the pypa image bump's rust std — noted on parsanol-python#72).

## The remaining perf program

1. #528 incremental (tree-sitter's GLR replay still 30-70x faster on
   keystrokes — the last trailing lane).
2. LR slice 2 (the LrRun executor; the interning fix #670 unblocked
   the spine — both recursive and LEFT-recursive variants build).
3. The 417-site routing tail.
