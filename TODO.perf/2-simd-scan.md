# 2. SIMD token-class scanning (P2)

Supersedes the SIMD half of TODO.max-perf/5. Design: a `SCAN`
optimizer pass that fuses TestSet/CharSet repetition runs over
disjoint classes (identifier/whitespace runs) into one opcode
striding 16/32-byte blocks via a 256-entry class table (Lemire
simdjson-style); memchr (already a dependency) handles the
single-lead-byte special case. Gate: differential parity unchanged;
measurable win on `benches/large-input.rs` identifier-heavy inputs.
Status: backlog — the VM's ByteDispatch already removed most
per-byte dispatch cost, so this needs a fresh profile to justify.
