# 8. Incremental reparse latency benchmark (runnable gate)

Item 4's timing gate ("keystroke re-parse under 5 ms on 600 KB") was
validated for correctness only: wall-clock on the dev machine was
unreliable under load. This item commits a benchmark that makes the
gate runnable anywhere:

`parsanol/examples/incremental_bench.rs` builds a ~600 KB KV-style
document, runs an initial full parse, then simulates 100 keystrokes
(single-line edits spread across the document), reporting per-edit
incremental latency, mean/p95, the full-reparse baseline, and the
speedup — plus a tree-equality assertion per edit so the benchmark
doubles as a gate.
