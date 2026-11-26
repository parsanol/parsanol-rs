# The splice fixed-overhead profile (2026-11-26): where the 446us goes

The falsification receipt's closing demand ("the next tranche is a
profile of the 446us: the constant-factor diet before any further
granularity work") answered: the per-edit fixed overhead is measured
phase-by-phase with the new `PARG_SPLICE_PHASES` instrument (three
env-gated timers in the retained path — invalidation, the derivation
probe + region re-parse, and the graft; shipped with this receipt,
behavior-neutral).

The doc0 protocol (200 keystrokes, identity-checked, splice engaged
200/200, 0 identity failures):

| phase | median | share of the ~418us total |
|---|---|---|
| cache invalidation (the retain + dirty walk) | 18 us | ~4% |
| derivation probe + region re-parse (the spine plan, the bounds chain, the bridge parse, the sibling tails) | 192 us | ~46% |
| graft (the into-cache handoff, the spine walk, the arena stores of the rebuilt path) | 115 us | ~27% |
| uninstrumented (the per-edit parser construction + the session setup) | ~90 us | ~20% |

The means are dominated by a handful of heavy edits (invalidate mean
646us, probe mean 2124us against those medians; the run's p95 5.4 ms)
— the medians are the honest steady-state per-keystroke numbers.

## The diet levers, in the order the numbers dictate

1. **Instrument the unprofiled ~20% first** — the per-edit parser
   construction (the IncrementalParser rebuilds a PortableParser every
   keystroke) and the cache handoff are a fifth of the budget and the
   only unmeasured phase. A persistent-parser reuse across edits (the
   arena discipline already exists for the retained tier) is the
   candidate, A/B-gated per the negative-result law.
2. **The probe's bridge parse** — the straddling item's re-derivation
   already rides the dense cache for its unchanged interior; the
   remaining cost is the item's own body dispatch. The member
   granularity was falsified (the leaf experiment); the lever here is
   making the probe parse cheaper (the plan9-routed bodies dispatch
   through the DFA lane where routed).
3. **The graft's shifted-copy clone** — copy_shift_tree re-stores the
   k unchanged items through the session arena on every splice. The
   retained arena could re-reference instead of re-store (the
   cross-arena handle tension: the new array's pool must live where
   the items live — the snapshots_in_live discipline). This is the
   largest single phase after the probe.

The tree-sitter comparison is unchanged (14.3us p50): the remaining
20-27x gap decomposes into the phases above, and the identity
guarantee (every edit's tree deep-equals the fresh parse) is the
engine's retained advantage over that comparator.
