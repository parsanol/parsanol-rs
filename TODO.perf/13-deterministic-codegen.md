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

## Slice B stage 3 tranche 1 SHIPPED (2026-11-21, #618): flat sequences

The design simplified on contact: a flat Sequence's thompson chain has
ONE fragment accept (the chain end) — no cut stack, no per-accept
plans. The only missing piece was the per-iteration VALUE wrap: the
classic path emits one BuildSeq per iteration whose fold merges the
adjacent fragment spans into a single span, so the routed executor
wraps the iteration span in a `:sequence` tag behind a parallel
registry flag (`mark_dfa_loop_sequence` / `get_dfa_loop_sequence_wrapped`
— `DfaLoopMeta` keeps its published fields). The routing widens to
flat Sequences of value-1 span terminals (non-empty Str / ASCII-class
Re). Pinned: engagement assertions on four sequence grammars;
collapsed walker-identity x12 inputs each (the walker's raw tier
splits mixed literal+span values — the collapsed wire is the
cross-engine contract); the routed synthesis's raw shape exact
(`[3(s:":repetition";[2(s:":sequence";in:"1x";)];...)]`). The
tranche-2 scope (named/capture-wrapped bodies, nested fragments)
stays on the classic loop. Gates: lib 590/590, integration rc=0,
clippy/fmt/typos, semver-checks vs main = no update required.

## Slice B stage 3 tranche 2 SHIPPED (2026-11-21, #620): named wraps

Probed first: the classic per-iteration shape for a Named body is
one `{name: inner}` hash on BOTH engines (the inner = the bare span
for a terminal body, the `:sequence`-tagged merged span for a flat
span sequence). The routing accepts Named-wrapped span terminals /
flat span sequences; the executor emits the hash per iteration via
the parallel `mark_dfa_loop_named` / `get_dfa_loop_named_key`
registry (published shapes untouched). CAPTURE/SCOPE/TRIVIA wrappers
are EXCLUDED by design: their compile arms ride the capture
machinery (CapMark/RecordCapture/ScopeEnd) which the routed executor
does not run — routing them would silently drop capture recording
even though their tree shapes look bare. Pinned: engagement on four
named grammars, collapsed walker-identity x12 inputs each, the raw
synthesis shape (`{item-> [:sequence 1x]}` per iteration). Gates:
lib 591/591, integration green modulo the debug-profile timing gate
(release 2/2), Still on the classic
loop: capture/scope/trivia wrappers, nested multi-value fragments
(the plan is a tree), non-terminal value atoms.

## Slice B stage 3 tranche 3 SHIPPED (2026-11-21, #623): rule refs

A RULE REFERENCE as the loop body routes — the fleet's dominant loop
shape (`*(digit)`, `*(ws)`, `*(item)`). The classic path lowers a
rule ref as a subroutine CALL to the target's compile, so the
per-iteration value = the TARGET's shape: a Named-wrapped rule (the
v2 named-tree law) hashes `{name: inner}` keyed by the RULE name; an
unwrapped DSL-style body delivers the bare span / `:sequence`-tagged
span directly. NO metadata changes — the Entity arm reuses the
tranche-1/2 registries. Entity cycles refuse in thompson (no
recursion reaches the routed path); double-wrapped targets (rule
Named -> label Named) refuse for now (their per-iteration value
nests hashes). GOTCHA recorded: the DSL builder's FIRST rule becomes
the grammar root — a fixture declaring the referenced rule first
serializes a degenerate single-atom grammar; the differential
fixtures must declare the ROOT rule first. Gates: lib 593/593,
integration rc=0, clippy/fmt/typos, semver clean.

## Slice B stage 3 tranche 4 DESIGN (2026-11-21): seq-of-refs, the cut assembly

**Probed ground truth**: a flat Sequence whose atoms are RULE REFS
(`*(sep word)`) — one iteration = tagged `:sequence` with the
elements' values as SEPARATE spans (`[3(s:":sequence"; in:" "; in:"ab";)]`
— the classic fold does NOT merge ref-element spans, unlike tranche 1's
adjacent-literal fusion). Per-element values follow each ref's compile
shape (bare for unwrapped rules; `{name: inner}` hashes for Named rules).

**The design** (measured open — not started): the thompson chain gives
each element its own accept node mid-chain, so the synthesis needs
CUT POSITIONS per iteration:
1. `build_loop_dfa` marks every element accept with its index (a subset
   contains at most one chain node — the disjointness proof — so the
   per-state mapping is a function, not a set); returns
   `element_accepts: Vec<Option<u8>>` per DFA state alongside the
   existing `iteration_accepts`.
2. Registry (parallel, additive): per loop table — the per-state
   element-accept index + each element's value descriptor
   (`None` = bare span, `Some(name)` = hash wrap).
3. Executor: per iteration, a small cut buffer — each element-accept
   crossing records `(element, position)`; the iteration-accept
   crossing (the LAST element) closes the iteration: assemble the
   value list — element i's span = cuts[i-1]..cuts[i] (start_0 =
   iteration start, end_last = the boundary) — wrapped per descriptor,
   then the `:sequence` tag. The single-path table walk has no
   backtracking inside `exec_dfa_run`, so the cut buffer is safe.
4. The routing: extend the Sequence arm to accept Entity-ref elements
   (their targets = span terminals / Named-wrapped span terminals).

