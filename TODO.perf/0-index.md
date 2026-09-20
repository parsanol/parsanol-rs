# TODO.perf — remaining performance work (SSOT)

Supersedes the backlog sections of TODO.max-perf/{4,5,7,8} (those
files keep their shipped history; their open items live here). MECE
decomposition of everything left on parsanol performance:

| # | Item | Supersedes | Status |
|---|------|------------|--------|
| 1 | Compiled-program artifact cache | TODO.max-perf/8 | SHIPPED (parsanol-rs TODO.perf/1 round) |
| 2 | SIMD token-class scanning | TODO.max-perf/5 (SIMD half) | specced, backlog |
| 3 | JIT codegen (cranelift) | TODO.max-perf/5 (JIT half) | specced, backlog |
| 4 | Incremental reparsing by edit span | TODO.max-perf/7 | specced, backlog |
| 5 | VM host-callable Dynamic contexts | TODO.max-perf/4 remainder | specced, backlog |

Done items stay done: lead-byte dispatch, parser hot path, the
bytecode VM phases 1-4, the handle API, pure-Ruby posture, and
expressir-core (TODO.max-perf/1-4, 6, 9) all shipped.
