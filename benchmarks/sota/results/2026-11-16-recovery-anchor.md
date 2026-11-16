# Receipt: the recovery spine-anchor generalization — 2026-11-16 (rs#539 layer 1)

rs#539's finding: the recovery rounds anchored only at the ROOT's
direct repetition — json's root (`json = ws value ws`) is not one, so
declared resyncs never engaged and the recovering parse produced no
partial structure at all (`captures = {}` measured).

## The change

1. **The spine anchor walks the grammar.** `substantial_spine` is a
   DFS from the root: the first UNBOUNDED repetition whose body drives
   a rule call (contains an Entity/Named — bare char-class bodies like
   ws are not a recovery granularity), descending through Named/
   Entity/Ignore/Trivia/Capture/Scope wrappers, sequences, and
   alternatives. For json it lands on the member-list repetition —
   exactly the granularity corruption recovery wants.
2. **The rounds' cursor seeds from the journal's consumption boundary**
   (was byte 0 — recovery never reached where the good parse stopped).
3. **Recovered iteration spans ride the wire** (`recovered_spans`,
   offset-exact — the parslet-compatible shape merges string items and
   drops per-item offsets, so the rounds emit their spans directly).
4. The json flavor declares `resync value` / `resync member` (junk =
   the complement of the structural boundaries; resume AT the boundary,
   leaving the enclosing list's comma intact) — pubid-grammar lands
   the same change; the bench artifact carries it.
5. The runner credits the rounds' coverage (consumed ∪ furthest span
   end − junk) and walks recovered spans for the recall column.

## Results (60-case harness, min over seeded runs)

| metric | parsanol (anchored) | tree-sitter |
|---|---|---|
| structure recall (mean) | 43.5% (delete-20 87.5%, insert-20 100%) | 65.0% |
| suffix recovery | 14.7% | 70.6% |
| diagnostic distance | ±24,744 (win holds) | ±56,349 |

The gap decomposition sharpened: single-site rows tie (both engines
stop or stumble at the first break); the recall on depth-20
delete/insert rows reaches 87–100%; the mutate rows' 0% is the
nearest-span tolerance missing under substitution noise — the nearest-
span matching needs span-aware tolerance (next layer). The suffix gap
decomposes into: the rounds recover locally around the failure
(~8 iterations) but do not yet re-drive the full suffix replay —
the remaining #539 layer, now measurable per class.
