# Spec validator (one-shot, disposable)

TODO.sota/8 **B2**: an independent implementation of the PARG match
relation, written from `docs/parg-semantics.md` (parsanol-ruby), run
against the conformance corpus and a fuzz-fixture batch once, triaged,
and archived.

## Why this exists

The engines share one wire format but not one implementation. The
reference mode (B1) removes *optimizations* from the Rust walker; it
cannot catch a misreading shared by the whole Rust codebase. This
validator is a second reading of the document in a different language
(Python + its `re` engine), so a divergence is either an engine bug or a
place where the document does not say what the engines do.

## What it models

- Values and the flatten laws V0-V4, including the named-context rules
  (a capture's **direct** value only, per the parslet `CanFlatten`
  contract).
- Terminals `Str` / `Re` (Rust-regex spellings translated), literal and
  regex lookbehind, lookahead (positive and negative).
- Combinators: sequence, ordered alternative, repetition (`min`/`max`,
  `Repetition`/`Maybe` tags), rule references (`Entity`, transparent).
- Captures (`Named`) and comment trivia capture (`TriviaCapture`, K5).
- Injected trivia as it appears in the baked artifact (`Trivia`); skip
  *injection* is a compile-time rewrite and is not modelled, only its
  result.

It does not model the bindings layer (`example` tests) or recovery;
those are outside the match relation.

## Running

```sh
python3 validate.py --corpus <parsanol-rs>/corpus/conformance \
                    --fuzz /tmp/fuzz-fixtures \
                    --report REPORT.md
```

`--fuzz` expects the Ruby fuzz spec's export layout
(`gNNN.artifact.json` + `gNNN.cases.json`), produced with
`FUZZ_EXPORT_DIR=... bundle exec rspec spec/parsanol/parg/differential_fuzz_spec.rb`.

## Disposal contract

This directory is **not** a runtime, a CI gate, or a library. It must
never be imported by `parsanol`, `parsanol-ruby`, or `parsanol-ts` and
never move under a `lib/` directory. Do not keep it green over time:

- **Re-run** when the match relation in `docs/parg-semantics.md` changes
  materially — a new combinator, a changed flatten law, a new terminal
  form.
- Every divergence is triaged to **an engine bug or a spec bug**; both
  get fixed, neither gets papered over here.
- The findings of the archived run live in `REPORT.md`.
