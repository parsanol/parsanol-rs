# Per-call artifact stack budget

`PargArtifactJs.parseShapeWithCallDepth(entry, input, compatibility, reporter, maxCallDepth)`
returns the same JSON success, syntax refusal and typed resource envelopes as
`parseShapeWithReporter`. It accepts an integer budget from 1 through 8192;
zero cannot disable the guard. The budget applies only to that parse. Existing
entrypoints retain the WASM default of 1024 simultaneous VM atom calls.

A long right-recursive expression can need more VM atom calls than a deeply
nested rule needs rule references. Consumers may opt into a larger finite call
budget without increasing the rule-recursion or shape-JSON limits. The host
still needs enough stack for the chosen budget; the limit is not a promise that
every engine can supply that stack. Resource aborts remain distinct from syntax
refusals. Rust callers use `PargArtifact::parse_shape_with_call_depth` with
`Some(budget)`; `None` preserves the target default.

The WASM workflow exercises invalid budgets, a low-budget typed abort, an
above-default successful parse, unchanged defaults and parser reuse in Node
and a real browser. No publication is required to test this API.
