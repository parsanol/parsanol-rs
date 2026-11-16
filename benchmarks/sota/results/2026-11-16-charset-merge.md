# Receipt: single-byte choice merge — 2026-11-16 (rs#533 follow-up, TODO.perf/11)

The ws shape — a choice of single-byte matches inside a repetition —
paid an interleaved choice per character. The merge compiles a choice
whose branches are ALL bare single-byte atoms with pairwise-distinct
bytes to ONE charset: the same byte is consumed and the same span
value produced whichever branch matches, so acceptance and trees are
identical by construction — sound at ANY repetition depth (unlike
ByteDispatch, whose per-alternative values forced the depth gate).

Extraction is compile-time and strict: `Str` of exactly one byte, or
an `Re` class probed over all 256 bytes to match exactly one (probe
on one-byte haystacks — multi-byte codepoints fail structurally).
Anything wrapped (Named/Capture/...) or multi-byte refuses the merge.

## Results (same corpus, min-of-15)

| build | VM µs/B | interpreter µs/B |
|---|---|---|
| 1.5.1 release baseline | 1.086 | 0.828 |
| hot-path round (#536) | 0.153 | 0.464 |
| determinism dispatch (#538) | 0.125 | 0.457 |
| + single-byte merge (this round) | 0.078 | 0.409 |
| + Span for merged-choice bodies (final) | **0.074** | **0.432** |

The final increment extends the existing Span fast path (bare-class
repetition bodies already lowered to the vectorized maximal-run
instruction) to merged-choice bodies: a choice of bare distinct
single-byte matches consumes the same run as one charset of the
union, so json's `ws` drops its per-character choice machinery
entirely — the spec now pins ONE Span and ZERO Choices for the ws
shape. Cumulative **14.7×** on the VM. Context: Lark LALR 0.344 (4.7×
behind us now), tree-sitter 0.029 (2.5× ahead).

## Verification

- Full suite 638 green (new spec: `single_byte_choices_merge_to_charsets_inside_repetitions` — exactly ONE charset for the ws shape, acceptance identical on mixed whitespace).
- The 60-case recovery harness output byte-identical.
- The remaining per-char cost is the repetition loop's own
  Choice/RepCount/PartialCommit iteration control — the true `Scan`
  (bulk runs, TODO.perf/11) is the next lever.
