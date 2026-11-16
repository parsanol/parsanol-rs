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
