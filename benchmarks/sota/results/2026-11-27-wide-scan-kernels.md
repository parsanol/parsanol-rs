# The wide scan kernels verification receipt (2026-11-27): item 6 closes

The 32-byte-window scan machinery was implemented across the
rs#106-era rounds (the windowed driver, the AVX2 single-load kernel
behind runtime detection, the composed two-block aarch64 form) —
this round supplies the item's missing verification leg and closes
the item.

## The asm inspection (aarch64, --emit=asm)

`block_first_non_member` compiles to pure 16-byte q-register NEON:
`cmhs.16b` pairs (the unsigned range tests via the saturating
max/min idiom), `orr.16b` accumulator, one `uminv.16b` horizontal
reduce per block. The window path composes TWO such block kernels
per 32-byte window — the item's required aarch64 form ("two 16-byte
compares per block, q-register pairs"), kept unfused deliberately:
the fused single-accumulator form miscompiled on this toolchain
(parsanol-rs#106 — windows reported all-member with non-members
present, failing parses at EOF); the composed shape is
property-test-verified against the scalar reference. The x86 side's
AVX2 kernel is the one-load 32-byte form (`_mm256_loadu_si256`,
per-range saturating compares, `_mm256_movemask_epi8`) behind the
cached runtime detection, SSE2 the fallback — per the item.

## The interleaved A/B (the NEON path, this host)

`examples/wide_scan_ab.rs`: the shipped 32-byte-window driver vs a
local 16-byte-chunk reference over a 4 MiB `[a-z]` run, interleaved
min-of-20:

| driver | best (8 MiB buffer, 4 MiB run) | throughput |
|---|---|---|
| wide (32-byte windows) | 105 us | ~25.7 GB/s |
| narrow (16-byte chunks) | 1452 us | ~2.9 GB/s |

**13.8×** over the pre-window shape, stable across runs (0.072
ratio, three rounds). The drivers agree on the run end (asserted in
the harness). The host-noise caveat stands; the delta is far beyond
it.

## The identity gates

The scan module's property tests verify the windowed kernels
against the scalar reference (the #106 discipline); the corpus
gates cover every engine through the real parses (the item's own
text: "short runs keep the scalar tail path unchanged" — the
driver's 16-byte remainder and scalar tail are unchanged).

Item 6 closes: both target forms are in place, inspected in the
emitted asm, and measured against their 16-byte reference.
