# Receipt: the memo hasher + pooled tree writes — 2026-11-20 (rs#574 Phase 0)

The instruction histogram's "85 of 253 tree-build instructions" pointed
at the right cost CENTER but not the right COST: the sample profile of
the receipt runner over the json corpus (400 rounds, M1 Max, load ~10)
ranked the actual hot blocks as (1) the VM dispatch itself, (2)
`AstArena::store_tagged_array`, (3) the memo HashMap's sip hash +
un-pre-sized rehash churn (`hash_one`/`insert`/`reserve_rehash`/memmove/
madvise ≈ 14% of top-of-stack samples), (4) the arena's per-node intern
probes for shape tags and BuildHash names.

## The three changes

1. **The memo map** (`vm.rs`): the packrat's `(rule pc, position)`
   memoized on every rule call through a default sip hasher with no
   pre-sizing. Swapped to a multiply-xor hasher (identity is all the
   memo needs) and pre-sized at `2 × input.len()` clamped to the store
   cap — the raw table no longer grows through rehash cycles per parse.
2. **Pooled tree writes** (`arena.rs` + `vm.rs`): `store_tagged_array`
   interned its `:sequence`/`:repetition`/`:maybe` tag per node and
   `BuildHash` re-interned its per-instruction name per capture — both
   are per-program constants, now cached as interned pool indexes in the
   VM (`store_tagged_array_indexed`, `store_hash_indexed_key`).
   `BuildSeq`/`BuildRep` no longer `split_off` a fresh Vec per node —
   they store straight from the value-stack tail and truncate.
3. **The arena pre-size** (`arena.rs`): `for_input`'s node estimate
   (input/50, clamped 20 000) regrew the pools several times per parse
   on any real corpus; sized at input/4, clamped 4M.

Trees are identical by construction: same pool contents, same node
shapes, same intern dedup — only the mechanics of writing them changed.

## Interleaved A/B (min-of-15 per side per round, 6 rounds, same-worktree binaries)

parsanol VM, json corpus, 447 801 bytes, per-parse totals:

| round | baseline (HEAD) | memo + pooled writes |
|---|---|---|
| 1 | 28 919 µs | 18 849 µs |
| 2 | 29 265 | 19 179 |
| 3 | 28 802 | 19 185 |
| 4 | 29 308 | 18 723 |
| 5 | 29 692 | 19 053 |
| 6 | 29 162 | 18 751 |
| **min** | **28 802** | **18 723** |

**−35.0%** on the min; every round wins by ~10 ms. The absolute moves
0.0599 → **0.0418 µs/B** — the gap to tree-sitter's 0.031 narrows from
1.93× to **1.35×**, ahead of Lark LALR and ANTLR by 8×+.

## Gates

Byte-identity held: the full engine suite under the RSS watchdog, the
corpus identity lanes, and the receipt runner's own full-consumption /
clean-tree verification over every document, every round.

## What remains to 0.03

The dispatch loop itself (32% of samples) and the per-char scan kernels
are the next profile-ranked blocks; the Phase 2 LR routing behind the
#585 prover stays the structural endgame.
