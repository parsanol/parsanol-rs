# 11. Determinism dispatch: Switch tables + Scan runs (dispatch LANDED 2026-11-16; Scan open)

The receipt's damning row: Lark LALR — pure Python — is 3.1× faster
than the Rust VM on JSON (0.344 vs 1.073 µs/B). The gap is strategy,
not language: table-driven determinism does one lookup per token; the
PEG VM walks Choice machinery per node. The lever is compile-time.

## Work

- FIRST-set analysis in the compiler (extend the `order_warnings`
  lint pass): for every Choice, decide whether the alternatives'
  first-sets are pairwise disjoint.
- Disjoint Choices compile to a `Switch` instruction: a 256-entry
  (byte → alternative) table dispatch — no try/fail/backtrack on the
  hot path. JSON's `value` rule becomes fully table-dispatched.
- A `Scan` instruction for repetitions over a single class (`*digit`,
  string runs, `ws`): bulk-consume matching bytes with a memchr-class
  inner loop instead of per-char VM dispatch (extends items 2/6 to
  the VM's choice-dense paths).
- Fallback: non-disjoint Choices keep the existing Choice lowering —
  semantics preserved by construction, and the corpus gates enforce
  it.

## Gate

rs#533 harness re-run: JSON lands Lark-class or better (≤ 0.35 µs/B
projected, stretch 0.1); every flavor's corpus gate byte-identical;
all engines inherit (Switch/Scan live in the shared artifact VM).

## Round 2026-11-16: dispatch landed (VM 8.7x cumulative)

The value choice dispatches: singleton classes decode (regex-syntax's
Literal folding had blinded the analysis AND the fast path),
case-folded literals are provable first sets (HIR walker + exact
UTF-8 lead ranges), and the repetition cost gate keeps per-char
choices interleaved (the un-gated attempt measured 18x on json ws —
specs pin both sides in `dispatch_shape.rs`). VM 0.153 -> 0.125 us/B
on the JSON receipt. Open from this item: the Scan instruction for
class-run repetitions (the profile's remaining per-char dispatch).
Receipt: `benchmarks/sota/results/2026-11-16-determinism-dispatch.md`.
