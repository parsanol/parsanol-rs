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

## Slice B stage 3 tranche 6 SHIPPED (2026-11-22, #633): the plan tree

The grounded design landed: the element plan became a TREE
(`ElementDesc::Nested(children)` — one `:sequence` node per Sequence
atom, ALWAYS, even a builder-fused one-element inner;
`HashNested(name, children)` — `{name -> :sequence node}`), the
builder chains LEAF BY LEAF (`leaf_chain` flattens nested elements
depth-first, unwrapping Named/Entity exactly as the plan does — the
per-leaf cut indices and the plan tree in lockstep; the determinized
table is unchanged vs whole-subtree Thompson, only the accept
bookkeeping is per-leaf), and the executor assembles recursively
(the plan resolved once with interned keys, the cut buffer sized to
the leaf count).

**The bonus catch**: the ref-to-seq-rule loop BODY (an Entity body
unwrapping to a Sequence) had routed since tranche 4 with a
seq_wrapped plan and NO element cuts — it delivered ONE merged span
per iteration raw (`[:sequence, "ac"]`) where the classic delivers
per-element spans (`[:sequence, "a", "c"]`). Collapsed-identical (the
collapse merges contiguous InputRef runs — the same veil the tranche-5
falsification hid behind), raw-divergent. Tranche 6's Entity unwrap in
the seq-atom extraction plus the recursive plan makes it
walker-identical; the `ref_seq_rule_body` fixture pins it.

**Pins**: five nested fixtures (bare / named / mixed hash-leaf / refs
/ ref-to-seq-rule body), engagement-asserted (`has_iteration_accept_tables`), the collapsed battery plus RAW walker equality on the whole
battery, the nested raw shape pinned. Gates: lib 596/596, integration
rc=0 (workspace suite green), clippy -D warnings, fmt, typos,
semver-checks vs main = no update required.

**Round 2 (the semver correction)**: CI's semver gate compares
against MAIN (the local `cargo semver-checks` default compares
against the LAST RELEASE — 1.5.13 — and masked the break). Round 1
added `Nested`/`HashNested` variants to `ElementDesc` (on main since
tranche 4) — a main-to-main major, the additive-registry law's exact
case. Round 2: `ElementDesc` returns to its exact main shape; the
plan tree is a NEW pub type (`ElementPlan`) on a NEW parallel
registry (`dfa_loop_element_plans`); `DfaLoopElements` keeps its
published struct (names = the flat per-leaf fallback). LAW UPDATE:
the local semver gate must run `--baseline-rev origin/main` to
mirror CI.

**The fleet coverage survey** (`loop_routing_survey_over_the_corpus_fleet`,
shape-only classification — the first draft called `build_loop_dfa`
per site and the determinization over arbitrary fleet loops blew up
the subset construction, spinning three orphaned 100%-CPU test
binaries): over the pubid fleet artifacts —

- 59 flavors, 1479 unbounded loop sites, **1036 route-shaped
  (70.0%)**: 435 `terminal` (tranche 1) + 600 `wrapped-terminal`
  (tranches 2/3) + 1 `wrapped-seq` (tranches 4-6).
- The unrouted tail (443 sites, 30.0%): 313 `other` (loop-of-loop
  bodies, choice bodies, predicates) + 130 `seq-unplan-able`
  (structured / recursive sequences) — the loop-composition slice's
  material, now sized.
- The pubid fleet carries ZERO plain-seq loop bodies; that shape
  lives in the bench grammars the 2026-11-21 A/B receipt measured.

**Stage-3 scope after tranche 6**: every INLINE flat and nested shape
routes. Remaining (a later slice): loop-COMPOSITION routing — bodies
whose elements target other LOOPS (the bench fleet's outer
`0*(sep word)` shapes measured in the 2026-11-21 receipt are
seq-of-refs, already routed; loop-elements-of-loops remain classic)
and Entity targets whose shapes the plan cannot name. The survey
sizes that tail at 443 fleet sites; the tranche-7 probe (2026-11-22)
REVISED the feasibility read: the classic delivers
`[:sequence, [:repetition, per-inner-span...], span]` for
loop-of-loop bodies (walker-identical; Named composes as
`{inner -> [:repetition, ...]}`), and the inner boundaries ARE
recoverable — in the chained construction the inner accept node is
visited once per inner iteration, so a per-state inner-accept
marking (a second parallel registry) can record envelope cuts
exactly like the outer loop's. Not a structural wall — an
engineering slice.