**The A/B upside**: this routes the fleet's OUTER loops (`0*(sep word)`
— sequences of rule refs everywhere), composing with the already-routed
inner loops; the 2026-11-21 A/B receipt (#625) measured this grammar's
outer loop still classic. The win scales with the scan-to-tree-build
ratio (the rule-ref A/B leg measured the hash-write bound).

## Slice B stage 3 tranche 4 SHIPPED (2026-11-21, #627): the cut assembly

The design's contact with reality simplified AND corrected it: the
thompson chain gives each element its own accept; the per-state
element-accept index is a FUNCTION (the disjointness proof), and the
executor's cut buffer records `(element, position)` at each crossing —
the LAST element's crossing doubles as the iteration boundary, so no
separate iteration-accept bookkeeping is needed for the sequence path.

**The fold's per-case merge, reproduced**: the probes showed the
classic BuildSeq fold MERGES adjacent spans produced INLINE (Str/Re
instructions) but keeps spans delivered through rule-ref CALLS
separate — the synthesis carries a three-state per-element descriptor
(`ElementDesc::InlineBare / RefBare / Hash(name)`), and the assembly
groups consecutive InlineBare runs into one span. Both observed
classic shapes hold in one executor (the tranche-1 merged pin and the
seq-of-refs separate pin).

**GOTCHA recorded**: the DSL builder FUSES adjacent literal strings
at build time (`seq("a","b")` → `Str("ab")`) — a 1-element Sequence
body must still track its single element or the cut plan desyncs from
the router's element count (caught by a subtract-with-overflow on the
tranche-1 fixtures; the fix lowered the element-chain branch to
`!atoms.is_empty()`).

**Pins**: engagement on three seq-of-refs grammars; collapsed
walker-identity across the input battery; the raw cut-assembly shape
(ref elements stay separate spans). Gates: lib 594/594, integration
rc=0 (37 targets, timing gate included), clippy, fmt, typos,
semver-checks vs main = no update required. Remaining stage-3 scope:
NESTED multi-value fragments (the plan becomes a tree) — every flat
shape now routes.

## Slice B stage 3 tranche 5 CLOSED (2026-11-22, #631): the fold never merges

The open question is resolved — and the answer falsified tranche 4's
merge rule.

**The characterization**: the producing-instruction matrix (str_str /
str_re / re_str / re_re / re_multi_str) run against the CLASSIC
engine pinned at 8983451: the classic BuildSeq fold NEVER merges.
Every element lands as its own span (`in:"ab"; in:"d";`) for every
inline pair; the zero-match legs (re_re, re_multi_str — min=0 loops
over non-matching classes) agree empty on both engines.

**The falsification**: the "classic merges inline spans" observation
was CIRCULAR — `assert_agree_collapsed`'s right side was the routed
VM itself, so the synthesis was compared against itself. The
"merged" tranche-1 fixture was the DSL builder's literal fusion
(`seq("a", "b")` → `Str("ab")`: a ONE-element sequence — single-span
output, not a fold). Tranche 4's `InlineBare` run-merging (#627)
was built on that misreading; its section above is superseded here.

**The correction (#631)**: `push_assembled_iteration` drops the
run-merging — every element delivers its own span, byte-identical to
the classic raw tree. The collapsed wire is unchanged, so the #625 /
#629 A/B receipts stand (they measure the collapsed wire). The
element plan now routes through Named/Entity wrappers too
(`build_loop_dfa` unwraps Named/Entity before the chain check; the
router extracts `seq_atoms` through the Named wrapper; the assembly
applies the outer named hash). Pins updated to the separate-span raw
shapes. New regression: `differential_fold_merge_characterization` —
walker == VM RAW per producing-instruction pair, engagement-asserted
(the `has_iteration_accept_tables` law).

**Aftermath**: `ElementDesc::InlineBare / RefBare` describe
provenance only (inline Str/Re instruction vs rule-ref call) — both
deliver a separate bare span; the executor keys on `Hash` alone. The
distinction stays because the nested-fragments slice reads the
producing path. Gates: lib 595/595, integration rc=0 (37 targets,
timing gate included), clippy, fmt, typos, semver-checks vs main = no
update required. Remaining stage-3 scope: NESTED multi-value
fragments (the plan becomes a tree).

## Slice B stage 3 tranche 6 DESIGN GROUNDING (2026-11-22): the nested raw law

Probed the CLASSIC engine (walker-identical on every shape) for loop
elements that are THEMSELVES sequences — the remaining stage-3 scope:

- `*(([ab] c) d)`: the iteration value keeps the nested grouping —
  `:sequence[ :sequence[in:"a"; in:"c";]; in:"d"; ]`. Every element
  its own span; every Sequence atom emits its own `:sequence` node.
- `*((inner: [ab] c) d)`: the Named wrap is
  `{inner -> :sequence[in:"a"; in:"c";];}` — a hash around the
  nested node (the v2 named-tree law composes).
- `*((("ab" "cd")) e)` (builder-fused inner → `Str("abcd")`): the
  fused ONE-element inner sequence STILL emits its `:sequence`
  wrapper (`:sequence[in:"abcd";]`) — fusion changes element counts,
  never removes nodes.
- `*((w t) d)` with `w`/`t` span-terminal rules: the refs deliver
  BARE spans inside the nested node (no `{w: ...}` hash).

**The design consequence**: the element plan becomes a tree
(`NestedSeq(children)` always emits the node; `Hash(name)` wraps;
bare leaves), and the executor's cut buffer needs SUB-CUTS within an
element's span (each leaf's accept already lands in the DFA chain —
the flattening is free; only the assembly is tree-shaped).
