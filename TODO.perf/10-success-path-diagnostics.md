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
diagnostics diff-identical to the pre-patch engine. VM 1.086 → 0.153
µs/B (7.1×), interpreter 0.828 → 0.464 (1.8×). Full receipts:
`benchmarks/sota/results/2026-11-16-vm-hotpath.md` (all three
findings: diagnostics deferral, periodic timeout sampling, Arc'd
regex caches — items 2 and 3 surfaced by the follow-up profiles).
Remaining from this item's list: none — the pool/lint work moved to
item 12; the class-decode/TLS survivors are item 11's targets.

## Phase 0 negatives (2026-11-19, interleaved A/B, min-of-6) — measured, not guessed

1. **The deps opt-level flip** (`[profile.release.package."*"]`
   "s" → 3): FALSIFIED — the clean A/B at current main shows the "s"
   profile **6% faster** (min 26.79 vs 28.35 ms). The smaller kernels
   win the I-cache; the owner's "s" is deliberate and optimal. (The
   first "-20%" reading = a stale pre-rs#586 baseline artifact.)
2. **The ErrorTracker record-time duplicate scan** (the linear
   `contexts.iter().any` per failed alternative): FALSIFIED as a
   measurable lever — removing it (the wire dedups at render anyway)
   lands at noise level (min 26.99 vs 27.06 ms). The scan is not hot;
   the wire contract stays pinned.

The measured remaining lever stays the TREE-BUILD OPS (85 of 253
program instructions — Phase 0's pooled writes / --no-ast split),
then the Phase 2 LR routing behind rs#585's prover.
