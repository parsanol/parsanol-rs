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
