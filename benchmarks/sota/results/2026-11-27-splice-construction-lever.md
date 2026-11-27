# The splice-diet parser-construction lever (2026-11-27): the analysis computed once per session

The profile receipt's first lever ("instrument the uninstrumented
~20% — the per-edit parser construction") answered: the per-edit
`PortableParser` construction ran `ParserAnalysis::compute(grammar)`
— the O(atoms) dependence closure — on EVERY keystroke, in both
session tiers (the retained path's `new_with_cache_in_place` and the
snapshot path's `new_with_cache_and_snap`; the rs#486 note already
provided `with_analysis` for exactly this reason on the artifact
path, but the session paths missed it).

## The change

The session computes the analysis ONCE at construction
(`IncrementalParser::new_with_analysis` / `owned_with_analysis`) and
threads it through two new constructor variants
(`new_with_cache_in_place_and_analysis`,
`new_with_cache_snap_and_analysis`). Every other construction default
is unchanged; callers without an analysis keep the compute fallback.

## The doc0 measurement (200 keystrokes, identity-checked)

| round | incr p50 | fresh p50 | identity |
|---|---|---|---|
| 1 | 350.7 us | 1486.8 us | 0 failures |
| 2 | 356.8 us | 1494.8 us | 0 failures |
| 3 | 351.9 us | 1488.6 us | 0 failures |
| 4 | 354.7 us | 1489.9 us | 0 failures |

Against the corrected baseline's 390 us and the profile rounds'
378–433 us: **−9 to −12%** on the splice's per-edit overhead, and
the fresh p50 drops the same way (1.49 ms vs 1.60–1.67 ms) — the
analysis cost left the fresh leg too. Splice engagement 200/200
throughout; the identity guarantee holds on every round.

## The remaining levers (in the order the profile dictates)

1. The probe's bridge parse (the straddling item's re-derivation —
   ~46% of the per-edit budget; the plan9-routed bodies dispatch
   through the DFA lane where routed).
2. The graft's shifted-copy clone (~27%; the retained arena could
   re-reference instead of re-store — the cross-arena handle
   tension).

## The probe-phase correction + the suffix finding (2026-11-27, later)

The sub-phase instrumentation corrected the profile's remainder
split (the first nav timer swallowed the parse calls — the captured
form reports the true split):

| sub-phase (doc0 medians) | us |
|---|---|
| invalidation | 18 |
| nav (gate + discover_spine + the previous-tree walk) | 17 |
| **the parse calls (bridge + tails + deep probes)** | **~105** |
| graft | ~115 |

The nav is NOT the cost. The parse calls are — and the per-edit
cost **scales with the edit offset**: the doc0 trace's first-20
edits mean 343us vs last-20 mean 1760us (**5.1x**). The mechanism:
the plain retain drops every cache entry whose end sits at/after
the cutoff, so all the suffix members after the edit re-parse from
scratch through the bridge — the splice's suffix re-parse is the
dominant lever, and it explains the doc0 (390us) vs doc3 (2.03ms)
gap as scaling with document size.

**The scoped lever**: re-key the surviving suffix entries in place
during the existing retain walk (`pos += delta`), so the bridge
memo-hits them instead of re-parsing. The 2026-11-12 falsification
killed the SNAPSHOT tier's REMAP VECTOR (a separate 4 MB structure
with random access at 10^6 entries); an in-place re-key inside the
retain walk adds no structure — it is the ledger's "dense suffix
memo re-test", gated on the corpus identity plus the doc0/doc3
ladder. The design care: the dense cache's key layout (pos, atom)
determines re-key cost, and the shift must stay within the cache's
window discipline.

## The suffix re-key: FALSIFIED, and the scaling term re-diagnosed (2026-11-27, final)

