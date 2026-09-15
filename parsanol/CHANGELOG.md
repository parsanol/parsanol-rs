# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- C-ABI handle + batch functions; build the cdylib by @[object]
- Distinguish maybe from repeat(0,1) and collapse input refs in Ruby transform by @[object]

### Fixed

- Drop the redundant isize cast in the c-api test by @[object]
- Drop unnecessary unsafe on the safe release call in the c-api test by @[object]
- Never mutate shared atoms when merging Str/Re runs in optimize() by @[object]
- Bump magnus to 9407bd9 for 32-bit musl timespec fix by @[object]
- Satisfy clippy -D warnings with current toolchain by @[object]

### Other

- Cargo fmt by @[object]
- Collapse in the C-ABI tier too; collapse_ast moves to shared by @[object]
- One decode path — extension tier returns the raw batch format by @[object]
- Handle-based grammar API with zero-copy input by @[object]
- Release v0.5.0 by @[object]
- Remove dead AstNode::Tagged variant ([#50](https://github.com/parsanol/parsanol-rs/pull/50)) by @[object]

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
