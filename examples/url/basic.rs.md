# URL Parser - Rust Implementation

## How to Run

```bash
cargo run --example url/basic --no-default-features
```

## Code Walkthrough

### Scheme Detection

The parser first identifies the URL scheme (protocol):

```rust
.rule("scheme", choice(vec![
    dynamic(str("https")),
    dynamic(str("http")),
    dynamic(str("ftp")),
    dynamic(re("[a-z]+")),
]))
.rule("scheme_sep", str("://"))
```

Schemes are case-insensitive and followed by `://`. Common schemes are recognized explicitly, with a fallback regex for custom schemes.

### Host and Port Parsing

The host can be a domain name, IP address, or localhost:

```rust
.rule("host", re(r#"[a-zA-Z0-9.-]+"#))
.rule("port", seq(vec![dynamic(str(":")), dynamic(re("[0-9]+"))]))
```

Port is optional and defaults based on scheme (80 for http, 443 for https). The parser extracts it when present.

### Path Component

The path starts with `/` and continues until `?`, `#`, or end:

```rust
.rule("path", seq(vec![
    dynamic(str("/")),
    dynamic(re(r#"[^?#]*"#)),
]))
```

Paths may contain multiple segments separated by `/`, and each segment can have URL-encoded characters.

### Query String Parsing

Query strings are parsed into key-value pairs:

```rust
.rule("query", seq(vec![
    dynamic(str("?")),
    dynamic(re(r#"[^#]*"#)),
]))
```

The query string is further parsed into a HashMap, handling `&` separators and `=` assignments.

### Fragment Extraction

The fragment (anchor) is the final component:

```rust
.rule("fragment", seq(vec![
    dynamic(str("#")),
    dynamic(re(r#".*"#)),
]))
```

Fragments are used for in-page navigation and are not sent to the server.

## Output Types

```rust
pub struct ParsedUrl {
    pub scheme: String,
    pub host: String,
    pub port: Option<u16>,
    pub path: Option<String>,
    pub query: Option<HashMap<String, String>>,
    pub fragment: Option<String>,
}
```

Each component is optional except scheme and host, allowing flexible URL handling.

## Design Decisions

### Why Separate Query Parsing?

Query strings can be complex (arrays, nested objects, encoding). Separating extraction from parsing allows custom handling per application needs.

### Relative URL Handling

This parser handles absolute URLs only. Relative URL resolution requires a base URL and is typically handled by higher-level libraries.
