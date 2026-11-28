# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.20.22] - 2026-11-15

### Performance

- Eval loop: cut the per-parse fixed cost on tiny inputs (#492) —
  `DenseCache::for_input` sizes to the memo surface, lazy CaptureState,
  lazy items vecs, lazy bulk-scan failure labels; 338 -> 231 allocations
  on a 4-byte parse, -32% end-to-end on the shared parse legs.
- Hash-shadow dedup on the deepest failure labels (rs#498 round 1):
  the probe is a hash lookup with linear verification on hash hits —
  byte-identical buffers.

## [0.20.21] - 2026-11-14

### Fixed

- Bytecode VM: the prefix-split choice fail-jump landed in the rule
  epilogue instead of the next alternative (#500, rs#496) — the VM
  silently short-parsed shared-prefix grammars (expressir's syntax.exp
  parsed Ok@10 instead of 21028).

## [0.20.20] - 2026-11-14

### Added

- PreparedGrammar: the analyses computed once for the parse-many
  consumer pattern (rs#486/#495).
- Checksum refusals name the compiler (rs#494/#497):
  ChecksumCompilerMismatch carries the stored/computed checksums and
  the producing compiler.

## [0.20.19] - 2026-11-13

### Added

- Constrained-decoding export: the Rust-authoritative lowering
  (ruby#493), exposed on the wasm and C-ABI tiers.

## [0.18.8] - 2026-10-16

### Fixed

- Return capture depth errors without panicking, preserving parser reuse
  after a depth limit is reached ([#239](https://github.com/parsanol/parsanol-rs/pull/239)).
- Classify grammar-layer conformance failures in the capture phase
  ([#241](https://github.com/parsanol/parsanol-rs/pull/241),
  [#242](https://github.com/parsanol/parsanol-rs/pull/242)).

### Performance

- Avoid reading the debug environment below the parse depth threshold
  ([#238](https://github.com/parsanol/parsanol-rs/pull/238)).

## [0.15.0] - 2026-10-07

### Added

- Constant and Lookbehind wire atoms (rs#137 follow-up): constant-yield
  and byte-window-lookbehind atoms, making coradoc-markdown Output /
  precedes? shapes natively serializable. New exhaustive-enum variants
  are semver-minor on the 0.x line (the 0.13.0 precedent).

## [0.16.0] - 2026-10-07

### Added

- Atom-valued capture writes across the dynamic bridge
  (parsanol-ruby#160): `CaptureValue::Opaque` handles and the typed
  `WriteValue` channel let the open_block/continuation chain pattern
  store parser expressions in captures natively. New exhaustive-enum
  variants are semver-minor on the 0.x line (the 0.13.0 precedent).

## [0.17.0] - 2026-10-07

### Added

- LookSpec for Atom::Lookbehind: Literal (the fixed byte window) or
  Regex (end-anchored at the position) - the CommonMark flanking
  form (rs#163 gap 1). The wire shape change rejects loudly on
  0.15/0.16.

### Fixed

- The dynamic-bridge writeback interns every non-string host value -
  the lazy-continuation :block marker no longer drops natively
  (parsanol-ruby#160).

New exhaustive-enum variants are semver-minor on the 0.x line (the
0.13.0 precedent).

## [0.17.1] - 2026-10-07

### Fixed

- Capture iteration uses map keys, not the undo log (rs#165):
  fragment-seeded capture states exposed nothing through the dynamic
  bridge — context materialization omitted every capture and
  capture_signature hashed every fragment temp as empty. The
  coradoc lazy-continuation divergence (7 kramdown examples) was
  exactly this; the full markdown suite (637 examples) and the
  engine differential (13/13) parse tree-identically natively now.

## [Unreleased]

- The spine is derived once per session, not per keystroke ([#740](https://github.com/parsanol/parsanol-rs/pull/740))
- The retention-band diet: per-keystroke cache maintenance becomes O(fresh inserts) (SOTA-2028/1) ([#736](https://github.com/parsanol/parsanol-rs/pull/736))
- P3(a)'s final step, owner-authorized: the shift-preference construction IS the default. The PARG_LR_SHIFT gate is gone — lr_table_for builds shift-preference unconditionally (a per-rule refusal still falls back to the classic lowering; the strict builder remains for the survey). The ab_shift harness is deleted (its off/on premise is dead); examples/bake_flavor.rs carries the flavor A/B forward.
- Move the v1-suffix probe out of the match scrutinee (clippy: complex block in scrutinee)
- The memo-warm interior: keep the above-edit container entries in the shift retention — doc0 p95 56ms -> 619us, reused 3.4x
- The count-based compaction backstop: reclaim unconditionally at 16x live — the CI-debug deferral the regression spec caught
- The pool dead-node reclamation: compact the session's output arena behind a payback-gated trigger — doc0 pool 405k -> ~9k, shift tail 310us -> ~9us, identity 0
- Drop the orphaned doc-comment fragments left by the copy_shift_tree deletion (clippy: empty lines after doc comment)
- The lazy-span-shift graft: the arena shift InputRefs in place and the splice re-references suffix handles — doc3 p50 5.1x, doc0 -32%, identity 0
- The probe-phase correction: nav is 17us; the parse remainder scales 5.1x with the edit offset — the suffix re-key is the scoped lever
- P3(b) falsified: the greedy-direction tables misparse valid json; the seam keeps strict->shift only and the sizing stays analysis-side
- The splice-diet construction lever: the session computes the analysis once ([#712](https://github.com/parsanol/parsanol-rs/pull/712))
- The per-flavor sizing: sort_by_key satisfies clippy
- Item 6 closes: the wide-kernel verification receipt (the asm inspection + the 13.8x A/B); the index rows for 6/11/12 settle
- Item 11's Scan remainder: the Entity-wrapped class loops take the bulk run; the Named refusal is the gate
- The P3(a) A/B receipt: the ieee flavor runs the shift arm byte-identical and non-regressing; the sizing is per-flavor
- The final loop-site census: both-lane coverage with every refusal's named reason; the 38 decompose into the zero-width, P3(b)/(c), and depth classes
- The LR survey sizes P3(a): 499 refused rules build under shift-preference; the seam is wired behind PARG_LR_SHIFT
- The census carries the LR lane: the survey counts lr-covered sites, and tranche 10 converges with P3(a)
- The survey diag: drop the unused atoms binding (CI denies warnings)
- The survey diag: rustfmt
- The routing tail documented: three refusal classes with reasons and the next tranche scoped
- The survey diag prints the first-refused element of every unrouted site
- The splice fixed-overhead profile: the phase split answers the 446us demand
- The #102 byte-tier spec: rustfmt rewrap (the CI lint's preferred layout)
- The depth-leak pins: rustfmt import order
- Parsanol-ts#103 regression 2: the folded dispatch's ? sites skip the tail bookkeeping
- Parsanol-ts#103 regression 1: Arc the string-span spec so the typed depth guard wins
- The splice instrumentation: the gate, the engagement, the phase timers
- The cross-language spec: the reordered envelope loads via the byte tier
- #102's engine fix: the byte-exact tier replaces the canonical when verified
- The first baseline measured the registry 1.5.1 engine
- The snapshot-recycle invariant spec: the exemption is the boundary
- The named-choice tranche: Named(choice) rides the tranche-9 lane
- The plan9 refusal hooks: the per-site trace completes the map
- The three-level strictness: the shift-preference seam, A/B-gated
- The P2 specs-first opening: the edit-sequence differential pin
- The census mirror unwraps Entity/Named transitively
- De-drift the routing census mirror; add the survey diagnostics
- The named-dispatch frame diet: fold try_atom_impl_inner into its wrapper
- The LR seam propagates lowering refusals; the construction refuses
- The special-lane precedence: the quoted-string idiom owns its rules
- Rs#574 LR slice 3b: the seam flips LIVE — the fleet proof delivered
- The classic parser caches its internd tag indexes — the profile's lever
- Rs#574 LR slice 3: the compiler seam — the strict construction, the arena wiring, and the inert-until-proven gate
- Rs#574 LR slice 2: the LrRun executor — byte-identical trees to the walker on the recursive spine
- The changelog line spells the helper rename without the trip word
- The LR construction's symbol interning goes uniform: NonTerminal symbols returned nt_count+nt WITHOUT interning, so the GOTO edge's symbols[id] indexed out of bounds — the two spine tests (recursive + left-recursive) were #[ignore]-documented on a wrong lowering-desync theory; both build now
- The StringSpan variant moves to the enum tail: the implicit discriminants of every later Instruction variant shifted (Custom 30->31, ..., ByteDispatch 45->46) when the variant landed mid-enum — the numeric as-isize surface must not move
- The test helper renames to post_literals (the typos gate)
- Rs#574 option A: the string-scan shape — one whole-string span on the wire
- Rs#574 the LR fusion slice 1: the LrTable type + the canonical LR(1) builder (WIP, the recursion-lowering count desync documented) ([#663](https://github.com/parsanol/parsanol-rs/pull/663))
- The string-shape probe (the value-noise finding's evidence tool) ([#658](https://github.com/parsanol/parsanol-rs/pull/658))
- Rs#574 multi-byte class slice: UTF-8 sequence chains route composite choice bodies ([#653](https://github.com/parsanol/parsanol-rs/pull/653))
- Rs#574 stage 3 tranche 9: per-arm choice slots route composite choice bodies ([#652](https://github.com/parsanol/parsanol-rs/pull/652))
- Phase 2: the instruction histogram and the revised slice plan ([#647](https://github.com/parsanol/parsanol-rs/pull/647))
- Phase 2 opened: the measured state, the profile, and the arena pool resize ([#643](https://github.com/parsanol/parsanol-rs/pull/643))
- Slice B stage 8: choice-body loops routed (the envelope-exit catch) ([#641](https://github.com/parsanol/parsanol-rs/pull/641))
- Release v1.5.13
- Validate JavaScript budgets before integer conversion
- Expose a bounded per-call depth budget for WASM artifact parsing
- Validate JavaScript budgets before integer conversion
- Expose a bounded per-call depth budget for WASM artifact parsing
- The mask FFI: the constrained-decoding surface crosses the C ABI (rs#530)
- Re-pin the export-parity fixture to the re-baked corpus
- Cargo fmt
- Phase 0 shipped: the memo hasher + the pooled tree writes (rs#574)
- The ruby-ext transform depth cap rises to the stack budget (ruby#535) ([#593](https://github.com/parsanol/parsanol-rs/pull/593))
- The dispatch depth gate falls: 0.079 to 0.0599 us/B on json (-20.6%) ([#586](https://github.com/parsanol/parsanol-rs/pull/586))
- The determinism prover: per-rule exact FIRST sets at the least fixpoint (rs#574 slice 1) ([#585](https://github.com/parsanol/parsanol-rs/pull/585))
- The recovery spine replays the unchanged prefix — the TED gap halves ([#580](https://github.com/parsanol/parsanol-rs/pull/580))
- The mask emitter: the llguidance wire, the sampler-gated loops, and the relaxed recovery ([#579](https://github.com/parsanol/parsanol-rs/pull/579))
- The README: the language server and the constrained decoding
- Release v1.5.7
- Release v1.5.7
- The CI gates: the reserved FFI fn allowed, the unused workspace deps dropped
- The constrained-decoding oracle: the token-mask sets + the state tracker (rs#530 engine side)
- The ASCII guard binds only the range end
- The scan instructions stop taxing the hot dispatch: guard + out-of-line dispatch
- The CI gates: the reserved FFI fn allowed, the unused workspace deps dropped
- The constrained-decoding oracle: the token-mask sets + the state tracker (rs#530 engine side)
- The ASCII guard binds only the range end
- The scan instructions stop taxing the hot dispatch: guard + out-of-line dispatch
- The ScanUntil instruction: fused negated-delimiter runs
- The ScanClass instruction: bulk exact class runs for repetition bodies
- Drop Spine.rep and let rustfmt settle the test-module import
- The multi-level spine splice: bridge at the nested list containing the edit
- Align the crate metadata with the discoverability pass (keywords + categories)
- Release v1.5.4
- Resolve the changelog reword conflict (keep the current spelling)
- CI hygiene on the window commits: the spell-check reword, identity-op
- Borrow arena pool items element-wise (array_item, hash_item)
- Bound the memo to a working window: the recycle keeps the newest half
- Release v1.5.4
- The persisted chain: bounds without a per-keystroke drain (rs#542 slice 2b)
- Close the entries field (no consumer remains)
- The anchor-byte drain scan + the visibility the splice substrate needs
- The path splice engages: the tag-slot tree model (rs#542 slice 2b)
- Rename the nav siblings field so the typos gate passes
- The path-splice execution scaffold (rs#542 slice 2b) + the atom-to-tree finding
- The remaining substrate lints
- Silence the substrate's dead-code lint pending the splice consumer
- The spine module: the shared list-structure discovery with its structural path
- The post-parse window: fresh suffix entries survive (rs#542 slice 2a)
- The suffix shift: memo-window retention across scattered edits (rs#542 slice 1)
- Fix the parked-wire spec and the changelog spell gate
- The recovery suffix replay: 14.7% to 92.6% suffix (rs#539 layer 2)
- Merge pull request #546 from parsanol/fix/legacy-source-identity-current-sdk-20261116
- Add explicit deepest failure reporting without changing defaults
- Preserve source rule identity for Parslet compatibility
- The persisted chain: bounds without a per-keystroke drain (rs#542 slice 2b)
- Close the entries field (no consumer remains)
- The anchor-byte drain scan + the visibility the splice substrate needs
- The path splice engages: the tag-slot tree model (rs#542 slice 2b)
- Rename the nav siblings field so the typos gate passes
- The path-splice execution scaffold (rs#542 slice 2b) + the atom-to-tree finding
- The remaining substrate lints
- Silence the substrate's dead-code lint pending the splice consumer
- The spine module: the shared list-structure discovery with its structural path
- The post-parse window: fresh suffix entries survive (rs#542 slice 2a)
- The suffix shift: memo-window retention across scattered edits (rs#542 slice 1)
- Fix the parked-wire spec and the changelog spell gate
- The recovery suffix replay: 14.7% to 92.6% suffix (rs#539 layer 2)
- Merge pull request #546 from parsanol/fix/legacy-source-identity-current-sdk-20261116
- Add explicit deepest failure reporting without changing defaults
- Preserve source rule identity for Parslet compatibility
- Span the ws shape: merged-choice bodies take the vectorized run
- Reword the rejection input so the spell check passes (the literal is spelled with a space; still not ws-adjacent-to-a)
- Single-byte choice merge: the ws shape compiles to one charset
- Relocate the lead-byte helper above the test module (clippy items_after_test_module)
- Determinism dispatch round: singleton-class decode, folded-literal first sets, the repetition cost gate (rs#533 TODO.perf/11)
- VM-local regex predecode + the exact deferred-bookkeeping spec
- Keep the public API surface: deferred buffer on the VM, tracker untouched
- Rustfmt
- VM hot-path round: deferred diagnostics, periodic timeout, Arc regex caches
- Release v1.5.0
- Align the internal parsanol-derive requirement to 1.5.0
- Release v0.20.23
- Box the pending frame payload: keep the recursion stack footprint
- Pending diagnostics: the rs#498 v2 surface with hash-first label storage
- Release v0.20.23 ([#510](https://github.com/parsanol/parsanol-rs/pull/510))
- The pure-class regex fast path and the clone-free fetch (rs#507) ([#512](https://github.com/parsanol/parsanol-rs/pull/512))
- Strip the @[object] attributions from the changelog ([#513](https://github.com/parsanol/parsanol-rs/pull/513))
- Fix the changelog template and curate the 0.20.x sections ([#511](https://github.com/parsanol/parsanol-rs/pull/511))
- Release v0.20.22
- Strip the @[object] attributions from the changelog ([#513](https://github.com/parsanol/parsanol-rs/pull/513))
- Fix the changelog template and curate the 0.20.x sections ([#511](https://github.com/parsanol/parsanol-rs/pull/511))
- Release v0.20.22
### Other

- Release v0.20.22 ([#506](https://github.com/parsanol/parsanol-rs/pull/506))
- Hash-shadow dedup on the deepest failure labels (rs#498 round 1) ([#507](https://github.com/parsanol/parsanol-rs/pull/507))
- Eval loop: cut the per-parse fixed cost (setup/eval on tiny inputs) ([#492](https://github.com/parsanol/parsanol-rs/pull/492))
- Release v0.20.21 ([#504](https://github.com/parsanol/parsanol-rs/pull/504))

### Other

- Hash-shadow dedup on the deepest failure labels (rs#498 round 1) ([#507](https://github.com/parsanol/parsanol-rs/pull/507))
- Eval loop: cut the per-parse fixed cost (setup/eval on tiny inputs) ([#492](https://github.com/parsanol/parsanol-rs/pull/492))
- Release v0.20.21 ([#504](https://github.com/parsanol/parsanol-rs/pull/504))

### Other

- Release v0.20.21 ([#502](https://github.com/parsanol/parsanol-rs/pull/502))
- The express-grammar repro gates (the VM short-parse, packaged) ([#500](https://github.com/parsanol/parsanol-rs/pull/500))

### Other

- The express-grammar repro gates (the VM short-parse, packaged) ([#500](https://github.com/parsanol/parsanol-rs/pull/500))

### Other

- The producer-naming refusal as a separate variant
- Checksum refusals name the compiler and document the contract (rs#494)
- The analyses computed once for the parse-many pattern (rs#486 residual)

### Other

- Rs#459 slice 2: the segment->generator lowering, exposed on every tier
- Cache the per-parse fixed work: decoded entries and grammar analyses (rs#486)
- Cache the per-parse fixed work: decoded entries and grammar analyses (rs#486)
- :negate: iterator form (stable 1.100 clippy needless_range_loop)
- The iso render expectation follows the #116 spec fix
- Iterator-style union/intersect (the CI clippy's needless_range_loop)

### Other

- Shrink_to_entries after retained retention (rs#464 round 2, TODO.perf/9)

### Other

- Scheduled heavy fuzzing: the weekly hours-long gate (rs#466)

### Other

- Constrained-decoding export: the Rust-authoritative lowering (ruby#493)
- Jit#458 closed NEGATIVE: the dispatch-mix harness + the recorded mechanism ([#462](https://github.com/parsanol/parsanol-rs/pull/462))
- Jit#458 phase 1: the dispatch-mix profile harness ([#460](https://github.com/parsanol/parsanol-rs/pull/460))

### Other

- Support portable ASCII-insensitive state matching in PARG ([#449](https://github.com/parsanol/parsanol-rs/pull/449))

### Other

- Preprocess vocabulary v1.1: const_map, default_when_absent, rename (rs#147) ([#445](https://github.com/parsanol/parsanol-rs/pull/445))

### Other

- Expose explicit artifact compatibility selection in C and WASM ([#440](https://github.com/parsanol/parsanol-rs/pull/440))

### Other

- Drop the now-unused has_entity (wasm clippy dead-code)
- Restore the injected-skip interleave (ruby#425)

### Other

- Json-only member cases bind no source pair (ruby#420) ([#428](https://github.com/parsanol/parsanol-rs/pull/428))

### Other

- Publish the engine-compat manifest (ruby#421) ([#423](https://github.com/parsanol/parsanol-rs/pull/423))

### Other

- Drop the needless mut on the parsed grammar document
- Keep token spellings contiguous — no trivia inside a token

### Other

- Revert "Merge pull request #410 from parsanol/feat/540-stage-replay"
- Revert "Merge pull request #412 from parsanol/feat/540-stage-replay-languages"
- Find the changed-payload probe's slice at any byte offset
- Replay the asciimath and unicode stage generations on the Rust engine

### Other

- Read stage payloads as JSON instead of adding a YAML crate
- Replay staged testsuite raw-tree rows on the Rust engine
- Pin anchored matching of case-insensitive literals ([#409](https://github.com/parsanol/parsanol-rs/pull/409))
- Evaluate pure terminals directly in Strict mode (stacked on #406) ([#408](https://github.com/parsanol/parsanol-rs/pull/408))
- Record terminal failures without per-failure allocations ([#406](https://github.com/parsanol/parsanol-rs/pull/406))

### Fixed

- Match regex atoms anchored at the position ([#403](https://github.com/parsanol/parsanol-rs/pull/403))

### Other

- Exclude model_bindings from the hash (ruby#447 mirror) ([#400](https://github.com/parsanol/parsanol-rs/pull/400))

### Other

- Exclude byte_checksum from the canonical payload (rs#397) ([#398](https://github.com/parsanol/parsanol-rs/pull/398))
- Release v0.20.3

### Other

- Release v0.20.3
- The byte-checksum tier + the canonical exclusion
- Share cached legacy diagnostic snapshots
- Move legacy diagnostics into empty application frames
- Batch legacy failure-label merges without changing diagnostic order

### Other

- The byte-checksum tier + the canonical exclusion
- Share cached legacy diagnostic snapshots
- Move legacy diagnostics into empty application frames
- Batch legacy failure-label merges without changing diagnostic order

### Other

- Keep merge's 2-arg signature; merge_with_atomic threads exemptions
- Merge_sources test asserts prefixed rule docs
- The atomic exemption qualifies with its rules
- Qualified references survive the prefix
- Merge_sources — resolve use from an in-memory source map

### Other

- Drop the unnecessary None-substituting closure
- The declared per-target binding artifact (#432, rs side)

### Other

- Kill the repetition-root fallback (ruby#408/#412 mirror)

### Other

- Slice the byte view directly in the multibyte span test
- Pin whole-codepoint capture spans at the lutaml-path shape

### Other

- Merge pull request #379 from parsanol/feat/capture-flatten
- Edition 2 wraps only non-captured structure (ruby#400)

### Other

- Alt from_table parses in the brace lane; the lint gate goes sibling-aware (rs#376) ([#377](https://github.com/parsanol/parsanol-rs/pull/377))
- Release v0.19.17

### Other

- Release v0.19.17 ([#373](https://github.com/parsanol/parsanol-rs/pull/373))
- A zero-instance array binds nothing (rs#370) ([#371](https://github.com/parsanol/parsanol-rs/pull/371))
- Remove the mode grammar-header keyword (owner ruling 2026-10-31)

### Other

- A zero-instance array binds nothing (rs#370) ([#371](https://github.com/parsanol/parsanol-rs/pull/371))
- Remove the mode grammar-header keyword (owner ruling 2026-10-31)

### Other

- Merge pull request #368 from parsanol/corpus/rebake-semantics-hash
- Re-bake the conformance corpus at parsanol 1.3.117; pin the cross-compiler semantics hash
- (any) reduces to the plain cross-engine shape (ruby#383); wasm semanticsHash reader (rs#366)

### Other

- Expose the semantics hash; pin the re-bake stability (ruby#377)

### Other

- Pair array captures by instance, not occurrence (ruby#381)
- `as` after a guard binds the guard (ruby#380) + the semantics hash (ruby#377)

### Other

- The remap lands under the bucketed instrument
- Remap slot indexes instead of re-hashing on retention (TODO.perf/9)
- Retained bench: per-edit-position latency buckets

### Other

- Keep capture snapshots out of recursive parser results
- Revert "Keep legacy diagnostics out of strict recursive stack frames"
- Keep legacy diagnostics out of strict recursive stack frames
- Retain failure diagnostics across dynamic fragment parsing
- Preserve selected legacy failures and repetition memo outcomes
- Merge current main into the compatibility prototype
- Add opt-in Parslet prefix memo compatibility
- Interleave the retained-bench tiers; record the scan-fusion negative (TODO.perf/9)

### Other

- Null captures bind nothing (ts#34)
- Release v0.19.10

### Other

- Release v0.19.10
- Reapply capture writes when packrat branches are retried

### Other

- Reapply capture writes when packrat branches are retried

### Other

- Drop the dead entry lookup from the delegating compile
- Thread the glue names additively (the semver gate)
- Keep the glue field crate-internal
- The glue declaration: author-declared wrap exemptions (the rs mirror)

### Other

- Numeric capture editions + whitespace-class glue (the rs mirror)
- Compile mutually recursive grammars: the wire walker mirrors the serializer's identity memo (rs#334)

### Other

- Fmt
- The pre-0.19.7 emission names ride as deprecated shims (the semver gate)
- The probe's fixture artifacts (the tests reference them)
- The envelope field is edition; the header keyword is edition (mode = the deprecated alias); language_version removed
- The vm probe rides; the envelope rename completes on the rs side
- The default capture mode is v1; v2 is opt-in (the owner's reversal)
- The rs#334 rep-prefix repro (rs main rejects both shapes)

### Other

- Fmt
- The emission stamp: envelopes declare the capture-emission level (ruby#353)
- Whitespace-glue rules are exempt from the v2 wrap (ruby#352)

### Other

- Wire columns are character counts, not bytes (Slice parity)

### Other

- The v1 alias: YAML-style legacy spelling; the corpus re-baked on the stamped ruby
- The v2 default: the header, the wrap, the stamps, the pinned fixtures
- The U3 provenance stamp: unicode_tables rides the envelope (rs#297)

### Other

- The mode rides beside the parse output, not on ParsedSource (semver)
- The classic-compiler wrapper was dead under feature combinations
- Modern mode: the header, the wrap pass, and the envelope field (ruby#304)

### Other

- Restore the PargArtifactJs docs my insertion displaced
- The wasm compiler: compileSource (site#19 seam)
- Imports normalize CRLF at the read boundary (docs bake LF)
- Normalize CRLF fixture reads (the envelope compares LF contracts)
- Flavor-source parse coverage: 47/47 (rs#298 closeout)
- Publish the recovery-quality metric (rs#304)
- Resync scans from the spine boundary, not the failure offset
- Keep the slice-4b API surface (semver), merge returns the docs map
- PARG frontend slice 4c: lint output + envelope parity (rs#298)
- Keep atomic rules out of the public Document (semver-clean)
- Cargo fmt
- PARG frontend slice 4b: trivia injection, leftrec tiers, imports (rs#298)

### Other

- PARG frontend slice 4a: the compiler, atom-stream parity with Ruby (rs#298)

### Other

- PARG frontend slice 3: the full corpus parses, 35/35 documents identical (rs#298)
- PARG frontend slice 2: the parser, document parity with Ruby (rs#298)
- PARG frontend slice 1: the Rust lexer, token-parity with Ruby (rs#298)
- Rename the init squatter, document the Node pattern (rs#308)
- Recovery precision: node-level matching replaces the name proxy (rs#304)
- Recovery quality harness: seeded, deterministic metrics (rs#304)

### Other

- Unicode-native U1: codepoint-exact any-repetition runs; VM backend consumes all

### Other

- Release v0.18.19

### Other

- Release v0.18.19
- C ABI: recovery export, artifact-first dispatch, and the parsanol.h header for C/C++

### Other

- C ABI: recovery export, artifact-first dispatch, and the parsanol.h header for C/C++

### Other

- Reference mode — the plain relation in the existing walker

### Other

- Declared resync — the baked pattern owns the spans (TODO.sota/1 R2) ([#283](https://github.com/parsanol/parsanol-rs/pull/283))

### Other

- Cold parse: kill the capture-state quadratic (rs#270) ([#279](https://github.com/parsanol/parsanol-rs/pull/279))
- Inferred resync (TODO.sota/1 R2, no grammar addition) ([#280](https://github.com/parsanol/parsanol-rs/pull/280))

### Other

- Corpus + fix: the R1.5 partial differential input (recovery_kv) ([#277](https://github.com/parsanol/parsanol-rs/pull/277))

### Other

- Partial trees in the envelope (TODO.sota/1 R1.5) ([#274](https://github.com/parsanol/parsanol-rs/pull/274))

### Other

- Source the splice chain from the repetition trace (rs#258) ([#271](https://github.com/parsanol/parsanol-rs/pull/271))

### Other

- Float canonical form: the Ruby JSON number text (B6) ([#267](https://github.com/parsanol/parsanol-rs/pull/267))
- Shape test phase + repetition flatten parity

### Other

- Optional nil capture, array bindings, boolean cast
- Merge pull request #261 from parsanol/fix/resource-abort-diagnostics
- Artifact errors: report resource aborts instead of phantom syntax
- Merge pull request #257 from parsanol/test/incremental-multiregion
- Keystroke probe: report splice engagement and retained arena
- Multi-region differential gate and keystroke probe

### Other

- Packrat cache: document the memory bound, stop slot-table doubling
- Expose parseShapeRecovering (TODO.sota/1)

### Other

- Char classes: byte-string literal in the escape-decoding test (clippy)
- Stop a repetition after an iteration that consumes nothing
- Ruby bridge: VM failures are reported by the walker
- Ruby bridge: a VM parse that stops short is the walker's to decide
- Program cache: key on the engine and host versions
- Decode character classes through CharRanges
- Recovery R1: restore failure_wire's doc comment
- Recovery R1: parse_shape_recovering — failure forensics envelope (TODO.sota/1)

### Other

- Conformance corpus: cross-engine behavioral gate (parsanol-tree/v2)

### Other

- Update proptest requirement

### Other

- Release v0.18.4

### Other

- Release v0.18.4
- Pin the wire-atom vocabulary as a contract test

### Other

- Pin the wire-atom vocabulary as a contract test

### Other

- Pin proptest to =1.10.0 (MSRV)
- Corpus test: skip stale sibling checkouts with guidance

### Other

- Fix test name typo flagged by typos
- Trivia channel: snapshot/restore, drain-scoped memo, positioned units

### Other

- Release v0.18.0

### Other

- TriviaCapture whitespace kind: record marker-less units (ruby#180)

### Other

- Evaluate PARG runtime-state atoms natively (parsanol-ruby#129/#162)

### Other

- GC-root the host atom registry (ruby#177)
- Drop the exploratory note trace; satisfy the format gate
- Track the deepest failure position through trivia (rs#171)

### Other

- Satisfy the lint gate: mem::take over drain
- Trivia capture channel and Atom::TriviaCapture (rs#172/#152)

### Other

- Satisfy the lint gates: fmt, dead bindings, orphaned doc line
- Restore the names() doc and document the undo-log semantics
- Fragment boundary seeds visible capture state (rs#174)

### Other

- Signature-keyed dynamic outcome cache
- Release v0.13.0

### Other

- Version 0.13.0: the Trivia atom variant is semver-minor for a 0.x line
- Trivia atom variant (rs#172 diagnostics parity)

### Other

- Replay dynamic dispatch outcomes

### Other

- Remove leftover diagnostic abort: 60M atom-application cap kills legitimate workloads
- Outcome field is module-private (pub field = breaking); include DenseCache sizing
- Native engine: memoize dynamic-dependent atoms and dynamic outcomes
- Skip parity fixture: regenerated with atomic token rule
- Skip-trivia parity gate: replay the Ruby skip suite through the engine
- Release v0.11.0

### Other

- Release v0.11.0
- Cargo fmt the state-atom round
- Runtime-state and custom-ref atom variants ([#129](https://github.com/parsanol/parsanol-rs/pull/129))

### Other

- Cargo fmt the state-atom round
- Runtime-state and custom-ref atom variants ([#129](https://github.com/parsanol/parsanol-rs/pull/129))

### Other

- Restore PortableParser's doc comment displaced by NegScanTerm
- Fused maximal-run scan for negated-lookahead repetition bodies

### Other

- Cargo fmt
- Fast path for general character-class Re atoms

### Fixed

- Green the CI gates for the pg-artifact runtime

### Other

- Skip artifact-gated tests when inputs are absent
- V0.9.0
- F4 corpus gate + three Ruby-parity leaf fixes the gate caught
- F6 v1: derive evaluator with cross-engine parity test
- Parsanol_pg_parse / parsanol_pg_error / parsanol_pg_free
- Ranked multi-error reporting in the portable VM
- F6 v1: render specs in the envelope, generic renderers in all engines
- Structured parse error wire on artifact APIs
- VM progress guards, C10 shape check, default_entry, containerized tests
- Ship cdylib crate-type for wasm-pack builds
- Add pg schema + suite runner (C8) and wasm surface (C9)
- Add pg bindings runtime (C7): capture collection, preprocess, casts
- WasmParser.fromArtifact — PG artifact consumption with checksum verification
- Fix formatting and Windows CRLF in the PG artifact test
- Add pg module: PG artifact envelopes (load, verify, extract)

### Added

- Prefix-tree splice v1 for repetition-spine documents (TODO.perf/9)

### Fixed

- Keep try_atom's 2-arg public signature (semver); try_atom_impl is the consume_all variant
- Clear splice graft state when the session arena resets

### Other

- Thread consume_all through the native walker (root incomplete-consumption)

### Added

- Npm publish workflow for @parsanol/wasm

### Other

- Cargo fmt
- Record fresh dispatch profile — JIT revisit condition not met (TODO.perf/3)
- Incremental retained-tree foundation; two cache-retention bug fixes (TODO.perf/9)

### Other

- Release v0.8.9

### Other

- Magnus 0.9 from crates.io — the git patch is no longer needed

### Fixed

- LruCache::get needs &mut (promotes to front) — ruby-feature build

### Other

- Bound the compiled-program cache — dynamic fragments minted fresh hashes per parse (#93, #84)

### Added

- Binary-safe parsanol_c_parse_len — explicit-length input

### Other

- Release v0.8.7

### Added

- Binary-safe parsanol_c_parse_len — explicit-length input

### Other

- Update Cargo.toml dependencies

### Added

- Compile-once program, parse_into arena ownership; hotspot warning re-land ([#115](https://github.com/parsanol/parsanol-rs/pull/115))

### Other

- Selective rule memoization for dynamic programs (#100, #90) ([#113](https://github.com/parsanol/parsanol-rs/pull/113))

### Fixed

- Capture atoms expose parsed trees to dynamic blocks (coradoc nested-block parity) ([#110](https://github.com/parsanol/parsanol-rs/pull/110))
- List-pattern repetition collapses correctly with named separators

### Fixed

- Tree parity ([#83](https://github.com/parsanol/parsanol-rs/pull/83)), EOF scan ([#106](https://github.com/parsanol/parsanol-rs/pull/106)), dispatch-cache memory ([#84](https://github.com/parsanol/parsanol-rs/pull/84)) ([#107](https://github.com/parsanol/parsanol-rs/pull/107))

### Other

- Merge pull request #103 from parsanol/release-plz-2026-09-21T04-34-13Z
- Drop the memchr dependency (unused since simd.rs removal)
- Per-atom/per-opcode dispatch counters; raw-tree API docs; dead simd module removed (parsanol-rs#100)

### Added

- Capture writes across the bridge, dispatch caching, dynamic incremental sessions (parsanol-ruby#80)

### Fixed

- Unwrap the [atom, captures] pair before fragment resolution

### Other

- Release v0.8.0
- Capture-state example uses the span accessor (doc-test)
- Rustfmt

### Added

- Capture writes across the bridge, dispatch caching, dynamic incremental sessions (parsanol-ruby#80)

### Fixed

- Unwrap the [atom, captures] pair before fragment resolution

### Other

- Capture-state example uses the span accessor (doc-test)
- Rustfmt

### Fixed

- Qualify the AVX2 kernel through its module (x86 build)

### Other

- Scan module header describes the wide-window kernels
- Runnable latency gate + two retention fixes (TODO.perf/8)
- 32-byte window kernels (TODO.perf/6)

### Added

- Incremental parsing sessions through the Ruby bridge (TODO.perf/4)

### Fixed

- X86 kernel loop shape, private doc links, typos (CI)
- Drop edit-invalidated entries before the parse, not after
- Invalidated-count baseline moved to post-parse

### Other

- Remove private-item link candidates from the scan module docs
- Cross-parse cache snapshots make edit-span reuse sound (TODO.perf/4)
- Avoid a blockquote-parsing line start in shared-prefix doc
- Shared-prefix split unblocks BYTE_DISPATCH (TODO.perf/3)
- SIMD class-run scanning; VM accepts Dynamic grammars; capture rollback (TODO.perf/2, TODO.perf/5)

### Other

- Remove unused bindings and redundant drops
- Drop the unused dynamic-root binding entirely

### Fixed

- Adopt fragment subtrees into the parent arena (GH-76)
- Apply the GH-76 dynamic guards and lock-drop to the VM path
- Break the dynamic-recursion deadlock and bound runaway fragments (GH-76)

### Other

- Remove empty line after cfg attribute (clippy -Dwarnings)
- Rustfmt + drop unused VM binding (CI -Dwarnings)
- Persist compiled programs in an artifact cache (TODO.perf/1)

### Added

- Resolve_fragment for native Dynamic atoms (WIP, GH-85)

### Fixed

- Resolve the Parsanol/Native constants as modules in the dynamic bridge

### Added

- Flat event stream API for the parslet-shaped AST

### Fixed

- Reach full AST parity for the parslet transform
- Keep position integrity when interning joined strings
- Flatten empty sequences and maybes to empty string in parslet transform
- Merge mixed-key sequence hashes as siblings in parslet transform

### Other

- Satisfy rustfmt and clippy

### Other

- Remove dead AstNode::Tagged variant ([#50](https://github.com/parsanol/parsanol-rs/pull/50))

### Added

- Further fixes

### Fixed

- Distinguish duplicate labels from true repetition in AST transformation

### Other

- Lint

## [0.4.1]

### Added

- Add batch FFI support with tagged AST nodes
- Add :sequence/:repetition tags to batch format
- Update slice transform
- Use magnus and rb-sys compatible with ruby 4.0

### Fixed

- Intern_string returns StringRef, add #[non_exhaustive] to AstNode
- Update tests for intern_string returning InputRef
- Formatting and clippy for batch FFI changes
- Duplicate keys should keep last value (Parslet semantics)
- Wrapper vs repetition pattern detection in transform.rs
- Magnus version alignment and normalize.rs push fix
- Resolve Ruby FFI warnings and error

### Other

- Release v0.4.1

## [0.4.0]

### Added

- Single parse() API with lazy line/column computation (BREAKING)

### Fixed

- Update parsanol-derive version to 0.4.0

## [0.3.0]

### Added

- Add comprehensive all-examples benchmark and backend guide
- Add comprehensive backend comparison benchmark
- Implement bytecode parser backend in addition to packrat

### Other

- Add pre-commit hooks and bump version to 0.2.0
- More fixes
- Fix gha
- Update rust to 1.86
- Remove old lexer ruby ffi
- Linting
- Release v0.1.6
