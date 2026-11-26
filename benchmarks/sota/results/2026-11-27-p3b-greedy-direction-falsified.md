# The P3(b) falsification receipt (2026-11-27): the greedy-direction class is not routable

The plan's program 3 ordered the resolved-conflict classes:
shift-preference (P3(a)) → greedy-direction (P3(b)) → longest-reduce
(P3(c)), each gated on the byte-identical proof per class. P3(b) takes
its proof and FAILS it — acceptance, not just trees.

## The class

The greedy-direction resolution: a SHIFT arriving at a byte where a
REDUCE was already taken — the same maximal-munch continuation law as
P3(a), applied at the deeper conflict. The machinery ships behind
`Strictness::Resolved` (`build_lr_table`; the shift closure's
`LrAction::Reduce(_) if strict == Strictness::Resolved` arm).

## The sizing

On the 58-flavor fleet (1973 provable, 1583 refused): **517 rules
build under shift-preference and another 318 under the
greedy-direction resolution** — a tempting class.

## The falsification

The compiler seam took a cumulative gate ladder
(strict → shift → greedy) and the full test battery ran with both
gates ON. **Two tests fail:**

1. `histogram_probe_json` — the resolved table **refuses to parse
   valid json** (`result.is_ok()` fails): the shift-over-reduce
   resolution strands the parse at the conflict byte — the reduce's
   continuation is unreachable from the shift's target state.
2. `artifact_json_string_span_differential` — the string-span
   differential diverges the same way.

The class does not merely reorder values; it breaks ACCEPTANCE on the
fleet's json grammar. The shift-preference class survives the same
battery (the P3(a) receipt's 10/10 identity) because its conflicts
are the LOOP's continue-vs-exit — where the PEG is greedy by
definition. The greedy-direction class reaches CHOICE conflicts,
where the PEG's order is NOT a greedy law, and the resolution
silently abandons the ordered alternative's continuation.

## The disposition

- The greedy arm is REVERTED from the compiler seam (the ladder stays
  strict → shift-preference); the comment at `lr_table_for` records
  the falsification.
- The sizing stays in the survey (the analysis surface — the count is
  legitimate data; 318 rules' tables would build but misparse).
- P3(b) is a DOCUMENTED REFUSAL: the class is unsound for PEG
  semantics beyond the loop-continue shape that P3(a) already covers.
- P3(c) (longest-reduce) is likewise a documented refusal: the
  reduce/reduce conflicts refuse at every strictness level (the
  construction's own law: "the ordered-choice class is not
  deterministically reducible") — the longest-production tiebreak
  would reorder PEG alternatives exactly like the greedy-direction
  does.

Program 3's class ladder resolves: P3(a) shipped (the receipt);
P3(b)/P3(c) falsified/documented-refused with the failing repro —
the resolved-conflict frontier is exactly the shift-preference class,
and the census's both-lane refusal reasons stand.