## Slice B stage 3 tranche 7 SHIPPED (2026-11-22): loop-composition envelopes

The tranche-7 probe's design landed: loop bodies whose elements are
THEMSELVES loops route. The element's value is the inner loop's
ENVELOPE — one `:repetition` node whose values are the
per-inner-iteration spans (the classic's exact raw shape; Named
wraps compose as `{inner -> [:repetition, ...]}`).

**The construction**: `build_loop_dfa_envelopes` (a NEW pub fn — the
tranche-4 `build_loop_dfa` stays frozen) chains the inner loop's
copies into the byte chain; every copy accept is an INNER boundary
(a second parallel lane, `dfa_loop_inner_accepts`); the envelope
element carries NO element-accept marker at all. The full plan tree
is a NEW pub type, `ElementPlanTree` (the tranche-6 variants plus
`Envelope`/`HashEnvelope`), on a NEW registry (`dfa_loop_plan_trees`)
— extending `ElementPlan` itself is gated TWICE over: variant-adds
AND even `#[non_exhaustive]` are main-to-main majors (the
`enum_marked_non_exhaustive` lint fired; both taught the same
lesson).

**The merge lesson (the marker law, sharpened)**: the zero-path edge
(an empty envelope must reach the next element) merges the envelope
boundary into EVERY iteration-boundary state — a state carrying TWO
element markers, where the single-marker `find_map` lane silently
drops one (a cut never recorded → a span underflow). The fix is
structural: envelope elements contribute NO fixed-lane markers — the
inner lane carries the boundaries, and the assembly's prev-boundary
walk supplies the span starts (an empty envelope's slot is never
read). The one-marker-per-state law now holds by construction:
boundary connectors carry nothing.

**Pins**: four fixtures (bare inner, Named-wrapped inner, bounded
`a{2}` inner, mixed `([ab]* c [de])`), engagement-asserted, the
collapsed battery plus RAW walker equality on the whole battery, the
envelope raw shape pinned (per-inner-iteration spans inside the
`:repetition` node). Scope: ONE envelope level (nested envelopes
refuse); Entity-to-loop targets stay classic. Gates: lib 598/598,
full workspace suite green, clippy -D warnings, fmt, typos,
semver-checks `--baseline-rev origin/main` = no update required.

**The post-tranche-7 survey** (plan_able now accepts envelope
elements, shape-only): 59 flavors, 1479 sites, **1062 route-shaped
(71.8%)** — 435 terminal + 600 wrapped-terminal + 22 seq + 5
wrapped-seq; the tail narrows to 417 (313 `other`: loop-of-loop
BODIES, choice bodies, predicates + 104 `seq-unplan-able`). The
fleet's loop-of-loop INLINE elements were rare (+26 sites); the
tranche's value is shape-class completeness and the raw-identity law
for the composition shapes.

## The zero-match contract LANDED (2026-11-22): ruby's named law, both flatten layers

The stage-2 differential's filed divergence (rs engines delivering the
tagged empty array where ruby delivers `""` at a zero-match `min=0`
loop) is fixed — but the owner's probe matrix was INCOMPLETE, and the
corpus caught it: ruby's `CanFlatten#flatten_repetition(list, named)`
(the SSOT) returns `[]` when the repetition value is a Named wrapper's
DIRECT value and the plain empty string `""` everywhere else. The tgpp
corpus case pins the named half: `parts = (*part) as parts` at zero
matches delivers `{"parts": []}`, not `{"parts": ""}`.

