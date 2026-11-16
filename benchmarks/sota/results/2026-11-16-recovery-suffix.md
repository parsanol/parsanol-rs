# Receipt: the recovery suffix replay — 2026-11-16 (rs#539 layer 2)

Layer 1 left the suffix gap measured: the rounds recovered locally
around the failure (~8 iterations) but the clean suffix past the last
resync went unparsed — suffix 14.7% vs tree-sitter 70.6%. Layer 2
closes the gap.

## The changes

1. **The spine picks the grammar's list shape, not a char run.** The
   layer-1 predicate ("body drives a rule call") let `1*digit` inside
   `int` and a string's interior run qualify — both "match" at every
   offset, so the trial scan found them everywhere and the rounds
   stalled byte-by-byte (one 1-byte "iteration" per round). The
   predicate is now *anchors a fixed literal AND is not a char
   matcher*: a bare class run, or an alternative offering a bare class
   branch, is excluded; flat terminal lists (a `pair*` of `k=v;`
   items, csv fields) keep their spine — their literals anchor the
   scan.
2. **The walk is breadth-first.** The outermost list wins: a grammar
   nesting both an object member list and an array element list
   recovers at the member granularity, which also carries the
   enclosing object's separators. Depth-first picked the array list
   through `value`'s descent and recovered object members as
   key-string-only fragments.
3. **The probes run on a clean parser.** The journaling parser
   collects partial structure, which swallows inner failures — a probe
   there reported prefix "successes" (`,"key"` without `": value"`).
   The rounds now probe through a dedicated parser carrying the same
   analysis, parsing into the render arena so the recovered iterations'
   shaped values join the stash directly (the journaling parser gets
   its own arena; the pre-round stash items are deep-copied across —
   pool indexes are arena-local).
4. **The rounds replay the full suffix.** Each round strictly advances
   the cursor, so the budget scales with the input: 8 rounds + one per
   kilobyte. The 20-site corruption rows stay bounded and the clean
   remainder parses to the end.
5. **Span-aware scoring (both engines).** The runners emit `start:end`
   byte spans; the ground truth carries (start, end) pairs; a
   recovered span credits a ground-truth value span it covers within
   the corruption's length delta. Start-offset equality could never
   match — the member-list iterations start at the separator comma,
   not at the key. Clean full parses cover the whole suffix on both
   engines (previously scored 0%).

## Results (60-case harness, same corpus + seed, hashes in the manifest)

| metric | parsanol (layer 2) | tree-sitter | layer 1 |
|---|---|---|---|
| suffix recovery | **92.6%** | 95.6% | 14.7% |
| structure recall (span-aware) | 52.8% | 72.2% | 28.4% (offset-equality) |
| diagnostic distance | **±24,744** | ±56,349 | ±24,744 |

Per class (suffix / recall):

| class | depth | parsanol suffix | ts suffix | parsanol recall | ts recall |
|---|---|---|---|---|---|
| delete | 20 | **96.2%** | 94.2% | 76.1% | 99.3% |
| insert | 20 | 83.4% | 98.8% | 80.1% | 100% |
| mutate | 20 | 90.6% | 96.0% | 66.2% | 94.2% |
| unterminated | 20 | 74.3% | 73.5% | 80.3% | 84.0% |

- The suffix target from the mission (>70%) is met: **92.6%**, within
  3 points of tree-sitter, and delete-20 rows **win outright**
  (96.2% vs 94.2%).
- The residual recall gap is span granularity: tree-sitter's recovery
  tree keeps coarse non-error containers (its root `object` node often
  survives an error inside, covering every ground-truth span at once),
  while parsanol emits fine-grained per-iteration spans — one skipped
  iteration breaks that iteration's ground-truth containment. The
  windowed TED scorer (rs#527) is the honest next instrument; coarse
  containers win any containment metric.

## Provenance

- Harness: `benchmarks/sota/` — corrupted corpus + manifest
  (`corpora/json-corrupted/manifest.json`, seed-pinned), runners in
  `runners/parsanol` (the artifact now baked from pubid-grammar main's
  `json.parg` with the declared `resync value`/`resync member`) and
  `runners/ts-rust` (tree-sitter Rust binding, JSON grammar).
- The declared per-rule resync resolution (spine rule → `resyncs`
  envelope member) is engineered in the suite but parked on the wire:
  a rule→index table is order-dependent across compilers (both emit
  duplicate Entity copies per rule; first-encounter disagrees).
  Parked with the analysis on parsanol-ruby `mirror/envelope-members`;
  the entry-level root alias and the inferred scan serve the wire
  today.

## Suite

- `parsanol/tests/recovery_spine_layer2.rs` — four specs: the digit
  hijack, whole-member honesty, the declared-resync artifact path, and
  the clean-suffix contract.
- Full crate suite green except the pre-existing sibling-checkout
  corpus seal (owner re-bake pending; skips are loud, not silent).
