# Receipt: determinism dispatch round — 2026-11-16 (rs#533 follow-up, TODO.perf/11)

The compiler's ByteDispatch analysis was blind to two shapes the JSON
grammar needs; both fixed, plus a cost gate for the trap the first
attempt hit.

## What changed

1. **Singleton classes decode again.** regex-syntax folds one-member
   classes (`[ ]`, `["]`, `[e]`, `[0]`, `[+]` — JSON's hottest bytes)
   into Literal HIR; `CharRanges::from_pattern` only matched
   `Class::Unicode`, so every singleton-class instruction missed both
   the first-set analysis and the VM's per-char class fast path,
   falling to the regex engine per character.
2. **Case-folded literals are provable first sets.** `(?i:true)`
   lowers to `Concat[Class[tT], Class[rR], ...]`; the new HIR walker
   (classes, literals, alternations, concat-first-child, captures,
   exact UTF-8 lead-byte ranges) proves them, and json's 7-branch
   `value` choice — disjoint by first bytes — lowers to ByteDispatch.
3. **The repetition cost gate.** The un-gated first attempt dispatched
   json's `ws = *( %x20 / ... )` too and collapsed 18× (VM 2.83 µs/B):
   per-node dispatch wins, per-character choices inside `*()` loops
   lose — the table lookup costs more per iteration than interleaved
   charset probes. ByteDispatch is now emitted only outside repetition
   bodies; `dispatch_shape.rs` pins both sides.

## Results (same corpus, min-of-15)

| build | VM µs/B | interpreter µs/B |
|---|---|---|
| 1.5.1 release baseline | 1.086 | 0.828 |
| hot-path round (PR #536) | 0.153 | 0.464 |
| + determinism dispatch (this round) | **0.125** | **0.457** |

Context: Lark LALR 0.344, tree-sitter 0.029. Cumulative **8.7×** with
the typed-AST output intact.

## Observables

- Trees byte-identical (full suite incl. differential + corpus
  conformance + export/generate parity).
- Deepest-failure positions shift a few bytes DEEPER on dispatched
  choices (the selected branch explores further before registering);
  the recovery harness re-scored: suffix 15.0%, diagnostic precision
  ±24,744 vs tree-sitter's ±56,349 — the win holds. The corrupted
  corpus (61 rows) re-ran clean end to end.
- New specs: `dispatch_only_outside_repetitions`,
  `case_folded_literals_are_provable_first_sets`,
  `singleton_classes_decode_to_ranges`.

## Next (item 11 remainder + 12)

- The remaining profile survivors are per-char class scan dispatch —
  the Scan instruction (bulk class runs) is the next compiler lever.
- Item 12's --no-ast recognition/construction split quantifies how
  much of the residual 4.3× to tree-sitter is tree construction.
