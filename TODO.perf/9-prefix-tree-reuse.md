# 9. Prefix-tree reuse (specced, unscheduled)

The measured ceiling of edit-span memo retention (items 4/8) is the
structural re-walk: every keystroke still traverses the whole rule
structure against memo hits (~2 hits/line on the KV benchmark,
~1.5–1.7x total). Single-digit-millisecond keystrokes on
hundred-KiB-plus documents require reusing the previous parse TREE
for the unaffected prefix — the tree-sitter design:

- Persist the previous AST (arena-backed) alongside the memo window.
- On edit at offset O: the reused prefix is the deepest ancestor
  chain whose span ends at or before O; splice its subtree, reparse
  the suffix, rebuild ancestors' containers along the right spine.
- Node identity/height bookkeeping decides splice points (a parent
  cannot be reused when the edit lands inside its span).

This is a parser-architecture change (persistent trees, not just
persistent memo), gated by the same tree-equality corpus as item 4.

## Round 2026-09-24: foundation + two cache bugs + honest negative

**Two real bugs found by the new gates (both fixed):**

1. **Boundary-touching retention was unsound.** Retention kept entries
   with `end_pos <= cutoff` — but an entry ENDING exactly at the edit
   offset used the byte at that offset as its match boundary (maximal
   runs stop there). An edit can make the run extend differently, and
   the stale span replays (KV corpus: insert "xx" right after a key
   run → parse consumed 80 of 382 bytes). Retention is now STRICT
   (`end_pos < cutoff`) in all four retain sites.
2. **`DenseCache::insert` recycling wiped snapshot-marked entries.**
   The max_entries recycle cleared the WHOLE cache, cross-parse
   retention included — measured 0 hits across a 100-keystroke session
   on 911 KiB (every parse after the first ran cold). Recycling now
   preserves snapshot entries and grows the slot table instead when
   nothing else remains.

**Retained-tree API (foundation for the splice):**
`IncrementalParser::parse_retained / parse_with_edit(s)_retained /
retained_arena` — the session owns its output arena, so retained
entries need NO adoption copy (identity adopt + `snapshots_in_live`
hit path), full container entries survive, and the previous parse TREE
persists alongside the memo window. Gated by
`parsanol/tests/incremental_retained.rs`: 40-edit deterministic
sessions (insert/delete/replace + a boundary edit at offset 0) must
match a full cold reparse's OUTCOME — tree deep-equal on success,
error payload identical on failure (edits can invalidate the input;
both engines must agree on that too). Session budget: the persistent
arena resets wholesale past 64 MiB.

**Honest negative (TODO.perf/7 discipline):** full container
retention through the dense memo is a NET LOSS — 0.40x mean / 0.53x
p95 vs the terminal-only snapshot tier on the 911 KiB / 100-keystroke
bench (`examples/incremental_retained_bench.rs`, both tiers gated
per keystroke). One `max_entries` budget cannot serve the live parse
window AND the retained prefix at once; the recycles and the
O(entries) retain passes dominate. Denser memoization is NOT the
path — the win must come from the actual splice: parse only the
suffix from the parent grammar position and rebuild the right spine,
which the retained-arena API now makes possible (same-arena subtree
reuse is O(1); the adoption copy that motivated the item-4 ceiling
is gone). The retained API ships as the foundation and stays
NON-default until the splice lands.

## Splice v1 shipped (2026-09-24, later): correctness complete, perf pending

`try_splice` runs inside the retained family for repetition-spine
documents: chain-walk the previous body-item boundaries to the last
intact item before the edit, parse ONLY the suffix iterations, rebuild
the root from grafted prefix items + new suffix items (re-wrapping the
Named hash). Fallback to the full retained parse on any mismatch; the
64 MiB arena reset also drops the graft state (stale-node fix).

Honest status: the splice is CORRECT (all gates pass through it) but
not yet a measured WIN — at 911 KiB the per-edit overhead (multiple
O(entries) retention scans, the 911 KiB set_input clone, the graft
copy) still exceeds the saved traversal, measuring 0.59x vs the
snapshot tier. The next lever is fusing the three per-parse cache
scans (pre-drop retain, chain collect, post-retain) into one pass and
dropping the per-parse input clone. Until then the splice stays
NON-default behind `parse_*_retained`.
