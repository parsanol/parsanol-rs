# TODO.perf — remaining performance work (SSOT)

Supersedes the backlog sections of TODO.max-perf/{4,5,7,8} (those
files keep their shipped history; their open items live here). MECE
decomposition of everything left on parsanol performance:

| # | Item | Supersedes | Status |
|---|------|------------|--------|
| 1 | Compiled-program artifact cache | TODO.max-perf/8 | SHIPPED (0.7.5) |
| 2 | SIMD token-class scanning | TODO.max-perf/5 (SIMD half) | SHIPPED (this round) |
| 3 | Codegen: shared-prefix split | TODO.max-perf/5 (JIT half) | SHIPPED (this round); cranelift deferred, see item |
| 4 | Incremental reparsing by edit span | TODO.max-perf/7 | SHIPPED (this round) |
| 5 | VM host-callable Dynamic contexts | TODO.max-perf/4 remainder | SHIPPED (this round) |

Done items stay done: lead-byte dispatch, parser hot path, the
bytecode VM phases 1-4, the handle API, pure-Ruby posture, and
expressir-core (TODO.max-perf/1-4, 6, 9) all shipped.

## Follow-ups (specced, unscheduled)

- A portable 32-byte-block scan kernel (AVX-512/BMI2 on x86, SVE on
  aarch64) for item 2's engine; the 16-byte baseline kernels shipped.
- Keystroke re-parse timing on quiet hardware for item 4 (the gate
  held correctness under 30-edit sequences; wall-clock numbers need
  an unloaded machine to mean anything).
- cranelift JIT (item 3's original form) only if a fresh profile
  shows dispatch — not backtracking — dominating a real corpus
  again.