The scoped lever above took its A/B and failed to pay. The
`retain_shifted` re-key (keep the pure suffix, `pos += delta` in
place, drop only the spanning entries) ran the battery green (38/38
binaries) with identity 0 failures — but the doc0 p50 was UNCHANGED
(353.8µs vs the 350–357µs band), the reuse count IDENTICAL
(382,776 before and after), and doc3 moved only ~5% (1.93 vs
2.03 ms) with p95 slightly worse. REVERTED per the negative-A/B law.

**The misdiagnosis, corrected**: the splice never re-parses the
suffix — it CLONES the suffix items (`copy_shift_tree`, the
shifted-copy graft). The suffix cache entries are structurally
unconsulted, so re-keying them cannot pay. The 5.1× offset-scaling
term is therefore the GRAFT'S SHIFTED-COPY CLONE — the receipt's
lever #2, now confirmed as THE scaling lever by this falsification:
the clone is O(suffix size) because every leaf's embedded span
(offset/length) must shift with the edit.

**The scoped design** (lever #2, the next build): make the suffix
items' spans shift lazily — either a span indirection (items
reference the retained arena and carry a base offset applied at
read time) or a per-session shift epoch — so the graft assembles
the new :repetition array from re-referenced handles (they are
16-byte pool handles into the persistent arena) instead of
deep-restoring every node. The cross-arena handle tension named in
the profile receipt is exactly this indirection's cost. Gated on
the corpus identity, the doc0/doc3 ladder, and the byte-identical
differential as always.

## The lazy-span-shift graft SHIPPED (2026-11-27, final): lever #2 lands

The scoped design above is built: `AstArena::shift_input_refs`
shifts every stored InputRef at/after the edit start IN PLACE (one
sequential pass over the flat array/hash pools — the leaves are
inline values, no line/column stored, so the bulk shift is exactly
equivalent to the deep `copy_shift_tree` walk), and the graft then
RE-REFERENCES the suffix items' 16-byte pool handles instead of
deep-restoring every node. `copy_shift_tree` and `shift_if_above`
are deleted — the pool IS the shift. Multi-region edits keep the
old deep per-tree shift (sound fallback).

The ordering discipline: the shift runs after the cache invalidate
and BEFORE the plan/parse — the plan's navigation is index-based
(offset-free), the bridge/tails parse fresh nodes in the new
coordinates, and the suffix handles see the shifted data. Dead pool
nodes (superseded trees) shift harmlessly — nothing references them.

## The edit-sequence receipt (the doc0/doc3 traces, identity-checked)

| trace | incr p50 before | incr p50 after | the gain |
|---|---|---|---|
| doc0 (13KB, 200 edits) | 390 us (the corrected baseline) | 260–264 us | **−32%** |
| doc3 (152KB, 200 edits) | 2.03 ms | 384–404 us | **5.1×** |

Identity 0 failures on every round; splice 200/200 (doc0) and
199/200 (doc3); the deep-equality protocol (every incremental tree
vs the fresh parse) held throughout. The doc3 `reused` count jumped
283k -> 437k — the shifted suffix entries now serve the parse's
memo as well (a compounding bonus).

**The measured tail cost**: doc0's p95 moved 1.86 -> 2.11–2.19 ms —
the shift pass is O(POOL SIZE) and the pool accumulates dead nodes
across the session (the budget reset is the current bound). The
known follow-up: dead-node reclamation / pool defragmentation, a
bounded follow-up with the same identity gates. The p95 on doc3 is
unchanged (~21 ms; the fresh-parse leg dominates there).

Tree-sitter's comparator stands at 14.3 us p50 on doc0 — the gap
closes from 27x to 54x->18x... precisely: 264 us / 14.3 us = 18.5x
(from 27x). The next lever after pool reclamation: the bridge
parse's memo-warm interior (the suffix entries now serve it, which
this change enables).

## The pool dead-node reclamation SHIPPED (2026-11-27): the shift's O(session) tail bounded

