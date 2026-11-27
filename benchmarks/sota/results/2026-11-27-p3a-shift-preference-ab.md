# The P3(a) A/B receipt (2026-11-27): the shift-preference class

The resolved-conflict seam (`build_lr_table_shift_preference`, wired
behind `PARG_LR_SHIFT=1` since the LR Phase 2, "the default stays
strict until the receipt lands") takes its A/B.

## The class

499 of the 1448 LR-refused fleet rules build under shift-preference
(34%; the dominant refusal class, "ordered choice with overlapping
FIRST sets", 603 rules). Per-flavor: ieee 121, bsi 73, nist 48,
iso 34, iec 21, itu 19. The compile path consults the builder
directly, so the gate routes them today — no provability change
needed.

## The A/B (the ieee flavor, 81 corpus accept-cases)

The harness (`examples/ab_shift.rs`): one canonical identity pass
(the shape JSON of every parse, hashed in order) then min-of-200
whole-corpus rounds, 5 interleaved off/on process pairs:

| arm | min of the minima | identity checksum |
|---|---|---|
| off (strict) | 8.68 ms | `2c944ef744878c88` |
| on (shift-preference) | 8.48 ms (**−2.3%**) | `2c944ef744878c88` |

**The byte-identical differential holds on every run** — the same
tree checksum across all 10 process pairs. The best single pair ran
−12.2%; the host is noisy (both arms spread 8.5–9.7 ms), so the
receipt-grade bake goes to the quiet machine with the same harness.
The class does not regress, and the correctness side is green
everywhere: the lib fleet 620/620, the full `--tests` battery, and
the conformance corpus, all with the gate ON.

## The census consequence

The loop-site census barely moves (481 dfa-routed + 4 lr-covered +
1 lr-covered-under-shift + 38 refused — the shift-buildable rules
are mostly not loop sites), but the LR RULE coverage grows
1628 -> 2127 under the gate: the 499 rules' shapes ride the
deterministic-subset machinery on every parse that enters them.

## The remaining decision

The default flip (strict -> shift-preference) is the owner's: the
evidence is byte-identical trees, no regression, a measurable win on
the class-heavy flavors; the flip should ride the quiet-machine
bake (wider corpus set: bsi 73, nist 48 rules) and then land as the
default with the census re-run in the same train.

## The DEFAULT FLIP SHIPPED (2026-11-27, owner-authorized): shift-preference IS the construction

The `PARG_LR_SHIFT` gate is gone: `lr_table_for` builds
shift-preference unconditionally (a per-rule refusal still falls
back to the classic lowering; the strict builder remains for the
survey). The `ab_shift` harness is deleted — its off/on premise is
dead; `examples/bake_flavor.rs` carries the flavor A/B forward
(cases-file inputs, identity checksum, min-of-N).

### The census re-run (the current fleet — pubid-grammar fast-forwarded 28 commits, 61 flavors)

| census | before the flip (58 flavors) | after (61 flavors) |
|---|---|---|
| LR rules | 1973 provable / 1583 refused / 499 shift | **2033 provable / 1610 refused / 529 shift-preference** (317 greedy documented-refused) |
| loop sites | 524 = 481 dfa + 4 lr + 1 shift + 38 refused | **1495 = 1414 dfa + 4 lr + 2 lr-covered-under-shift + 75 refused** (the 3 new flavors carry depth-refused sites; the PLAN9 depth class dominates) |

### The flavor A/B with the flip (the win the receipt predicted)

ieee (81 accept-cases, 2318 bytes; interleaved process pairs,
min-of-9, load ~100-116 — relative verdict):

| arm | median | min |
|---|---|---|
| strict (pre-flip main) | 31.4 ms | 30.2 ms |
| **shift-preference (the flip)** | **24.2 ms (−23%)** | **20.6 ms (−32%)** |

Identity: the shape-JSON checksum matches **bit-for-bit** across
both arms on ieee (2c944ef7…) and bsi (39f796bc…) — the
byte-identical differential holds under the flipped default. The
full suite green (620 lib + integration; the one red =
`single_codepoint_terminals_scale_linearly`, the recorded
timing-sensitive test, passes in isolation — host noise at load
100+, the same signature the ledger pinned pre-flip); the
conformance corpus green; `P3(b)`'s greedy-direction stays
out (the falsification receipt stands).
