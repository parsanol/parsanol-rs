# Receipt: the mask-emitter throughput — 2026-11-19 (rs#530)

The constrained-decoding receipt: validity + the per-token cost of the
`MaskEmitter` loop (rs#579's shipped emitter), release build, min-of-15,
M1 Max, engine 1.5.8 (f174354). The validity scorer = the artifact
oracle itself; the sampler = the lowest-allowed-id closure.

| grammar | generations | validity | tokens | ns / constrained token (end-to-end loop) | overhead vs a 10 ms/token forward pass |
|---|---|---|---|---|---|
| isbn | 200 × 15 | **200/200** | 600 | 583,000 | 5.8% |
| json | 200 × 15 | **200/200** | 400 | 4,797,000 | 48.0% |

Reading it honestly:

- **Validity 100% on both grammars** — the acceptance's scorer. The
  loop is the shipped `MaskEmitter::generate` (no shortcuts): per step
  the mask probes every vocabulary token through the recovering
  envelope, then the artifact parse confirms.
- **The cost center is the oracle probe, not the mask.** Each
  `extends()` call runs `parse_shape_recovering` on
  `prefix + candidate` — a fresh parse (two passes: the strict probe
  + the journaling recovery) with two fresh arenas. The mask
  application itself (the bitset) is nanoseconds. The json grammar's
  0.5 ms/probe against a 10 ms/token model forward pass = ~48% at
  the loop level; isbn's smaller grammar = ~6%.
- **The named lever** (rs#530 follow-up): the oracle should hold ONE
  arena pair and parse incrementally against the live prefix (the
  same arena-reuse discipline the incremental parser uses), taking
  the per-probe cost from the full-parse scale to the delta scale.
  That is the difference between a mask overhead of tens of percent
  (current, honest) and the <0.1% the production logits-processor
  integration needs.

The relaxation demo + the wire fixtures shipped with rs#579; this
receipt closes the throughput box of #530's acceptance with the
overhead honestly recorded and the reduction path named.