The receipt's named follow-up, built and A/B-gated. The pool
diagnostic (PHASE instrumentation extended with pool lens + shift
duration) measured the accumulation first: the doc0 session's pool
grows 9,205 entries (the initial tree) -> **405,383 by edit 200**
(~2,000 fresh entries stored per edit, nothing freed), and the
shift pass — O(pool) — costs **310 us at the tail** (0.77 ns/entry)
vs 9 us at edit 2. The shift is not the whole p95 (the high-offset
parse calls dominate it) but it is the session-scaling term: every
pool pass (shift, later passes) walks garbage linear in the edit
count.

### The design

`AstArena::compact(&mut roots) -> Option<PoolRemap>`: a mark-walk
from the live handle holders (the retained cache entries' trees +
the previous parse tree — the splice's graft source), an
order-preserving positional compaction of both pools, and a
two-pass handle rewrite (kept nodes, then roots; the maps return
for the cache's flattened entries). Any inconsistent handle aborts
without mutating — sound fallback. String interning untouched.

**The bit split the first implementation got wrong** (caught by the
doc0 trace's identity check, edits 39-41, then an out-of-bounds
panic at edit 42): the retained family's post-parse retention marks
survivors SNAPSHOT_BIT with an IDENTITY adopt — the node indices
STAY IN THE OUTPUT ARENA (the "identity adopt just marks them
snapshot" comment). Treating SNAPSHOT_BIT as "data lives in the
snapshot arena" skipped exactly the live handle holders: compaction
dropped their trees. The fix splits the semantics:
`RETAINED_PERSISTENT_BIT` (bit 29) — exempt from generation checks
LIKE a snapshot, but an OUTPUT-arena handle holder that compaction
must mark and remap. `is_arena_persistent()` (either bit) covers
the generation-check/recycle exemptions; `is_retained_persistent()`
covers compaction. Two regression specs pin it: the compaction
primitive (dead dropped, roots rewritten, content-identical) and
the session (offset-0 edits re-derive the full tree every edit —
tree-sized dead per edit, the doc0 accumulation shape — identity
held on all 60, pool reclaimed >= 3/4 of the no-reclaim bound).

### The self-tuning trigger (the doc3 falsification)

The uniform "compact at 2x live" trigger **REGRESSED doc3
catastrophically**: same-machine A/B p50 485 us -> **20.9 ms** —
one doc3 edit re-derives a tree-sized item (~90k fresh nodes/edit),
so the pool instantly re-doubles and the O(live) copy-compaction
fires every edit. The falsification is the design: the trigger is
now payback-gated — fire at the 2x-live doubling only when the dead
entries' shift cost on ONE future edit (1 ns/entry, conservative
vs the measured 0.77) repays the compaction cost (the measured cost
of the last fire; ~4 ns x live before the first fire). doc3 never
reaches payback and keeps the 64 MiB budget reset as its bound (its
shift tail is unchanged — a separately-bounded lever); small
sessions converge to a tight pool within a few edits. Loaded
machines self-defer (the measured cost inflates, the gate
tightens).

### The same-machine A/B (load 34-54, 99 users — quiet re-bake pending)

| trace | leg | incr p50 | incr p95 | incr max | identity |
|---|---|---|---|---|---|
| doc0 | before (graft only) | 316.8 us | 2.96 ms | 7.93 ms | 0 |
| doc0 | after (reclamation) | 310.8–319.0 us | 1.99–2.59 ms | 2.14–5.45 ms | 0 |
| doc3 | before | 485.2 us | 36.3 ms | 95.3 ms | 0 |
| doc3 | after (tuned) | 563.4 us | (load-dominated) | 84.4 ms | 0 |

doc0's session-scaling tail collapses (max −31 to −73%); the p50
holds (±2%). doc3 returns to its pre-reclamation p50 once the
payback gate declines (the uniform trigger's 20.9 ms is the
recorded falsification). reused/invalidated identical on doc0
(382,776/36,188 — the memo path untouched); splice 200/200 and
199-200/200. The absolute numbers are a LOADED-machine session —
the quiet-machine re-bake is the receipt-grade bake, as with P3(a).

The compact-cost accounting is visible per fire under
`PARG_SPLICE_DIAG=1` (`COMPACT before=(a+h) after=(a+h) roots=N
cost=Ns`); the pool/shift diagnostic rides `PARG_SPLICE_PHASES=1`.

### The count-based backstop (the CI-debug deferral, caught by the regression spec)

The first CI run of #719 failed ONE spec — the session reclamation
spec — on every Test job: in DEBUG builds the payback gate's
time-based measurement inflates ~10-30x, so compaction defers
legitimately and the pool sat at 93% of the no-reclaim bound. The
spec caught exactly the gap it was written to catch (an
environment-dependent trigger). The fix adds the count-based half of
the trigger: fire unconditionally when the pool exceeds **16x live**
(the dead term's per-edit shift cost is then >= 15x the copy's ~1x
live work — payback within one edit at ANY build mode). The payback
term stays the tight release-mode binder (doc0 unchanged); the
backstop makes reclamation deterministic everywhere and keeps doc3's
memo warmer than the wholesale resets it replaces (doc3 reused
415,541 vs main's 436,915, p50 at the receipt band 409 us).
Debug-mode and release-mode suites both green; the spec's bound is
now 20x live (the deterministic crest).

## The memo-warm interior SHIPPED (2026-11-27, final): the parse-remainder lever lands

The receipt's named next lever, taken through the discipline's
instrument-first gate. The parse-remainder decomposition (new
PARG_SPLICE_PHASES sub-phase timers: v1-suffix / deep / bridge /
tails) measured doc0's remainder first:

| sub-phase (doc0, 200 edits, loaded machine) | profile |
|---|---|
| v1-suffix replay | 0 everywhere (json's root is not a repetition — v1 never engages, the rs#542 decode) |
| **deep derivation probe** | nonzero on **163/200 edits**, mean-dominated |
| **level-0/fallback bridge** | nonzero on 37/200 edits, **tail-dominated (max 271 ms = incr_max)** |
| sibling tails | ~1.5 us |

The mechanism fell out of the code: the shift retention's
`shiftable` predicate kept only `Nil | InputRef` entries above the
edit — a restriction written BEFORE the span-shift existed
("pool-backed entries describe trees whose inner offsets cannot move
coherently"). Post-#718 that justification is stale: the bulk shift
moves every stored InputRef in the arena, and a cache entry's
Array/Hash payload is a position-independent pool handle. The
above-edit container entries were being dropped and re-parsed whole —
the deep probe's mean and the bridge's tail both.

**The lever**: relax `shiftable` to the position predicate alone.
The re-key loop still shifts only InputRef data_a (Array/Hash/StringRef
payloads are position-independent); straddling entries still drop;
multi-region edits keep the plain retain. Sound by the same argument
as the span-shift itself, and gated by everything the retained family
already gates with.

## The measurement (loaded machine — the quiet A/B re-bake is pending)

| trace | metric | before the lever | after |
|---|---|---|---|
| doc0 | incr p95 | 56.2 ms | **618.8 us (~91x)** |
| doc0 | incr max | 271 ms | **2.27 ms (~119x)** |
| doc0 | reused | 382,776 | **1,294,500 (3.4x)** |
| doc3 | reused | 415,541 | **11,051,417 (26.6x)** |
| both | identity / splice | 0 failures, 200/200 & 200/200 | unchanged |

Load 232-244 during the after-leg (the absolute p50s are noise); a
91-119x tail collapse cannot be load bias — the tail WAS the
above-edit re-parse. The suite: 620 lib + all integration tests green
in debug (the incremental_retained gates included), both traces
identity-clean. The quiet-machine receipt bake + the tree-sitter
comparator re-run remain the pending formality; the tree-sitter gap
closes from 18.5x toward the comparator's own per-edit cost.
