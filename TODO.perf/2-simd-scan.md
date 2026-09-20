# 2. SIMD token-class scanning — SHIPPED

`portable/scan.rs`: each character set decomposes into at most eight
closed ranges (a stray single byte is a degenerate range); long runs
scan 16-byte blocks with a baseline-vector membership kernel — every
range contributes a 16-lane compare OR-ed into one mask (NEON on
aarch64, SSE2 on x86_64, scalar elsewhere), the strspn technique.
Only the block that ends a run is searched byte-wise; short runs keep
the scalar path. The VM's Span and the walker's bulk repetition route
through one engine that preserves each side's stepping semantics
(lead-byte for Span, byte-wise for the walker).

LLVM would not auto-vectorize the early-exit search shape (a
memory-reading predicate), which is why the kernels are explicit.

Measured: ~1.5x on identifier-heavy run scanning, and stable under
heavy machine load where the scalar loop's timing collapsed.

History: the original design sketched a Lemire-style class table;
the range-decomposition plus explicit block kernel achieves the same
goal without a 256-entry table walk per byte.
