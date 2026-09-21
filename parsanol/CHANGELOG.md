# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Compile-once program, parse_into arena ownership; hotspot warning re-land ([#115](https://github.com/parsanol/parsanol-rs/pull/115)) by @[object]

### Other

- Selective rule memoization for dynamic programs (#100, #90) ([#113](https://github.com/parsanol/parsanol-rs/pull/113)) by @[object]

### Fixed

- Capture atoms expose parsed trees to dynamic blocks (coradoc nested-block parity) ([#110](https://github.com/parsanol/parsanol-rs/pull/110)) by @[object]
- List-pattern repetition collapses correctly with named separators by @[object]

### Fixed

- Tree parity ([#83](https://github.com/parsanol/parsanol-rs/pull/83)), EOF scan ([#106](https://github.com/parsanol/parsanol-rs/pull/106)), dispatch-cache memory ([#84](https://github.com/parsanol/parsanol-rs/pull/84)) ([#107](https://github.com/parsanol/parsanol-rs/pull/107)) by @[object]

### Other

- Merge pull request #103 from parsanol/release-plz-2026-09-21T04-34-13Z by @[object]
- Drop the memchr dependency (unused since simd.rs removal) by @[object]
- Per-atom/per-opcode dispatch counters; raw-tree API docs; dead simd module removed (parsanol-rs#100) by @[object]

### Added

- Capture writes across the bridge, dispatch caching, dynamic incremental sessions (parsanol-ruby#80) by @[object]

### Fixed

- Unwrap the [atom, captures] pair before fragment resolution by @[object]

### Other

- Release v0.8.0 by @[object]
- Capture-state example uses the span accessor (doc-test) by @[object]
- Rustfmt by @[object]

### Added

- Capture writes across the bridge, dispatch caching, dynamic incremental sessions (parsanol-ruby#80) by @[object]

### Fixed

- Unwrap the [atom, captures] pair before fragment resolution by @[object]

### Other

- Capture-state example uses the span accessor (doc-test) by @[object]
- Rustfmt by @[object]

### Fixed

- Qualify the AVX2 kernel through its module (x86 build) by @[object]

### Other

- Scan module header describes the wide-window kernels by @[object]
- Runnable latency gate + two retention fixes (TODO.perf/8) by @[object]
- 32-byte window kernels (TODO.perf/6) by @[object]

### Added

- Incremental parsing sessions through the Ruby bridge (TODO.perf/4) by @[object]

### Fixed

- X86 kernel loop shape, private doc links, typos (CI) by @[object]
- Drop edit-invalidated entries before the parse, not after by @[object]
- Invalidated-count baseline moved to post-parse by @[object]

### Other

- Remove private-item link candidates from the scan module docs by @[object]
- Cross-parse cache snapshots make edit-span reuse sound (TODO.perf/4) by @[object]
- Avoid a blockquote-parsing line start in shared-prefix doc by @[object]
- Shared-prefix split unblocks BYTE_DISPATCH (TODO.perf/3) by @[object]
- SIMD class-run scanning; VM accepts Dynamic grammars; capture rollback (TODO.perf/2, TODO.perf/5) by @[object]

### Other

- Remove unused bindings and redundant drops by @[object]
- Drop the unused dynamic-root binding entirely by @[object]

### Fixed

- Adopt fragment subtrees into the parent arena (GH-76) by @[object]
- Apply the GH-76 dynamic guards and lock-drop to the VM path by @[object]
- Break the dynamic-recursion deadlock and bound runaway fragments (GH-76) by @[object]

### Other

- Remove empty line after cfg attribute (clippy -Dwarnings) by @[object]
- Rustfmt + drop unused VM binding (CI -Dwarnings) by @[object]
- Persist compiled programs in an artifact cache (TODO.perf/1) by @[object]

### Added

- Resolve_fragment for native Dynamic atoms (WIP, GH-85) by @[object]

### Fixed

- Resolve the Parsanol/Native constants as modules in the dynamic bridge by @[object]

### Added

- Flat event stream API for the parslet-shaped AST by @[object]

### Fixed

- Reach full AST parity for the parslet transform by @[object]
- Keep position integrity when interning joined strings by @[object]
- Flatten empty sequences and maybes to empty string in parslet transform by @[object]
- Merge mixed-key sequence hashes as siblings in parslet transform by @[object]

### Other

- Satisfy rustfmt and clippy by @[object]

### Other

- Remove dead AstNode::Tagged variant ([#50](https://github.com/parsanol/parsanol-rs/pull/50)) by @[object]

### Added

- Further fixes by @[object]

### Fixed

- Distinguish duplicate labels from true repetition in AST transformation by @[object]

### Other

- Lint by @[object]

## [0.4.1]

### Added

- Add batch FFI support with tagged AST nodes by @[object]
- Add :sequence/:repetition tags to batch format by @[object]
- Update slice transform by @[object]
- Use magnus and rb-sys compatible with ruby 4.0 by @[object]

### Fixed

- Intern_string returns StringRef, add #[non_exhaustive] to AstNode by @[object]
- Update tests for intern_string returning InputRef by @[object]
- Formatting and clippy for batch FFI changes by @[object]
- Duplicate keys should keep last value (Parslet semantics) by @[object]
- Wrapper vs repetition pattern detection in transform.rs by @[object]
- Magnus version alignment and normalize.rs push fix by @[object]
- Resolve Ruby FFI warnings and error by @[object]

### Other

- Release v0.4.1 by @[object]

## [0.4.0]

### Added

- Single parse() API with lazy line/column computation (BREAKING) by @[object]

### Fixed

- Update parsanol-derive version to 0.4.0 by @[object]

## [0.3.0]

### Added

- Add comprehensive all-examples benchmark and backend guide by @[object]
- Add comprehensive backend comparison benchmark by @[object]
- Implement bytecode parser backend in addition to packrat by @[object]

### Other

- Add pre-commit hooks and bump version to 0.2.0 by @[object]
- More fixes by @[object]
- Fix gha by @[object]
- Update rust to 1.86 by @[object]
- Remove old lexer ruby ffi by @[object]
- Linting by @[object]
- Release v0.1.6 by @[object]
