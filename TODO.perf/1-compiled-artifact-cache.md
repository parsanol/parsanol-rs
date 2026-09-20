# 1. Compiled-program artifact cache (P1) — SHIPPED

Implements TODO.max-perf/8. `Program::to_artifact`/`from_artifact`
(the type owns its versioned wire format; CharSet bitmaps pack to 32
bytes, instructions ride serde) plus `bytecode::artifact_cache`
(storage policy: XDG cache dir, FNV-1a grammar-JSON key stable across
processes, embedded-key integrity check, tmp+rename atomic store,
corrupt artifacts are misses, never parse failures). The Ruby FFI
registration path loads before compiling and stores after.

Gates met: lossless round-trip spec, corrupt-input rejection, store/
load + miss specs, full 419-test suite green, wired into
`register_grammar`. Follow-up: subroutine-level cross-grammar
substructure dedupe (XGrammar-2's actual granularity) once more than
one giant grammar shares rule bodies in the wild.
