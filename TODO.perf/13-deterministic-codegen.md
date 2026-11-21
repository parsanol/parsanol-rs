# 13. Deterministic-subset codegen: the tree-sitter endgame

Extends item 3's codegen line (cranelift deferred) with the strategy
that actually closes the receipt's 37× gap: for the deterministic
subset, compile the grammar to a TABLE automaton (LR-family tables or
a native Rust state machine generated per grammar) with per-state
lexing — the tree-sitter design, in our engine.

- Scope: grammars (or rules) the FIRST/follow analysis proves
  deterministic. Everything else keeps the PEG VM — the artifact
  carries both and routes per rule.
- The generated automaton lives in the shared engine, so rs, ruby
  (native ext), python, ts, and wasm inherit it — one win, five
  runtimes. Cross-runtime byte-identity stays corpus-gated.
- Keep the PEG semantics at the seams: ordered choice, captures, and
  recovery envelopes must compose with the automaton path (envelope
  annotations on automaton rules lower to skip/resume actions).

## Gate

JSON ≤ 0.03 µs/B (matching tree-sitter 0.029, then beating it), the
full corpus fleet byte-identical, and the receipt table showing
parsanol ahead of tree-sitter on a deterministic grammar — with the
typed-AST output tree-sitter doesn't produce.

## Slice 4 (specced 2026-11-19, measured): the mixed-rep bulk scan — json's string body

The compiled bench-json program carries **zero bulk-scan instructions**
(the histogram: 253 instructions, 11 ByteDispatch, 0 ScanClass,
0 ScanUntil). json's dominant byte share — the string interior — is
the per-char Choice loop:

    string_lit = ( %x22 *( escaped / char_unescaped ) %x22 )

The neg-scan fusion (`*(!delim term)` → ScanUntil) rejects the body:
it is an ALTERNATIVE, not a negated-terminal sequence, and it cannot
be rewritten as one (the escape bytes %x5C must be MATCHED mid-run, not
skipped). The win: a mixed-rep lowering for the unbounded arm —

    body = Alternative { A = Seq{ lead ∉ Class, ... }, B = Class }

lowers to

    Choice L_exit / L_body: ScanClass(B) / <A inline> / RepCount /
    PartialCommit L_body / L_exit:

