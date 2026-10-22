# B2 — one-shot validator report

Run: **2026-10-22**. Validator: `validate.py` in this directory (written
from `docs/parg-semantics.md`, archived after this run).

## Corpus

`parsanol-rs/corpus/conformance` — 32 artifacts, 125 cases
(`accept` / `reject` / `capture` / `shape`; `example` cases are
bindings-layer and outside the match relation).

### Run 1 — the document taken literally: 117/125

Eight divergences, triaged:

| Cases | Divergence | Triage |
|---|---|---|
| `leftrec_arith` (4 shape) | The document's "inside a capture" (V2/V3) reads as arbitrary nesting, so an empty repetition nested in a sequence yielded `[]`; the engines yield the unnamed arm (`""`) and the sequence merge drops it. | **Spec bug.** The named arm applies to a capture's *direct value only* — the parslet `CanFlatten` contract; a sequence resets the context. |
| `merge_array` (1 shape) | V1 stated `H ⊕ X = [H, X]`, i.e. a map beside an array nests. The engines append flat (`[H] ++ A`). The pinned `merge_array` tree is only producible by the flat rule. | **Spec bug.** V1's table omitted the array operand and misstated the result. |
| `lookbehind` (1 capture) | Tree is `[{emph: …}]` (a root repetition of one capture); the expectation names `emph` directly. | **Validator modeling.** Corpus `capture` tests collect named captures from anywhere in the flattened tree (the suite's `collect_captures` walk). The match relation was correct. |
| `trivia_capture` (2 capture) | Same: `word`/`num` live under `list`/`item` in the flattened tree. | **Validator modeling**, as above. |

Both spec bugs are closed in **parsanol-ruby#293** (V1 merge table, V2/V3/V4
named-context wording, plus the re-trigger note for this validator).

### Run 2 — after the corrections: 125/125

## Fuzz fixtures

Exported by the Ruby differential fuzz spec
(`FUZZ_SEED=20261022 FUZZ_CASES=20 FUZZ_EXPORT_DIR=…`, 20 generated
grammars, 179 inputs, 58 accepted); expected shapes come from the Ruby
native engine, so this leg also cross-validates the independent matcher
against generated grammars rather than the hand-authored corpus.

### Run 3 — corpus + fuzz: 304/304

No skips: the generator's constructs (terminals, sequences, ordered
choice, repetitions, captures, rule references) are all in the
validator's scope.

## Totals of the archived run

```
cases=304 checked=304 ok=304 diverged=0 skipped=0
```

## Disposal

This directory is archived, not maintained (see README). Re-run it when
the match relation changes materially; triage every divergence as an
engine bug or a spec bug, fix the side that is wrong, and archive the new
report alongside this one.
