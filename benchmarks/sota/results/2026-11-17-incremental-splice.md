# Receipt: the multi-level spine splice — 2026-11-17 (rs#542 slice 2b completion)

The incremental gate is measured, not asserted: an edit inside a deeply
nested member now splices at THAT list's granularity instead of
re-parsing the enclosing member.

## Results (200-keystroke traces, identity = incr tree vs fresh parse per step)

| trace | incr p50 | fresh p50 | speedup | identity | spliced |
|---|---|---|---|---|---|
| doc3 (package-lock, 152KB) | 11.6 ms | 251.7 ms | **21.7x** | 0 failures | 190/200 |
| doc0 | 2.0 ms | 17.9 ms | 9.1x | 0 failures | 199/200 |

Machine note: this session ran under load ~67 (other builds sharing the
box); fresh p50 swung 53-252 ms across runs. The ratio is the receipt;
absolute wall numbers should be re-taken min-of-N on a quiet machine
before quoting them anywhere load-sensitive.

## What changed

1. **The parser records every run.** `spine_watch_bodies` + a frame
   stack record `(body, run_start, depth, item_bounds)` for EVERY
   unbounded run of each spine body — unwatched reps never record.
   This replaces the anchor-drain seed, whose probes failed any body
   starting before its anchor byte.
2. **Innermost-run selection, tree-validated.** The session picks the
   innermost recorded run containing the edit, validates it against
   the previous tree (walk `path_between(body, rep)` to the actual
   deep list node; item counts must match), bridges with ONE small
   parse, and `deep_patch()` reassembles the containers upward
   (`shift_if_above` for straddling siblings, `copy_shift_tree` for
   whole items above the edit).
3. **Failed backtrack attempts are in the records.** Both chain seeds
   pair by count + span-to-end with longest-chain-wins; deep
   candidates must match the tree's item count. The step-68 failure
   class (a retry branch's run bounds patching a wrong interpretation)
   is pinned by the deep-splice spec.
4. **Soundness.** The bridge parse must satisfy the shift identity
   (`new_end == old_end + delta`); the sibling tails parse from the
   rebuilt list's END, not the bridge end (mid-list edits splice now);
   seeded bounds land in the chain and shift with it (pre-edit
   coordinates seeding bounds directly broke the first post-fallback
   splice).

## The known tail

incr p95/max stays full-parse-sized: structural edits (a deleted comma
merging members, shape changes) break the identity guards and fall
back to a full parse by design. The fallback is what makes the splice
safe to be aggressive elsewhere.

## Regressions pinned

- `deep_splice_tests::deep_edit_matches_fresh_parse` — a nested-list
  edit must match fresh AND engage the deep surgery.
- `spine_tests::spines_orders_outermost_first_with_nested_lists` —
  discovery order + `path_between` reachability and no-cycle.
