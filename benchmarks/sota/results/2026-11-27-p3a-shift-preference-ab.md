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
