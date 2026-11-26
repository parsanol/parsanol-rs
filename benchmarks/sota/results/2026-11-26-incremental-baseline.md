# The incremental baseline receipt (2026-11-26): the gap quantified

The edit-trace benchmark (200 keystrokes per document, the
identity-checked protocol -- every incremental tree deep-equals the
fresh parse, 0 failures on both engines):

| doc | parsanol incr p50 | parsanol fresh p50 | ts incr p50 | ts fresh p50 |
|---|---|---|---|---|
| doc0 (13KB) | 14.3 ms | 18.1 ms | 14.3 us | 427 us |
| doc3 (152KB) | 572.7 ms | 668.2 ms | 100.5 us | 5.4 ms |

**The shipped retained-splice model is net-negative: the incremental
path costs as much as or more than the fresh parse** (the suffix
re-parse from the cutoff plus the arena rebuilds), while tree-sitter's
incremental wins 30-54x over its own fresh parse. The identity
guarantee -- every edit's tree deep-equals the fresh parse -- holds
everywhere and is the engine's unique strength (tree-sitter's
protocol carries no such proof).

**This baseline is the concrete-tree model's charter** (the plan's
program 2): the splice must re-parse only the affected region, the
retained prefix's tree must be reused without an arena rebuild, and
the target is tree-sitter's 30x+ incremental win with the identity
proof retained. The runner's incremental mode (the missing dispatch)
is fixed here; the repro:

```
run_parsanol <artifact> incremental corpora/traces/doc0-trace.json corpora/json/doc0.json
run_ts incremental corpora/traces/doc0-trace.json corpora/json/doc0.json
```


## The splice diagnosis (the same day): engagement 0/200

Instrumenting the runner (`SPLICE spliced 0 total 200`): the
concrete-tree splice -- fully built, the deep spine surgery included
-- engaged ZERO of the 200 doc0 edits. The cause: the string-scan
shape (option A) makes every quoted string ONE atomic span, so a
keystroke inside a string literal has no spine for the splice to
walk -- the derivation probe refuses and every edit falls back to
the full parse (the 14.3 ms p50 = the fresh parse's cost exactly).

**The model work is therefore precise:** a string-aware splice. The
spine terminates at the owning string-span rule; the span's new
value comes from the string kernel's re-scan (O(string length), not
O(document)); the surgery above the spine is the existing deep
mechanism. This is the first build item of program 2 and the only
one between the engine and tree-sitter's 30x incremental win.
