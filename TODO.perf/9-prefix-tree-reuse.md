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

## Round 2026-10-30: the instrument was lying; the scans are not the cost

Attempted the "fuse the scans + drop the input clone" lever. The
pre-parse retain became a lookup-time invalidation line (pre-parse
entries at/after the cutoff answer misses and are removed on touch,
with linear-probe cluster re-homing — insert never overwrites keys,
so masking alone accumulates shadow duplicates) and the arena input
spliced in place. All retained gates stayed green.

Two measurement lessons, both expensive:

1. **Sequential tier legs are unusable on a shared host.** The
   retained-vs-snapshot ratio, measured with the snapshot leg minutes
   before the retained leg, swung 0.59x -> 1.50x -> 1.97x -> 0.85x
   across runs purely with the load (70-90). The bench now runs the
   two tiers INTERLEAVED — one keystroke stream, two input copies,
   both sessions parsing the same keystroke milliseconds apart — so
   the ratio is the measured quantity.
2. **Under the interleaved instrument the lever is PARITY.** Baseline
   1.05x mean / 1.33x p95 vs the scan-fusion variant 1.04x / 0.99x
   (911 KiB, 100 keystrokes, 89/100 spliced). The pre-parse
   retain+rebuild and the whole-document clone are NOT the
   per-keystroke cost; the attempted machinery was reverted per the
   TODO.perf/7 discipline (unmeasured complexity does not ship). One
   real pathology stays recorded: masking EVERY entry past the line
   (not just pre-parse ones) measured 0.71x — spans crossing the edit
   lost memoization and duplicated until the recycle thrashed.

The dominant per-keystroke cost in BOTH tiers at 911 KiB is the
post-parse `retain_snapshot_adopt` full pass — the retain scan, the
per-entry adopt, and the FULL slot rebuild over ~10^6 entries —
roughly 400 ms of the ~400 ms retained leg. That is the next lever,
and it is the "slot-local drops" follow-up this file already names:
drop stale entries where they probe and fix only the touched cluster,
never rebuilding the table per parse. Until that lands, the splice
stays NON-default behind `parse_*_retained`.

## Round 2026-10-30 (later): the retention path is second-order; the suffix reparse is the workload

Implemented and measured the slot-remap retention (single compaction
pass recording old->new indexes; sequential slot remap; shift-until-
empty chain repair instead of the per-parse probe rebuild). Correct —
the 40-edit gates and the full suite stayed green — and REPARITY:
1.06x mean / 0.99x p95 on the interleaved bench, against the same-day
baseline's 1.05x / 1.33x and the scan-fusion variant's 1.04x / 0.99x.
Reverted per the TODO.perf/7 discipline.

**What three instrument-corrected variants now agree on** (baseline /
scan-fusion / slot-remap all within noise of each other): cache
maintenance on the retained path — the pre-drop scan, the full slot
rebuild, the input clone, the adoption pass — is SECOND-ORDER at
911 KiB. The dominant term both tiers share is the SUFFIX REPARSE:
the keystroke stream spreads edits across the whole document, and an
edit at fraction f of the document forces a reparse of the (1-f)
remainder in EITHER tier — the splice reuses the prefix, the snapshot
tier's memo retention covers the same prefix, and both then parse the
suffix. The measured ~1.0x ratio is the workload's shape, not an
implementation gap.

Consequences for the next round (open design questions, not
scheduled):

1. **The single-digit-ms target requires the suffix reparse to get
   cheaper**, not the retention bookkeeping: either a right-spine
   tree splice (reuse the suffix's unchanged TAIL as well — true
   persistent trees, the original item-9 ask beyond prefix reuse),
   or denser memoization of the suffix (item 7's negative was
   measured on the pre-tree architecture and deserves a re-test
   under the retained tier).
2. **The bench should bucket latencies by edit position** (early /
   mid / late thirds): end-of-document typing — the realistic editor
   case — pays a tiny suffix in both tiers, so the tiers' difference
   (if any) lives in the maintenance band, which the mean over
   document-wide edits hides. Measure there before optimizing there.
3. The retained tier stays NON-default: no regression measured, no
   win measured.

## Round 2026-10-30 (final): the remap lands under the bucketed instrument

The bucketed bench found the case the mean hid: end-of-document
keystrokes pay ~150 ms of pure maintenance in both tiers, and there
the slot-remap retention reproduces a ~15% win across runs (late band
1.14x -> 1.18x/1.19x; the snapshot tier's late mean 166 -> 140 ms via
the shared primitive). SHIPPED (perf/late-band-remap).

Laws this arc burned in:

1. Interleave the A/B legs — sequential legs minutes apart swing 2x
   with host load.
2. Bucket by edit position — a document-wide mean over the suffix
   reparse can hide any maintenance-band win entirely.
3. The maintenance band (~10^6-entry scans/rebuilds) is the retained
   tier's remaining floor; the splice stays NON-default (no measured
   mean-level win), but end-of-document typing — the realistic
   editor case — is now measurably faster in both tiers.
