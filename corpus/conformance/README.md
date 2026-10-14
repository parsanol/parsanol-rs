# PARG conformance corpus (parsanol-tree/v2)

The behavioral contract for every engine that consumes `parsanol-tree/v2`
artifacts: Ruby (both execution modes), the Rust native engine, the wasm
build, and the TypeScript runtime. Alignment is enforced by CI, not by
convention — an engine that ships with a red case here does not ship.

## Layout

Each case is a pair:

- `<name>.parg` — the source grammar, carrying a `test` section
  (`accept` / `reject` / `example { capture: value }`) that pins the
  case's observable behavior
- `<name>.json` — the compiled artifact envelope, produced by the
  reference compiler (parsanol-ruby) and checksum-covered; the
  envelope's `source_hash` binds it to the `.parg`

`stages.yaml` is data for the `preprocess_table` case (the compiler
embeds its rows into the artifact; the file stays for source parity).

## The rules

1. **A behavior change lands with its corpus change in the same commit**
   (the WPT rule). An engine PR that changes parse behavior without a
   corpus update is incomplete.
2. **Every engine runs every case in CI.** Failures block, always.
3. **A case that exposes a divergence is added even (especially) when
   the fix is pending** — the red case is the issue tracker.
4. `example` expectations assert the *bound captures* (post-bindings,
   post-preprocess) — the observable consumer surface — not internal
   trees. Tree-level parity stays with the ruby/rust differential gate.

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
