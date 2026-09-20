# 5. VM host-callable Dynamic contexts — SHIPPED

Dynamic grammars run on the bytecode VM: compile_dynamic emits
InvokeDynamic (it used to refuse, routing the whole grammar to the
walker). At runtime the instruction delegates to the packrat engine
for the resolved fragment — under the shared recursion/budget guards
(GH-76) — with capture state seeded both ways and the fragment's
value adopted into the VM arena.

Memoization soundness: dynamic outcomes depend on capture state,
which memo keys ignore. Programs containing InvokeDynamic disable
rule-call memoization; the walker skips memo for dynamic-dependent
atoms (reverse-reachability analysis).

Capture rollback (the coradoc GH-76 follow-up, 69 native-only
regressions): the walker stored captures with no rollback, so a
FAILED alternative branch's captures leaked into later branches and
re-keyed dynamic dispatch. Alternatives, sequences, repetitions and
lookaheads now scope their capture regions — failure pops, success
commits.
