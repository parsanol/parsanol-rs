# 7. Dense rule-call memo for the bytecode VM

The VM's rule-call memoization is a `HashMap<(usize, usize), MemoEntry>`
keyed by (rule pc, position): a tuple hash plus hashbrown probing per
memoized call. The tree-walker's DenseCache — open addressing over a
preallocated slotted array with a tiny FNV mix — is the shape that
lets the walker still beat the VM on giant inputs (EXPRESS: 0.9s vs
1.3s, which is why the backtrack budget routes giants to the walker).

This item replaces the HashMap with a dense linear-probing table of
the same design: slots preallocated from a size heuristic
(input_len + rule count), entries {rule_pc, pos, success, end_pos,
value: Option<AstNode>} stored inline, hit path with no allocation
and no tuple hashing. Disabled-memo programs (InvokeDynamic) skip the
table exactly as they skip the map today.

Gate: benches/large-input.rs A/B (VM path), full differential suite,
tree parity on the KV/EXPRESS-shaped grammars.
