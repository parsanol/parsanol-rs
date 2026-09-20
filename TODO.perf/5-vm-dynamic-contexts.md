# 5. VM host-callable Dynamic contexts (P2)

The TODO.max-perf/4 remainder: the bytecode VM declines Dynamic
grammars entirely (they parse on the walker). Design: a CALL_HOST
opcode that suspends the VM, invokes the registered DynamicCallback
(resolve_fragment), splices the returned fragment grammar's
instructions, and resumes — capture state marshaled both ways.
Gate: differential vs the walker on dynamic-heavy grammars, budget
unchanged. Status: backlog — the walker handles dynamic grammars
correctly today; this is a throughput, not correctness, item.
