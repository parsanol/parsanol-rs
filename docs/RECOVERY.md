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
