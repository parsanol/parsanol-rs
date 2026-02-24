//! URL Parser Example
//!
//! This example demonstrates parsing URLs into structured components.
//! Shows how to use sequence, choice, and repetition combinators.
//!
//! Run with: cargo run --example url_parser --no-default-features

use parsanol::portable::{
    parser_dsl::{dynamic, re, seq, str, GrammarBuilder, ParsletExt},
    AstArena, Grammar, PortableParser,
};
use std::collections::HashMap;

/// Build a URL grammar
fn build_url_grammar() -> Grammar {
    GrammarBuilder::new()
        // Scheme: http, https, ftp, etc.
        .rule("scheme", re("[a-z][a-z0-9+.-]*"))
        // Host: domain or IP
        .rule("host", re("[a-zA-Z0-9][a-zA-Z0-9.-]*"))
        // Port: optional :8080
        .rule("port", re(":[0-9]+"))
        // Path: /foo/bar
        .rule("path_segment", re("[^/?#]+"))
        .rule("path", re("/[^?#]*"))
        // Query: ?key=value&...
        .rule("query_string", re("\\?[^#]*"))
        // Fragment: #anchor
        .rule("fragment", re("#.*"))
        // Full URL
        .rule(
            "url",
            seq(vec![
                dynamic(re("[a-z][a-z0-9+.-]*")), // scheme
                dynamic(str("://")),
                dynamic(re("[a-zA-Z0-9][a-zA-Z0-9.-]*")), // host
                dynamic(re(":[0-9]+").optional()),        // optional port
                dynamic(re("/[^?#]*").optional()),        // optional path
                dynamic(re("\\?[^#]*").optional()),       // optional query
                dynamic(re("#.*").optional()),            // optional fragment
            ]),
        )
        .build()
}

/// Parsed URL components
#[derive(Debug, Clone)]
pub struct ParsedUrl {
    pub scheme: String,
    pub host: String,
    pub port: Option<u16>,
    pub path: Option<String>,
    pub query: Option<HashMap<String, String>>,
    pub fragment: Option<String>,
}

/// Parse a URL string into structured components
pub fn parse_url(input: &str) -> Result<ParsedUrl, String> {
    let grammar = build_url_grammar();
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    let _ast = parser
        .parse()
        .map_err(|e| format!("Parse error: {:?}", e))?;

    // Manual parsing for simplicity (the grammar validates structure)
    let input = input.trim();

    // Split scheme
    let (scheme, rest) = input.split_once("://").ok_or("Missing :// in URL")?;

    // Split fragment
    let (rest, fragment) = if let Some(idx) = rest.find('#') {
        (&rest[..idx], Some(rest[idx + 1..].to_string()))
    } else {
        (rest, None)
    };

    // Split query
    let (rest, query) = if let Some(idx) = rest.find('?') {
        let query_str = &rest[idx + 1..];
        let query = parse_query_string(query_str);
        (&rest[..idx], Some(query))
    } else {
        (rest, None)
    };

    // Split path
    let (host_port, path) = if let Some(idx) = rest.find('/') {
        (&rest[..idx], Some(rest[idx..].to_string()))
    } else {
        (rest, None)
    };

    // Split port
    let (host, port) = if let Some(idx) = host_port.rfind(':') {
        let port_str = &host_port[idx + 1..];
        let port = port_str.parse::<u16>().ok();
        (&host_port[..idx], port)
    } else {
        (host_port, None)
    };

    Ok(ParsedUrl {
        scheme: scheme.to_string(),
        host: host.to_string(),
        port,
        path,
        query,
        fragment,
    })
}

/// Parse query string into key-value pairs
fn parse_query_string(query: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for pair in query.split('&') {
        if let Some((key, value)) = pair.split_once('=') {
            map.insert(key.to_string(), value.to_string());
        } else if !pair.is_empty() {
            map.insert(pair.to_string(), String::new());
        }
    }
    map
}

fn main() {
    println!("URL Parser Example");
    println!("==================\n");

    let urls = [
        "https://example.com",
        "http://localhost:8080/api",
        "https://github.com/user/repo?tab=issues",
        "ftp://ftp.example.com:21/files/document.pdf#page=5",
        "https://search.example.com:443/search?q=rust+parser&page=1#results",
    ];

    for url in urls {
        println!("Input: {}", url);
        match parse_url(url) {
            Ok(parsed) => {
                println!("  scheme:   {}", parsed.scheme);
                println!("  host:     {}", parsed.host);
                println!("  port:     {:?}", parsed.port);
                println!("  path:     {:?}", parsed.path);
                println!("  query:    {:?}", parsed.query);
                println!("  fragment: {:?}", parsed.fragment);
            }
            Err(e) => println!("  Error: {}", e),
        }
        println!();
    }
}
