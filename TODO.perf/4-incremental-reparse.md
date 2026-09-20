# 4. Incremental reparsing by edit span — SHIPPED

`portable/incremental.rs` keeps a persistent session: the memo window
an edit provably did not touch (entries whose result ended at or
before the earliest edit offset; the root entry at 0 additionally
drops when the input length changed) survives to the next parse.

Two soundness problems in the retained-cache design — exposed by the
new differential gate — are fixed:

- Pool-backed node data (arrays, hashes, interned strings) referenced
  the previous parse's arena. Retained entries are now adopted into
  a stable snapshot arena owned by the session (top bit of the
  entry's generation marks them; hits adopt back into the live
  arena) with a 32 MiB budget guard that drops the store wholesale.
- Entries at or after the edit offset describe the old input and are
  dropped BEFORE the parse (replaying them returned end positions
  beyond the new input's length).

Gates: a deterministic 30-edit sequence over a 1200-line document
asserts incremental trees equal full re-parses (and acceptance equal
on unparsable docs); a late-edit case asserts substantial cache
reuse. A rule-based grammar regression covers rule-boundary entries.
Exposed through the FFI as incremental sessions and from Ruby as
`Parsanol::IncrementalSession`.
