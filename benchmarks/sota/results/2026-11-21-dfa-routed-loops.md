# The DFA routed repetition loops: the loop-heavy A/B receipt (rs#574 slice B stages 2+3)

**Date**: 2026-11-21. **Host**: shared Darwin box, load 17 (the session's
quietest window). **Methodology**: the suite's own runner
(`benchmarks/sota/runners/parsanol`, min-of-15 per document), two
interleaved rounds, minima aggregated. **A side** = 8983451 (the 1.5.13
release commit — stage-1 rule routing only, NO loop routing); **B side**
= main at 27bc14b (stage 2 + stage 3 tranches 1-3). Each side baked its
own artifact from the same PARG source; both sides proved full
consumption before timing (the rs#496 law).

## Grammar 1 — the inline-class loop (the pure-scan win case)

    grammar LoopBench2 version "1.0.0" edition "v2" {
      word = 1*(%x61-7A)
      sep = %x20
      doc = word 0*(sep word)
      entry doc: doc
    }

Input: 389,896 bytes of seeded random lower-case words (42,805 words).
The `word` loop routes to one DfaRun (tranche 1: an ASCII-class body,
bare spans per iteration).

| side | TOTAL ns | ns/B |
|---|---|---|
| pre-routing (min) | 15,399,792 | 39.5 |
| routed (min) | 13,285,958 | **34.1** |

**−13.7%** on the loop-heavy grammar (round-2 minima; round 1 showed
−16.0% — 17.45 → 14.66 ms). The routed table walk replaces the
per-character Choice/Char dispatch for the whole byte stream.

## Grammar 2 — the rule-ref loop (the hash-write-bounded case)

    grammar LoopBench version "1.0.0" edition "v2" {
      letter = %x61-7A
      word = 1*letter
      sep = %x20
      doc = word 0*(sep word)
      entry doc: doc
    }

Same input. `1*letter` routes (tranche 3: a rule-ref body whose target
is a bare class — bare spans per iteration); the outer `0*(sep word)`
loop stays classic (a sequence of rule refs — a later tranche).

| side | TOTAL ns | ns/B |
|---|---|---|
| pre-routing (min) | 72,180,166 | 185.2 |
| routed (min) | 70,263,875 | **180.4** |

**−2.7%**: the per-character `{letter: span}` hash writes (the v2
named-tree law) dominate this grammar's cost, so the dispatch saving is
bounded. The honest reading: the routing's win scales with the ratio of
scanning to tree-building in the loop.

## Reading

- The routed loops deliver a measured double-digit win where the loop
  body is scan-dominated (−13.7-16.0%), and a small honest win where
  per-iteration value construction dominates (−2.7%).
- The json corpus stays honest-zero (its loops bulk-lower via the
  slice-4 Span path) — this receipt is the loop-heavy complement.
- The remaining loop shapes (sequences of rule refs, nested
  multi-value fragments) stay classic; their tranches would extend the
  win to grammars whose OUTER loops route too.
