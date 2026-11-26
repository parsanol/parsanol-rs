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
