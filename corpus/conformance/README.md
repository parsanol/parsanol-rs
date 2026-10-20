# PARG conformance corpus (parsanol-tree/v2)

The behavioral contract for every engine that consumes `parsanol-tree/v2`
artifacts: Ruby (both execution modes), the Rust native engine, the wasm
build, and the TypeScript runtime. Alignment is enforced by CI, not by
convention — an engine that ships with a red case here does not ship.

The normative rules this corpus pins live in
[`docs/parg-semantics.md`](https://github.com/parsanol/parsanol-ruby/blob/main/docs/parg-semantics.md)
(parsanol-ruby): the match relation, capture and flattening laws, trivia
rewrite, bindings obligations, and the case↔rule coverage matrix. Each case
cites the rule IDs it pins in its first line (`## Semantics: …`).

## Layout

Each case is a pair:

- `<name>.parg` — the source grammar, carrying a `test` section
  (`accept` / `reject`, plus `capture { ... }` and/or `example { ... }`
  expectations) that pins the case's observable behavior
- `<name>.json` — the compiled artifact envelope, produced by the
  reference compiler (parsanol-ruby) and checksum-covered; the
  envelope's `source_hash` binds it to the `.parg`

A `.parg` **without a sibling `.json` is an import helper** — it exists
to be referenced via `use` (resolved against its file name) and is
skipped by the case walkers. `use`-carrying cases merge imports from
this directory at compile time; their replay passes the corpus as
`imports_dir` on the Ruby rebuild path (envelope-atom engines need
nothing — the atoms are merged at bake time).

`stages.yaml` is data for the `preprocess_table` case (the compiler
embeds its rows into the artifact; the file stays for source parity).

## The rules

1. **A behavior change lands with its corpus change in the same commit**
   (the WPT rule). An engine PR that changes parse behavior without a
   corpus update is incomplete.
2. **Every engine runs every case in CI.** Failures block, always.
3. **A case that exposes a divergence is added even (especially) when
   the fix is pending** — the red case is the issue tracker.
4. **Compile-parity ignores provenance fields** (`compiler_version`,
   `checksum`): artifacts bind to their source via `source_hash`, so gem
   version bumps do not — and must not — invalidate corpus cases.
5. **Two test phases** (parsanol-ruby#216): `capture` asserts the *raw
   capture tree* — the grammar layer, no bindings section needed.
   `example` asserts the *bound, preprocessed attribute hash* — the
   bindings layer, the observable consumer surface. `example` in a
   grammar without bindings rows is an authoring error; Tree-level
   parity stays with the ruby/rust differential gate.
6. **Expectation equality is canonical** on every engine: terminals
   reduce to their text (slice wire objects to their `value`), structure
   recurses — Ruby `Bindings.canonical`, Rust `suite::canonical`.
7. **Every case cites the normative rules it pins** in its first line
   (`## Semantics: T1, C1, …`, see `docs/parg-semantics.md`). A new or
   changed case without rule citations fails review; a rule with no
   case is a gap tracked in parsanol-rs#254.

## Running

- Rust: `cargo test -p parsanol` (the `conformance` test walks this
  directory)
- Ruby: `PARG_CONFORMANCE_DIR=<this dir> bundle exec rspec
  spec/parsanol/parg/conformance_spec.rb` (compile-parity + both modes)
- TypeScript: `PARG_CONFORMANCE_DIR=<this dir> npm test`

## Adding a case

Author the `.parg`, compile with the reference compiler
(`parsanol parg compile` or the `PARG::Compiler`), commit both files in
one commit, and make sure every engine's suite goes green before merge.
