# 10. Success-path diagnostics overhead (SHIPPED 2026-11-16)

The rs#533 JSON receipt measured the VM at 1.073 µs/B — 30–100× above
what a release Rust PEG should cost. First suspect, visible in the
code: `BytecodeVM` calls `track_failure_with_context` at EVERY failed
alternative. Ordered choice on valid JSON still fails ~2–6 branches
per value node, so the `ErrorTracker` runs constantly on the SUCCESS
path. The portable parser already has `DiagnosticsPolicy {Legacy,
Pending}` (parse-end materialization); the VM has no policy knob and
tracks eagerly.

## Work

- Add a VM-level diagnostics policy (default unchanged; `Pending` /
  off for the bench and the hot paths).
- Profile doc1 (`sample`/flamegraph) to rank the remaining per-byte
  costs: class decode (`get_or_decode_class`), backtrack-stack churn
  per Choice, landing-slot bookkeeping, capture/arena traffic.
- Bench builds get `lto = "fat"`, `codegen-units = 1`, `debug =
  true` (for symbolicated profiles) in the harness runner profile.

## Gate — MET

Byte-identical: 633 suite tests green, all 60 corrupted-case
diagnostics diff-identical to the pre-patch engine. VM 1.086 → 0.174
µs/B (6.2×), interpreter 0.828 → 0.484 (1.7×). Full receipts:
`benchmarks/sota/results/2026-11-16-vm-hotpath.md` (all three
findings: diagnostics deferral, periodic timeout sampling, Arc'd
regex caches — items 2 and 3 surfaced by the follow-up profiles).
Remaining from this item's list: none — the pool/lint work moved to
item 12; the class-decode/TLS survivors are item 11's targets.
