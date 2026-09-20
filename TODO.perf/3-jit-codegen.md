# 3. JIT codegen via cranelift (P2)

Supersedes the JIT half of TODO.max-perf/5. Compile each opcode to a
cranelift IR block once per grammar; cross-grammar fragment caching
keyed on compiled-hash (XGrammar-2). Builds on the artifact cache
(TODO.perf/1): the native artifact is the natural cache unit for
compiled code too. Gate: differential parity, perf on SRL corpus,
memory freed with the grammar handle. Status: backlog — current
bottleneck is packrat backtracking, not dispatch; revisit after
TODO.perf/2 and a fresh EXPRESS profile.
