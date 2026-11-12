# Fuzzing

parsanol carries evidence of surviving *long* fuzzing, not just the
per-push property suites. Two layers:

## Per-push (CI, minutes)

Every push runs the differential and property gates: exhaustive short
inputs over a 9-symbol alphabet (depth 5), seeded long inputs (3,000
cases), the scalar-vs-reference scan agreement, the incremental
reparse equality, and the corpus/conformance replays. These are the
same gates the heavy job scales up.

## Scheduled heavy fuzz (weekly, hours)

The [`fuzz-heavy` workflow](../.github/workflows/fuzz-heavy.yml)
(Saturdays 04:00 UTC, manual dispatch anytime) runs on a reduced
sandbox (`ulimit -c 0`, 8 GiB address space — a VM loop once hit
24 GB) with the grammar corpus checked out as the seed pool:

1. **Mutation fuzzing** — `tests/fuzz_heavy.rs` mutates the seed
   grammars (byte flips, truncations, splices, bracket/operator soup,
   rule splices) and drives the public surface: parse, compile,
   checksum-verified load, and the embedded-test replay. The invariant
   is absolute: **no panics — every rejection is a structured error
   naming its reason.** Budget: 60 minutes
   (`PARSANOL_FUZZ_SECONDS=3600`), roughly 100k+ cases per run.
2. **Scaled property gates** — the exhaustive frontier at depth 7
   (`PARSANOL_FUZZ_DEPTH=7`) and 500,000 seeded long inputs
   (`PARSANOL_FUZZ_SEEDS`).
3. **Property + differential gates** — the full release suite under
   the same environment.

## Crash handling

**New crashes are always real bugs first, flakes last.** Every
"flaky" failure in this repository's history eventually turned out to
be a real edge — the boundary-retention soundness bug was found by
exactly this class of gate.

On any failure the workflow uploads `fuzz-crashes/` and
`target/proptest_persist/` as artifacts named for the run. One
command reproduces a persisted case:

```bash
PARSANOL_FUZZ_REPLAY=fuzz-crashes/case-N.parg \
  cargo test -p parsanol --test fuzz_heavy
```

## The honest counter

```json
"state": {"hours": 0, "crashes": 0}
```

Cumulative heavy-fuzz hours with **0 crashes** since the counter
began. The scheduled workflow PRs a +1 bump on every green run and
**resets the counter to 0 on any crash** — the number is only worth
carrying if it can fall.
