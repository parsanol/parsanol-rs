# 6. Wide scan kernels — 32-byte blocks

Item 2 shipped 16-byte baseline kernels (NEON/SSE2). This item widens
the block where the target allows, with runtime detection on x86:

- x86_64: AVX2 + BMI2 kernel behind `is_x86_feature_detected!` —
  32-byte loads, per-range unsigned compares OR-ed into one mask,
  `movemask` + `tzcnt` locates the first non-member in the ending
  block. SSE2 stays the fallback.
- aarch64: NEON has no runtime variance — the kernel unrolls to two
  16-byte compares per block instead (q-register pairs), halving the
  per-block loop overhead for runs >= 32 bytes.

Verification is deterministic: emitted-asm inspection (vector loads
per block) plus interleaved A/B against the 16-byte kernels, since
this machine's load makes absolute wall-clock numbers meaningless.
Short runs (< one block) keep the scalar tail path unchanged.
