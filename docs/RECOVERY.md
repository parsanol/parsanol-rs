# Recovery quality: a seeded, published, engine-identical metric

Declarative resync recovery is measurable. Every engine that executes
a PARG artifact replays the SAME corruption stream and must produce
the SAME recoverable fraction — the metric is a standing
engine-parity gate, not a one-off benchmark (rs#304).

## The metric

For an artifact's clean input and a seeded corruption model —
per-byte delete / substitute / insert / duplicate, count
`1 + (seed % 3)`, drawn from a fixed u64 LCG stream — over 64 seeds:

- **recoverable fraction**: the parse either succeeds or the resync
  phase consumed at least one annotated junk span and parsing resumed;
- **precision of recovery**: recovered trees' non-error nodes that
  also appear in the uncorrupted document's node set;
- **bytes-to-recovery**: the consumed position at the first resync,
  relative to the deepest failure offset (the span may start before
  it).

The corruption stream, the metric, and the pins live in two
harnesses that must move together:

| engine | harness |
|---|---|
| Rust | `tests/recovery_metrics.rs` |
| Ruby | `spec/parsanol/parg/recovery_metrics_spec.rb` |

Any count change in one without the twin is engine drift and fails
the gate by its own contract.

## Published numbers (converged)

Both engines anchor the resync scan at the spine boundary and run it
whenever the parse fails (rs#304 slice 3, the convergence train):

| artifact | recoverable | precision |
|---|---|---|
| recovery_resync_declared | 58/64 (91%) | 1.00 |
| recovery_kv (inferred) | 33/64 (52%) | 1.00 |

## The comparative columns (rs#304)

The same seeded corruption stream, the same kv language, measured per
tool. parslet and pest have no recovery: the parse aborts at the first
violation — 0/64 by construction, the property that motivated parsanol.
tree-sitter's always-on GLR recovery is measured with its own parser
(`tools/tree_sitter_recovery_column.py`, the generated kv grammar,
`tree-sitter parse`):

| tool | recovery policy | recoverable | precision |
|---|---|---|---|
| **parsanol, declared resync** | author-owned spans, byte-identical across engines | 58/64 (91%) | **1.00** |
| parsanol, inferred | trial scan | 33/64 (52%) | 1.00 |
| tree-sitter 0.27 | always-on GLR resync | **62/64 (97%)** | 0.66 |
| parslet | none — abort | 0/64 | — |
| pest | none — abort | 0/64 | — |

The honest reading: tree-sitter recovers slightly more often and
mangles a third of the recovered trees (its ERROR nodes swallow or
reshape the neighboring pairs — precision 0.66). parsanol's declared
policy recovers 91% with exact fidelity: the recovered trees contain
only clean nodes, the junk spans are the author's declared pattern
consumed deterministically, and the same numbers reproduce on every
engine — which is also what makes the recovery language exportable to
constrained decoders (ruby#321's escape grammars).

Reproduce:

```bash
# Rust
cargo test -p parsanol --test recovery_metrics -- --nocapture

# Ruby (conformance corpus checked out beside this repository)
bundle exec rspec spec/parsanol/parg/recovery_metrics_spec.rb
```

The declared/inferred split is the grammar author's control surface:
a `resync <rule> = <pattern>` declaration raises the recoverable
fraction because the junk span is deterministic (no trial scan) and
always resumable.

## Placement

This table belongs beside the honest performance table (ruby#298,
quiet-host benchmark); it stands alone until that lands so the
recovery lead is already quantified and defensible.
