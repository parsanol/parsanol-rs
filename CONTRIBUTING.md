# Contributing to Parsanol-rs

Thank you for your interest in contributing to Parsanol-rs! This document provides guidelines and instructions for contributing.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [Making Changes](#making-changes)
- [Testing](#testing)
- [Pull Request Process](#pull-request-process)

## Code of Conduct

This project follows the [Rust Code of Conduct](https://www.rust-lang.org/policies/code-of-conduct). By participating, you are expected to uphold this code.

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/YOUR_USERNAME/parsanol-rs.git`
3. Create a feature branch: `git checkout -b my-feature`

## Development Setup

```bash
# Build the project
cargo build

# Run tests
cargo test

# Run clippy
cargo clippy --all-targets -- -D warnings

# Format code
cargo fmt

# Check formatting
cargo fmt -- --check

# Run benchmarks
cargo bench

# Build documentation
cargo doc --no-deps --open
```

### Feature-specific commands

```bash
# Test with Ruby FFI
cargo test --features ruby

# Test with WASM
cargo test --features wasm

# Test proc macro crate
cargo test -p parsanol-ruby-derive
```

## Making Changes

### Code Style

- Follow Rust standard naming conventions
- Run `cargo fmt` before committing
- Ensure `cargo clippy` passes with no warnings
- Add documentation comments to public items

### Commit Messages

Use semantic commit messages:

```
<type>(<scope>): <subject>

[optional body]
```

Types:
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation changes
- `style`: Code style changes (formatting, etc.)
- `refactor`: Code refactoring
- `test`: Adding or updating tests
- `chore`: Build process or auxiliary tool changes

Examples:
- `feat(parser): add input size limit configuration`
- `fix(arena): correct string pool index calculation`
- `docs(readme): add installation instructions`

## Testing

### Running Tests

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_parser_sequence

# Run integration tests
cargo test --test parser_integration

# Run with verbose output
cargo test -- --nocapture
```

### Writing Tests

- Place unit tests in the same file as the code (in `#[cfg(test)]` module)
- Place integration tests in the `tests/` directory
- Use descriptive test names that explain what is being tested
- Test edge cases and error conditions

## Pull Request Process

1. **Update documentation** if you change public APIs
2. **Add tests** for new functionality
3. **Update CHANGELOG.md** with your changes under `[Unreleased]`
4. **Ensure CI passes** - all tests, clippy, and formatting checks
5. **Request review** from maintainers

### PR Checklist

- [ ] Code compiles without warnings
- [ ] All tests pass
- [ ] Clippy passes with no warnings
- [ ] Code is formatted with `cargo fmt`
- [ ] Documentation is updated
- [ ] CHANGELOG.md is updated
- [ ] Commit messages follow convention

## Questions?

Open an issue for:
- Bug reports
- Feature requests
- Questions about the codebase

Thank you for contributing!
