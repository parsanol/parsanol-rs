# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

## Reporting a Vulnerability

**DO NOT** open a public issue for security vulnerabilities.

Instead, please report security issues to: security@ribose.com

### What to Include

1. Description of the vulnerability
2. Steps to reproduce
3. Potential impact
5. Your name/handle (optional, for acknowledgment)

### Response Timeline

- **Initial response**: Within 48 hours
- **Status update**: Within 7 days
- **Resolution target**: Within 30 days (critical), 90 days (others)

## Security Considerations

### Parser Security

Parsanol-rs is a parser library. When parsing untrusted input, consider:

#### Built-in Security Limits

Parsanol-rs includes built-in protection against denial-of-service attacks:

```rust
use parsanol::portable::{PortableParser, AstArena, Grammar, parser::{DEFAULT_MAX_INPUT_SIZE, DEFAULT_MAX_RECURSION_DEPTH}};

// Default limits are applied automatically:
// - max_input_size: 100 MB
// - max_recursion_depth: 1000

let grammar: Grammar = /* ... */;
let input = "untrusted input";
let mut arena = AstArena::for_input(input.len());

// Uses default limits (100MB input, 1000 recursion depth)
let mut parser = PortableParser::new(&grammar, input, &mut arena);
```

#### Custom Security Limits

For more restrictive environments, configure custom limits:

```rust
// Create parser with custom limits for untrusted input
let mut parser = PortableParser::with_limits(
    &grammar,
    input,
    &mut arena,
    10 * 1024 * 1024,  // max_input_size: 10MB
    100,               // max_recursion_depth: 100
);

match parser.parse() {
    Ok(ast) => { /* success */ },
    Err(ParseError::InputTooLarge { input_size, max_size }) => {
        eprintln!("Input too large: {} > {}", input_size, max_size);
    },
    Err(ParseError::RecursionLimitExceeded { depth, max_depth }) => {
        eprintln!("Recursion too deep: {} > {}", depth, max_depth);
    },
    Err(e) => { /* other errors */ },
}
```

#### Input Size Limits

The `max_input_size` limit prevents memory exhaustion attacks:

- **Default**: 100 MB (100 * 1024 * 1024 bytes)
- **Set to 0**: Unlimited (not recommended for untrusted input)
- **Recommended for web services**: 1-10 MB

#### Recursion Depth Limits

Recursive grammars can cause stack overflow. The `max_recursion_depth` limit prevents this:

- **Default**: 1000 levels
- **Set to 0**: Unlimited (not recommended for untrusted input)
- **Recommended for most grammars**: 100-500

#### Timeout for Untrusted Input (Planned)

For additional protection, a timeout mechanism is planned:

```rust
// Coming soon: parse_with_timeout()
// let result = parser.parse_with_timeout(Duration::from_millis(100));
```

Until then, use external timeouts for network services:

```rust
// Use tokio::time::timeout or similar
let result = tokio::time::timeout(
    Duration::from_millis(100),
    blocking_parse(input)
);
```

### Memory Safety

Parsanol-rs uses arena allocation. While this provides excellent performance:

- Memory usage scales with input size
- Very large inputs may exhaust memory
- Consider streaming for large files

### Regular Expression Safety

Parsanol-rs uses the `regex` crate which:
- Does not suffer from catastrophic backtracking
- Has bounded execution time
- Is safe for untrusted patterns

### Unsafe Code

Parsanol-rs contains minimal unsafe code:
- Only in arena.rs for UTF-8 string conversion
- All unsafe blocks are documented with SAFETY comments
- No unsafe code in parsing hot paths

## Known Limitations

1. **No sandboxing**: Parser runs with full process privileges
2. **Timeout not built-in**: Use external timeouts (e.g., `tokio::time::timeout`)
3. **Memory scales with input**: Arena allocation grows with parse tree size

## Security Features Summary

| Feature | Status | Default |
|---------|--------|---------|
| Input size limit | ✅ Implemented | 100 MB |
| Recursion depth limit | ✅ Implemented | 1000 levels |
| Parse timeout | ❌ Planned | N/A |
| Memory limit | ❌ N/A | Use input size limit |

## Security Best Practices

1. **Use custom limits for untrusted input**:
   ```rust
   PortableParser::with_limits(&grammar, input, &mut arena, 10_000_000, 100)
   ```

2. **Always handle limit errors**:
   ```rust
   match parser.parse() {
       Err(ParseError::InputTooLarge { .. }) => { /* handle */ },
       Err(ParseError::RecursionLimitExceeded { .. }) => { /* handle */ },
       // ...
   }
   ```

3. **Use external timeouts for network services**

4. **Monitor memory usage in production**

5. **Keep dependencies updated** with `cargo audit`

6. **Review grammar complexity** before using with untrusted input

## Security Updates

Security updates will be:
- Announced on GitHub Releases
- Published to crates.io immediately
- Documented in CHANGELOG.md with `[security]` prefix

## Acknowledgments

We thank all security researchers who responsibly disclose vulnerabilities.