the B-runs bulk (one compressed iteration, the exact register
discipline the charset arm already uses), each A matches inline at its
disjoint lead byte. The tree question — the compressed B-run's value
count vs the interleaved loop's per-char values — is the same problem
the neg-scan solved ("per-iteration values are the body's tagged
[Nil, span] sequence"); the build side needs the same treatment for
the mixed case before the corpus identity gates can pass. Strings are
~60% of package-lock.json's bytes; the per-char Choice over them is
the largest remaining block between 0.0599 and the 0.03 gate.

## Slice 4 CORRECTION (2026-11-19, measured): the premise is falsified

The instruction histogram of the compiled bench-json program shows
**Span: 1** — the string interior is ALREADY bulk-lowered (the
try_get_charset_for_atom path predates this slice). The detector
written for the mixed-rep shape never fires on the bench grammar; the
implementation was reverted and parked (the stash carries it). The
remaining 1.93× is therefore NOT the string body: the histogram's
largest block is the tree-build ops (85 of 253 instructions are
PushNil/BuildSeq/BuildHash/RepOpen-class writes — the typed-AST cost
tree-sitter's flat CST never pays). The next real lever = Phase 0's
pooled tree writes / the --no-ast split (TODO.perf/10), then the
Phase 2 LR routing. The mixed-rep detector stays parked for the
capture-bearing shapes the Span path cannot take.

## Phase 0 SHIPPED (2026-11-20, measured): the memo hasher + the pooled tree writes

The sample profile corrected the histogram's read: the tree-build
INSTRUCTIONS were the right cost center, but the actual cost was in the
WRITE MECHANICS — the packrat memo's default sip hasher + un-pre-sized
rehash churn (~14% of top-of-stack samples), `store_tagged_array`'s
per-node tag intern + `BuildHash`'s per-capture name intern, the
`split_off` Vec per node build, and `for_input`'s 20 000-node clamp
regrowing the pools per parse. All four fixed; trees identical by
construction. Interleaved A/B min-of-15×6: **−35.0%**
(0.0599 → 0.0418 µs/B; tree-sitter gap 1.93× → 1.35×).
Receipt: `benchmarks/sota/results/2026-11-20-memo-pooled-writes.md`.
Next profile-ranked blocks: the dispatch loop itself and the per-char
scan kernels; the Phase 2 LR routing stays the structural endgame.

## Dispatch-loop hoists FALSIFIED (2026-11-20, interleaved A/B min-of-15×6)

Moving the recursion-depth gate to the backtrack push sites (mirroring
the capture-depth convention) + hoisting the debug branch out of the
loop: min-of-min 19 376 vs the pre-hoist 18 723 µs — no win (the
per-instruction depth check and the debug branch were already
near-free predictable branches). Reverted; the loop check stays where
rs#533 put it. The dispatch loop's real cost = the match dispatch and
the scan pre-dispatch's two extra variant compares — the pre-dispatch
exists deliberately (the 1.5.6 +12% receipt), so the loop is at its
measured local optimum until the Phase 2 LR routing replaces it.

## Slice B design: the automaton backend (rs#574 Phase 2, the next opener)

Slice A shipped the router's evidence: `rule_lr_provability()` proves
55.5% of the fleet's rules DFA-lowerable, with the refusal profile
attributed (overlapping choice 666, poisoned closures 528, loop-exit
ambiguity 261, nullable bodies 136 — the survey test pins the counts).
Slice B = the backend the verdicts route onto:

1. **The table builder**: for a Provable rule, compile the atom tree
   to a packed byte-transition table (state × lead byte → next
   state/accept/epsilon), reusing the exact walk `lr_states` already
   performs — the state numbering falls out of the same traversal.
2. **The instruction**: `DfaRun { table_idx, span_into }` — the VM
   walks bytes through the table (no per-byte instruction dispatch),
   accepts at the table's accept state, records the span like Span.
3. **The capture carve-out**: first scope = CAPTURE-FREE provable
   rules (pure spans — the string/ws/number interiors); Named
   markers inside provable bodies ride a second slice (accept-state
   → value-synthesis metadata).
4. **The routing**: `compile_atom` checks the verdict; Provable +
   capture-free + the entity-closure rule → DfaRun; everything else
   = the current lowering (byte-identical trees by construction —
   the spans are identical, so identity gates hold trivially).
5. **The A/B gate**: the json corpus (its string/ws/number interiors
   prove) — the win = replacing the per-byte instruction streams
   with one table walk; measured against the 0.0418 µs/B baseline,
   targeting the dispatch-loop block (76% of the remaining profile).

## Slice B stage 2 SHIPPED (2026-11-21): the routed repetition loops

The compile-path routing: unlimited plain repetitions with a single
single-value terminal body (Str / ASCII-class Re) lower to one
self-contained DfaRun. The loop table marks the body's fragment
accepts; the executor synthesizes the identical per-iteration spans,
rewinds to the last boundary on a partial trailing iteration (never
counts it — the PEG's no-partial rule), checks min against the
crossings, and builds the same tagged envelope BuildRep constructs.
Proven walker-identical across full/partial/empty/min-fail/refusal
inputs (the packrat_differential gate). The value-1 body restriction
is the honest stage boundary: multi-value bodies (sequences, named
captures) need the accept-state synthesis metadata — stage 3 — before
their trees survive. json A/B: unchanged within noise (its bodies
already bulk-low; the win targets the fleet's literal/class loops).

## Slice B stage 3 design sketch (2026-11-21): flat multi-value bodies

**Ground truth** (probed walker+VM, identical today): for
`*("a" "b")` on "abab", ONE iteration pushes ONE tagged `:sequence`
array whose element list is the iteration's value — here fused to a
single span `[2(s:":sequence"; in:"ab";)]`; on "abx" the partial
trailing iteration vanishes (`[2(...)]` = one complete iteration).
The classic Choice loop builds this via the body's inline lowering
+ BuildRep; the routed table must synthesize it.

**The enabling proof**: the provability analysis guarantees
pairwise-disjoint choice FIRST sets — a deterministic subset state
contains AT MOST ONE fragment-accept node. So the emission plan keys
by accept NODE id: no per-branch subset plans needed for flat bodies.

**The metadata** (per routed loop table, riding the program-side
`DfaLoopMeta` registry — keeps `DfaTable`/`DfaRun` at their shapes):
1. `iteration_accept: u32` — the NFA node id whose crossing closes an
   iteration (star(body)'s loop-back source; the body's FINAL fragment
   accept).
2. `fragment_plan: Vec<(accept_node_id, fragment_index)>` — every
   intermediate fragment-accept mapped to its slot in the iteration's
   value list (flat Sequence of value-1 atoms → fragments 0..n-1, the
   last being the iteration accept).
3. `tag: the body's envelope tag` (`:sequence` for Sequence bodies).

**The executor**: per iteration, a small cut stack — crossing ANY
plan accept pushes `(fragment_index, crossing_position)`; crossing
the iteration accept closes the iteration: build the tagged array
from the cuts (each fragment's span = cut[i-1]..cut[i], iteration
start..cut[last]); rewind semantics unchanged (drop the partial
iteration's cuts on stuck). The value-1 path stays the fast path
(the plan degenerates to the existing single-span push).

**Out of stage 3** (stays on the classic loop): nested multi-value
bodies (a fragment that is itself multi-value — spans NEST, the plan
becomes a tree, not a cut list), bodies with predicates/zero-width
atoms mid-chain, and non-Sequence tags. Also gated on the owner's
zero-match `""` decision (rs#574) — a zero-iteration envelope's
delivery may change under (a), and stage 3's flat path inherits
whatever that decision sets.