**The design (ruby's architecture, ported):** the engines keep
producing the TAGGED EMPTY array (ruby's own marker — the raw-tree
contract is unchanged), and BOTH flatten layers implement the shallow
named law off it:

- `collapse_ast` (the native wire) gains the additive
  `collapse_ast_named(node, arena, named)`: a hash value recurses
  `named=true` (ruby's Named boundary), array children recurse
  `false`; the EMPTY repetition stays the tagged array under
  `named=true` and folds to the interned empty string elsewhere; a
  ZERO-LENGTH repetition result (e.g. the stalled one-iteration case)
  folds to the plain empty string in both contexts (ruby's join); and
  empty-string / zero-length-span members now act as zero-length gaps
  in the fold accounting (ruby's `join_string_list` merges them), so a
  zero-match INNER loop folds away inside a sequence — the `re_seq`
  row of the owner's matrix (`[0-9]* "!"` on `"!"` → one `"!"` span).
- `to_parslet_compatible` (the parg shape lane) gains the private
  named-aware form: hash values (single- and multi-key) recurse
  `named=true`; the EMPTY repetition returns the empty list at a hash
  value and the empty string elsewhere. The single-key machinery's
  existing repetition wrapping keeps delivering `[]` (why the corpus
  was green pre-change for the named half).

**Two engine corrections that fall out** (the fold layers made them
reachable): the bulk class path's zero-run now delivers the tagged
empty array (it delivered a bare zero-length span — a Named-wrapped
class loop would have rendered `{ws: ""}` where ruby gives `{ws:
[]}`), and the classic charset Span loop OWNS the rep register (the
ScanClass discipline; the lowering drops its RepCount) — a zero-member
run counts nothing instead of the stalled-iteration miscount, with NO
Choice machinery (the dispatch-shape gates stay at zero). Dynamic-block
capture values gain ruby parity too (ruby's Capture flattens unnamed —
`""`).

**Pins**: `zero_match_loops_deliver_the_ruby_empty_string` (the probe
matrix as a table-driven test: class/str/body-named loops on
{`""`, `"x!"`, `"zz"`} asserting the raw tagged-empty marker on both
engines + the collapsed plain-empty-string wire + the shape lane's
named law (`[]` under a Named wrapper, `""` unnamed) + the
fold-across pin + the min=1 raise); the stage-2 differential's
class-body matrix widened to the FULL input set (#615's zero-match
scoping is obsolete — the agreement domain covers it now).

**Gates**: lib 599/599, full workspace suite green (48 suites incl.
the tgpp ruby-oracle corpus case), clippy -D warnings, fmt, typos,
semver-checks `--baseline-rev origin/main` = no update required.
Scope guards: the stalled-iteration discipline is untouched (a body
that MATCHES empty still counts its first stalled iteration on both
engines); absent optionals keep the `[:maybe]` tag and its
(nil/`""`) laws; the raw-tree contract is unchanged (the corpus pins
the raw shapes that existed before).

## Slice B stage 3 tranche 8 SHIPPED (2026-11-22): choice bodies routed

The largest tail bucket routes: a loop body that is an Alternative of
bare span arms with pairwise-disjoint non-nullable FIRST sets lowers to
one DfaRun. Disjointness is the provability precondition checked
locally on the body (the same requirement the rule-level proof makes):
under it PEG ordered-choice == DFA longest-match exactly, so the routed
table is semantics-exact; overlapping-arm choices refuse and the
classic loop keeps its ordered-choice behavior (the engagement-refusal
fixture pins that).

**The construction**: `element_plan`'s Alternative arm yields ONE Bare
leaf (the per-iteration value is one bare span whichever arm matched);
the builder's element loop forks every arm's fragment from the element
entry and joins them into one shared node, with EVERY arm's accept
marking the SAME leaf slot (the one-marker-per-state law holds —
different states, one slot). Whole-body choices ride the existing
whole-subtree thompson union; named-wrapped choices take the flat
named shape (`{name: span}`). A choice element inside a flat sequence
counts as one leaf in lockstep with the plan. No new pub types: the
existing `ElementPlanTree` names every tranche-8 shape.

**The envelope-exit catch** (the tranche-7 latent bug the arithmetic
tail flushed out): a min>=1 UNBOUNDED envelope element had NO exit from
its last mandatory copy — the boundary was reachable only through the
tail copy, so `ws* op ws* 1*digit` (the arithmetic tail, the first
mixed envelope+choice real shape) demanded a SECOND inner iteration
before any outer iteration could close; "1+2" parsed as bare "1". The
fix: the last mandatory copy's accept reaches the boundary (unbounded
max may stop at the minimum). The backend-parity suite caught it
exactly as the corpus/differential gates are designed to.

**Pins**: the tranche-8 differential (whole-body choice / named choice
/ choice-as-element, engagement-asserted; the overlapping-FIRST
refusal; RAW walker==VM equality on the core inputs; the per-iteration
shape pins; the arithmetic-tail envelope-exit regression with the
walker==VM battery). The zero-match spec's classic-lane fixture
migrated from a choice body (which now routes — the fixture's own
comment promised the re-check) to a capture-wrapped body.

**The post-tranche-8 survey** (shape-only, 61 flavors): **1300/1490
sites route-shaped (87.2%)** — 435 terminal + 606 wrapped-terminal +
230 choice + 24 seq + 5 wrapped-seq. The tail narrows to 190: 86
`other` (loop-of-loop bodies, predicates, structured/recursive
sequences inside choices) + 104 `seq-unplan-able`. The choice class
reports the shape; the disjointness gate is the router's refinement.

## The tail census (2026-11-22, shape-only, post-tranche-8)

The 190 unrouted fleet sites' exact shapes (the atom-level census; the
classes overlap the rs survey's coarser buckets):

- **~34 choice-with-structured-arms** (choice-of-sequence arms mostly,
  19 sites of `choice-of-seq x2` alone): routeable by generalizing
  tranche-8's fork/join to per-arm LEAF RANGES — the matched arm is
  the one whose last-leaf slot carries a cut; the assembly picks the
  arm by that slot. Requires a plan-tree Choice variant → the
  additive-registry ladder (a new pub type on a new parallel registry;
  extending `ElementPlanTree` would repeat the unreleased-major trap
  that produced the ladder).
- **58 seq-with-inner-loop** (bounded/unbounded inner loops inside
  sequence bodies): the tranche-7 envelope machinery already owns the
  unbounded half; the census's loose inner-shape test over-counts —
  the rs survey's seq class (24) is the true covered figure; the
  residue is structured inner bodies.
- **11 loop-of-loop bodies** (`(x*)*`): two-level inner lanes — an
  engineering slice, small.
- **9 lookahead-guarded bodies**: zero-width predicates — the
  byte-driven DFA has no machinery for them; a later architecture
  slice.
- **The recursive/structured residue** (the `seq-unplan-able` bulk):
  recursion is architecturally unrouteable by a finite DFA — these
  sites belong to the classic path permanently, or to Phase 2's LR
  tables (which handle recursion via the table's call model).

**Strategic read**: the remaining coverage tranches total ~50-90
routeable sites with each slice needing the new-type ladder or new
zero-width machinery — diminishing returns against Phase 2, which is
the mission-critical throughput lever (the 0.03 µs/B gate).

## Phase 2 OPENED (2026-11-22): the measured state on the work machine

The SOTA json corpus (447,801 B, the runners' own verification-gated
lane, min-of-N), same machine, same run:

- **parsanol VM: 0.0444 µs/B** (17.1–21.7 ms per pass) — the tranche
  campaign's coverage work closed the gap from the recorded 1.93x to
  **1.25x** against tree-sitter.
- tree-sitter (rust): 0.0354 µs/B (15.8 ms).
- parsanol interpreter (the walker): 0.480 µs/B (the walker's 10.8x
  distance to the VM is why the compiled-VM lane is the only lane that
  matters for the gate).
- Lark LALR (python): 0.427; ANTLR (python3): 2.533.

**The profile (sample, 4161 samples over a 400-round VM run)**:

- The **VM dispatch loop holds ~88%** of samples (the instruction
  fetch/decode/stream cost itself — the LR-fusion target: remove the
  dispatch for fused regions, the jit#458 lesson's corrected framing:
  not operand-specialization, but REGION FUSION — whole-rule tables
  that skip the instruction stream entirely).
- **`store_tagged_array_indexed` ~8-10%** — the arena's array pool
  reallocs per store (RawVec growth + memmove + mach_vm_copy traps in
  the profile): the pre-sized/chunked pool is the identifiable
  non-dispatch win.
- **`exec_dfa_run` ~10.6%** — the routed loops; inside it the
  per-invocation `cuts`/`inner_cuts` buffer allocations (from_elem +
  memmove ≈3%) — hoistable into per-VM scratch.

**The Phase-2 slice plan** (evidence-ranked):
1. The arena array-pool chunking (kill the per-store realloc) —
   measurable, bounded, ~8-10% of the VM lane.
2. The routed-loop buffer reuse (per-VM scratch for cuts/inner_cuts) —
   ~3%.
3. The LR-table fusion proper: whole-rule tables driving the value
   stack directly (tree-sitter's GLR-free deterministic core path) —
   the 88% dispatch cost's structural answer; the design continues in
   the next session with the instruction-histogram detail.

## Phase 2 slice 2 REVERTED (2026-11-23, measured): the DfaRun scratch/cache

The profile's exec_dfa_run allocations (the per-invocation
`cuts`/`inner_cuts` from_elem + the plan re-resolution, ~3% of
samples) were addressed with per-VM scratch buffers (mem::take
around the body) plus a per-table asm-plan cache (remove/insert). The
interleaved A/B (alternating saved binaries, 5 rounds): baseline
17.1–18.3 ms, the scratch build 18.8–19.1 ms — a consistent ~5-8%
REGRESSION. The cache bookkeeping (two hash ops + the take/restore)
costs more than the small-plan allocation it saves: the fleet's
routed plans are 1–3 leaves, and `resolve_plan_tree` on a 1–3-node
tree is a handful of cheap interns. REVERTED; recorded per the
jit#458 discipline.

The read for the LR fusion: the exec_dfa_run lane's real cost is not
its allocations (they are small and warm) — the dispatch loop's 88%
share is the structural target, and the fusion must remove
INSTRUCTIONS, not bookkeeping.

## Phase 2's instruction histogram (2026-11-23): where the dispatch spends json

`histogram_probe_json` (the opcode counts over the SOTA json corpus's
largest doc, 151,823 B, the compiled-VM lane): **628,324 dispatched
instructions = 4.14 per byte**.

| share | opcode | what it is |
|---|---|---|
| 17.0% | ByteDispatch | the value/member choice dispatch |
| 16.2% | Jump | loop backs + choice falls |
| 16.2% | RepCount | per-iteration counting (classic loops) |
| 16.0% | PartialCommit | per-iteration commits (classic loops) |
| 10.2% | Regex | per-char class matching — THE STRING INTERIOR |
| 7.3% | CharSet | per-char class matching (the ws-style bodies) |
| 4.4/4.2% | Call/Return | rule boundaries |
| 2.1/1.6% | BuildSeq/BuildHash | the value nodes |

**The design read**: ~49% of ALL dispatch is CLASSIC PER-CHAR LOOP
MACHINERY (RepCount + PartialCommit + Regex + CharSet), and the
single biggest byte-share inside it is the STRING INTERIOR — the
`( escaped / char_unescaped )*` body that slice 4's neg-scan premise
falsified for the neg-scan fusion but that TRANCHE 8's choice routing
plus the per-arm leaf slots open properly: `escaped` is a flat
leaf-sequence arm and `char_unescaped` is a one-leaf arm — with
per-arm cut slots the DFA consumes the interior as one DfaRun and the
assembly picks the arm by which slot carries a cut. That moves
~60%-of-bytes work out of the dispatch loop.

**The Phase-2 slice plan (revised, evidence-ranked)**:
1. **Tranche 9: choice arms with per-arm leaf-slot ranges** — the
   string interior routes (the new-type ladder: a Choice plan variant
   on a NEW pub type/registry per the additive law). The gate: the
   arms' FIRST sets pairwise-disjoint (the provability precondition,
   as tranche 8).
2. The loop-scaffolding share that remains classic after tranche 9
   (RepCount/Jump/PartialCommit on the residual bodies) — the LR
   fusion proper: whole-rule tables driving the value stack, the
   design continues against the post-tranche-9 histogram.
3. ByteDispatch's share drops with every routed choice — re-measure
   per slice.

## Tranche 9's ground truth (2026-11-23): the json string interior's exact shape

The SOTA artifact's string body: `[36] = *( [35] )` where

- `[35] = ( [x20-21]? / [x23-5b] / [x5d-10ffff] / [34] )` — FOUR arms,
- `[34] = Sequence[17, 33]` — the escaped arm: the backslash class then
  `[33] = ( 9 bare class arms / Sequence[26, 31] )` — a NESTED choice
  with a sequence arm of its own.

So the escaped arm is NOT a flat leaf sequence: tranche 9 needs the
RECURSIVE per-arm plan — each arm's plan is an ElementPlanTree-shaped
subtree (the existing type), the new pub type carries
`Choice { arms: Vec<(arm-plan, leaf-count)>, leaf_count }` on its own
registry, the builder walks the arm's atom tree depth-first marking
the per-leaf slots in the plan's leaf order (lockstep), and the
assembly picks the arm by its LAST leaf slot's cut, then assembles the
arm's plan recursively. The gate stays tranche 8's: the arms' FIRST
sets pairwise-disjoint and non-nullable ([35]'s arms qualify: the
classes partition the byte range, the escaped arm starts at the
backslash).

The measured prize: the string interior is ~60% of json bytes riding
per-char Regex + loop scaffolding today (10.2% Regex + its
RepCount/Jump/PartialCommit share); one DfaRun over it is the largest
single step toward the 0.03 gate.

## Stage 3 tranche 9 SHIPPED (2026-11-23, #652): the per-arm choice slots

The recursion (per-arm plan subtrees) landed on its own registry:
`ElementPlanTree9::Choice` + `ElementPlanChoiceArm9` (additive, no
edits to published shapes; semver 223 pass). `build_plan9` walks the
arm's atom tree building the NFA fragment and the plan in lockstep —
arm i's leaves take the arm's contiguous slot range, the assembly
picks the arm whose LAST-leaf cut carries THIS iteration's boundary
(strictly after `prev`, so the shared cut buffer's stale
earlier-iteration values cannot win the pick), and the choice hands
back `start + total` so the parent's slot numbering survives the
union. The gate stays tranche 8's: pairwise-disjoint non-nullable
FIRST sets.

**The regression catch**: the first cut routed every choice body
through the per-arm machinery — including tranche 8's bare-arm bodies.
The interleaved json A/B measured a ~50% regression (27.3-28.6 ms vs
17.8-18.8 ms): the flat lanes' whole-span push is cheaper than the
per-arm pick for bodies where it suffices. The landed split keeps
`choice_span_arms` bodies on tranche 8's shared-slot lanes exactly
(builder + markings + executor); only the tranche-9-only shapes take
the new machinery. Post-split json A/B: parity within noise (16.9-18.0
vs 16.8-18.8 ms interleaved minima).

**Scope honesty**: the json grammar's exact interior does NOT route
yet — its `[^"\\]` arm is a negated Unicode class whose complement
spans multi-byte codepoints, and the stage-1 byte table refuses those
(a multi-byte member consumes 2-4 bytes as ONE value; a one-transition
byte edge cannot represent that — the deferred multi-byte class
slice). The shipped fixture is the ASCII-bounded interior: it routes,
and the differential battery pins escapes, mixed arm leaf counts, and
per-arm slot reuse across iterations (the stale-cut hazard).

**The A/B read**: the loop-heavy escape/plain grammar (the tranche-9
shape, 396,189 B corpus, min-of-15 interleaved) shows PARITY (54.6 vs
56.1 ms mins) — the per-iteration arm-pick costs what the per-byte
dispatch saving delivers. The LR fusion (Phase 2) must fold the
arm-pick into the table walk: that is where tranche-9's win lives.

## Multi-byte class slice SHIPPED (2026-11-23, #653): UTF-8 sequence chains

The stage-1 ASCII law lifted: `class_fragment` lowers classes with
members above 0x7F to regex-syntax's canonical `Utf8Sequences` chains
(a multi-byte codepoint consumes ALL its bytes as ONE leaf match —
the walker's match_at shape), so the per-arm choice lane routes
bodies with negated Unicode classes. Riding with it: the exact-copy
arm (min==max unroll) + the tranche-7 envelope arm mirrored into
build_plan9, and THREE correctness fixes the battery forced:
`leaf_count` counts envelopes as zero slots (the mirror counted the
inner subtree — the executor's cut buffer desynced, the json-like
parity test's leaf-span underflow), the arm-pick scans the arm's
WHOLE slot range (an arm's paths can end at different slots — the
last-slot read missed fired arms), and the ScanClass yield at the
eligibility gate (the bulk scan keeps its shape; the dispatch_shape
gate caught the hijack).

**The envelope-tail overlap law** (the hexquad's structural catch):
when the envelope tail's bytes intersect the loop restart's bytes,
the subset construction merges the greedy-tail and loop-restart
interpretations into one single-path DFA state — the walker's
greedy-inner preference is not expressible — so the routed lane
refuses and the classic path keeps the body. JSON's exact interior
refuses on this law (the u-hexquad's hexclass overlaps the
plain-char restart); its pin is in the battery alongside the refused
body's classic-path exactness.

**Receipts** (interleaved min-of-15): json parity within noise
(17.6-18.3 vs 17.8-18.8 ms; histogram byte-identical at 628,324
dispatched); the multibyte loop-heavy escape/plain corpus parity
(25.3-27.8 vs 24.9-28.7 ms). The tranche that unlocks the json
interior is the ENVELOPE-TAIL DISAMBIGUATION: a priority-labeled
subset construction (tail edges win where the tail's bytes lead) or
a two-lane table — that is Phase 2's next design work, and with it
the ~60%-of-bytes interior + the LR fusion land together.

## The class-run envelope DESIGN (2026-11-23, the hexquad's unlock)

The overlap law's positive counterpart: for an envelope with
`min >= 1, max = None`, and a SINGLE-CLASS-LEAF body (the json
u-hexquad's `4*(hexclass)`), the walker's classic lowering gives ONE
span for the whole run (the Span-owning-rep law — observed:
`in:"0041b"`). The routed equivalent needs NO inner-cut lane and NO
ambiguity:

- NFA: `min-1` UNMARKED byte-edge copies of the class, then ONE RUN
  state — a self-loop on the class bytes carrying the element marker
  of the run's single slot — epsilon into the boundary. The boundary
  is unreachable before min bytes (structurally: only the run state
  reaches it).
- Plan: ONE Bare slot (leaf_count 1); the cut = the run's end. The
  assembly's span = `[prev, cut]` = the whole run — byte-identical
  to the classic's shape.
- Why the overlap dissolves: the run state's marker stays live
  across the merged states (run ∪ entry), while the restart arms
  carry markers only AFTER their bytes land — by which time the run
  state has exited (a non-class byte). The marker collision never
  forms; the table stays single-path. On a class byte the merged
  state updates the run's cut (the greedy tail); on any other byte
  the run state has no transition and the restart interprets — the
  walker's greedy-inner semantics, exactly.

Gate: min >= 1, max = None, the body = a single leaf (Str/Re —
class_fragment's chain form works for multi-byte members too), and
the class-run bytes still must not collide with... nothing: the law
above replaces the overlap refusal FOR THIS SHAPE ONLY; other
envelope shapes keep the refusal. The differential fixture: the
json-exact interior (hexquad included) routes and replays the greedy
tail (the `0041b` swallow case pins it).

**(Implementation note, same day)**: the class-run's first cut routed
the hexquad interior but the assembly read WRONG slots (cuts landed
on neighboring slots — the marker/assembly interplay needs a
minimal-probe trace: dump the DFA states' markers + the walk's cut
records for the 5-slot fixture). REVERTED uncommitted; the overlap
refusal remains the shipped state, the design above stands, and the
implementation is the next session's focused slice with a clean
probe budget.

## The class-run tranche MEASURED AND REVERTED (2026-11-23, the session's deepest cycle)

The complete class-run implementation (byte-driven exit, the gated
boundary law, the plan cache, the single-bare fast path) ROUTES the
json string interior — **dispatch −81.6%** (628,324 → 115,424, Regex
10.2% → 0.28%) — but the interleaved wall A/B measures **+33-40%
REGRESSION** (A 22.5-24 ms vs B 16-18 ms), and the control experiment
(the class-run disabled, everything else in) returns to parity — the
regression is entirely the routed interior's runtime, NOT the
scaffolding. REVERTED per the A/B law; the implementation is preserved
at /tmp/class-run-tranche.patch (698 lines).

**What the cycle established** (all recorded for the next session):

1. **The first design's fatal flaw** (an epsilon boundary) made the
   run states iteration-accepts — a mid-run crossing fired per byte
   and the arm-pick fired wrongly. THE FIX: the exit is a BYTE edge on
   the class's non-first bytes → the run states' closures exclude
   body_accept → crossings fire only on the exit byte, which lands
   in the accept state.
2. **The exit-byte re-consumption law**: the crossing boundary = the
   incremental max cut (`last_cut`, updated O(1) at the cut
   recording); when it is behind the position, the position rewinds
   so the accept state's restart edges re-consume the exit byte as
   the next iteration's first byte — the walker's greedy-inner
   semantics, exactly. The law MUST be gated per table
   (`mark_dfa_loop_boundary_law`): applied unconditionally it
   rewinds ENVELOPE-lane tables whose slotless elements don't
   advance the cuts — the arithmetic-tail test hung re-parsing
   forever (the session's SIGKILL/hang catch).
3. **The per-invocation plan resolve** (resolve_plan_tree9 +
   AsmNode allocations per exec_dfa_run call) showed up in the
   profile as malloc/free/drop_glue — fixed with a per-VM plan cache
   (resolve once per table, clone the small Vec per invocation).
4. **The interior table = 53 states** (54 KB dense) — the utf8
   chains + the hex run + the restart arms.
5. **The single-bare fast path** (the fired arm = one bare span →
   the value IS the whole-iteration span: a direct scan + push,
   no assembly) — barely moved the wall.

**THE SHARPENED PHASE-2 CONCLUSION**: the interior's routed per-char
iterations are VALUE NOISE — the walker emits one span per char, but
the json grammar's `string` capture consumes the WHOLE string, so the
per-char values collapse at the named level. The classic choice loop's
~18 ns/char dispatch+fold BEATS the DfaRun's table walk + per-char
value push (~25-45 ns/char). The winning shape is NOT routing the
interior loop: it is routing the WHOLE STRING (`"` interior `"` as ONE
table with ONE value = the whole-string span) — a string-level scan
(the slice-4 bulk-scan shape with escape-aware continuation). That is
the next design, and with it the ~60% of json bytes collapse into ONE
instruction with ONE value.

## The LR fusion DESIGN (2026-11-24, option B's program): the recursive-spine table

With the string interior blocked on the shape decision (the value-
noise finding above), the remaining structural lever is the LR
fusion proper: ONE automaton driving json's RECURSIVE spine —
value/member/object/array — the machinery a byte-driven DFA
architecturally lacks (recursion) and the dispatch loop pays for
today: ByteDispatch 17% (the value/member choice per VALUE) +
Call/Return 8.6% (the rule boundaries) + the object/member loops'
RepCount/Jump/PartialCommit share.

**The model** (the classic LR(1) table with a value stack):

- STATE: `u32`; the table row = 256 byte-transitions + ACTIONS.
- SHIFT: byte consumed, state = next, position advances — the DfaRun
  walk's inner loop, one instruction per BYTE REGION (not per char).
- CALL: a rule reference = push (return-state, capture-frame);
  ENTER the rule's start state. Recursion = the stack (object →
  member → value → object), bounded by the existing depth guard.
- REDUCE: a rule completion pops the call, produces the rule's value
  (the v2 named law: the Named rules produce `{name: inner}`; the
  anonymous ones produce spans/sequences per the existing shape
  laws — the SAME trees the classic emits, byte-identical).
- ACCEPT: the entry rule's completion.

**The wire-shape contract** (what makes this a PERF slice, not a
semantic one): the LR machine emits THE SAME trees as the classic
compile — the differential battery pins it (the corpus replay +
packrat_differential). No shape decision needed.

**The gates** (per rule graph): the fixpoint prover (slice 1)
accepts the grammar's recursive spine (json's is right-recursive —
the edition-2 note); the table build refuses unbounded ambiguity
(the LR conflicts = refuse, the classic keeps the rule).

**The build order** (slices, each A/B-gated):
1. The `LrTable` type + the builder: the LR(1) item sets over a
   rule graph the prover accepts; the json spine as the first
   target. New pub types on NEW registries (the additive law).
2. The `LrRun { table_idx }` instruction + the executor walk
   (state/stack/values), the reduce actions producing the v2 shapes.
3. The compiler seam: the ENTRY rule lowers to LrRun when the
   prover accepts the whole graph; the sub-rules the LR table
   covers become table-internal (their classic code dead-stripped).
4. The A/B: json min-of-15 interleaved vs 1.5.17. The win =
   ByteDispatch+Call/Return+the object/member loop scaffolding
   (~25-30% of dispatch) out of the loop; the strings/ws/numbers
   stay on their existing lanes (the interior classic until the
   shape decision, the ws loops already DfaRun-routed).

**The measured ceiling for option B** (before the shape decision):
ByteDispatch 17% + Call/Return 8.6% + the member/object loop share
(~8-10%) ≈ 25-35% of dispatch ≈ 0.8-1.1 dispatched/byte removed —
json ~0.037 → ~0.030-0.032 µs/B projected — AT the gate, not
through it; through the gate needs option A's shape.
