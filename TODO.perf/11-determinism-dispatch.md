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

## Scan instruction design (next round, 2026-11-16)

Target: UNCAPTURED repetitions whose body is a single single-char atom
(CharSet, singleton Char/Regex-class, or a Choice of single-char
classes) — json's `ws = *( %x20 / %x09 / %x0A / %x0D )`, `*digit`,
string-content runs. Lowering: `rep_open; Scan(class_table);
build_rep` replaces the per-char Choice/loop machinery — the maximal
run is consumed word-at-a-time.

Soundness gates (identity by construction):
1. The repetition must be VALUE-INVISIBLE: no Capture/Named/Scope
   wrapper reaches it. compile_repetition cannot see its parents —
   thread a `captured: bool` through the wrapper arms (Ignore/Trivia
   force false; Named/Capture/Scope force true; Entity follows the
   referenced rule's wrapper state).
2. Single-char body only (maximal-run == zero-or-more loop semantics
   exactly).
3. Corpus gates byte-identical (48 flavors x 3 engines) + the rs#533
   harness receipt.

Expected: ws/string-content/digit loops drop the per-char Choice +
loop machinery — the profile's remaining recognition dominator after
the ~11% construction split (TODO.perf/12).

## Round 2026-11-16 (2): single-byte choice merge — VM 13.9x cumulative

The ws shape's interleaved per-character choice compiles to ONE
charset (bare distinct single-byte branches only; strict compile-time
extraction — Str-of-one-byte or a 256-probe-exact class; sound at any
depth because consumed bytes and span values are identical by
construction). VM 0.125 -> 0.078 us/B; recovery harness
byte-identical; spec `single_byte_choices_merge_to_charsets_inside_
repetitions`. The remaining per-char cost is the repetition loop's
own iteration control (Choice/RepCount/PartialCommit) — the Scan
instruction (bulk runs) removes it.
Receipt: `benchmarks/sota/results/2026-11-16-charset-merge.md`.
