# 4. Incremental reparsing by edit span (P2)

Supersedes TODO.max-perf/7. Design: persist the previous parse's memo
index keyed by content hash; on edit at [start, delta), invalidate
only memo entries at/after the edit plus rule spans crossing it;
expose `parse_incremental(handle, input, prev)` through the FFI. The
VM's explicit state (instructions, registers, memo) makes this
natural; the walker needs its DenseCache invalidated by span. Gate:
incremental == full re-parse on SRL corpus with simulated edit
sequences; keystroke re-parses < 5 ms on 600 KB documents. Status:
backlog — blocked on a consumer that needs it (editor integrations).
