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
