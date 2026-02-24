//! IP Address Parser Example
//!
//! This example demonstrates parsing IPv4 and IPv6 addresses.
//! Based on the Parslet ip_address.rb example.
//!
//! Run with: cargo run --example ip_address --no-default-features

#![allow(clippy::print_literal)]

use parsanol::portable::{
    parser_dsl::{choice, dynamic, re, seq, str, GrammarBuilder},
    AstArena, Grammar, PortableParser,
};

/// Build an IPv4 grammar
fn build_ipv4_grammar() -> Grammar {
    GrammarBuilder::new()
        // Single digit
        .rule("digit", re("[0-9]"))
        // Dec octet: 0-255
        // 25[0-5] | 2[0-4][0-9] | 1[0-9][0-9] | [1-9][0-9] | [0-9]
        .rule(
            "dec_octet",
            choice(vec![
                dynamic(re("25[0-5]")),     // 250-255
                dynamic(re("2[0-4][0-9]")), // 200-249
                dynamic(re("1[0-9][0-9]")), // 100-199
                dynamic(re("[1-9][0-9]")),  // 10-99
                dynamic(re("[0-9]")),       // 0-9
            ]),
        )
        // IPv4: dec_octet.dec_octet.dec_octet.dec_octet
        .rule(
            "ipv4",
            seq(vec![
                dynamic(re("(25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9][0-9]|[0-9])")),
                dynamic(str(".")),
                dynamic(re("(25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9][0-9]|[0-9])")),
                dynamic(str(".")),
                dynamic(re("(25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9][0-9]|[0-9])")),
                dynamic(str(".")),
                dynamic(re("(25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9][0-9]|[0-9])")),
            ]),
        )
        .build()
}

/// Build an IPv6 grammar (simplified)
fn build_ipv6_grammar() -> Grammar {
    GrammarBuilder::new()
        // Hex digit
        .rule("hexdigit", re("[0-9a-fA-F]"))
        // h16: 1-4 hex digits
        .rule("h16", re("[0-9a-fA-F]{1,4}"))
        // ls32: h16:h16 or IPv4
        .rule("ls32", choice(vec![
            dynamic(re("[0-9a-fA-F]{1,4}:[0-9a-fA-F]{1,4}")),
            dynamic(re("(25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9][0-9]|[0-9])\\.(25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9][0-9]|[0-9])\\.(25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9][0-9]|[0-9])\\.(25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9][0-9]|[0-9])")),
        ]))
        // IPv6 patterns (simplified - covers most common cases)
        .rule("ipv6", choice(vec![
            // Full form: 8 groups of h16
            dynamic(re("[0-9a-fA-F]{1,4}(:[0-9a-fA-F]{1,4}){7}")),
            // With :: compression
            dynamic(re("::([0-9a-fA-F]{1,4}(:[0-9a-fA-F]{1,4}){0,6})?")),
            dynamic(re("[0-9a-fA-F]{1,4}::([0-9a-fA-F]{1,4}(:[0-9a-fA-F]{1,4}){0,5})?")),
            dynamic(re("[0-9a-fA-F]{1,4}(:[0-9a-fA-F]{1,4})?::([0-9a-fA-F]{1,4}(:[0-9a-fA-F]{1,4}){0,4})?")),
            dynamic(re("[0-9a-fA-F]{1,4}(:[0-9a-fA-F]{1,4}){0,2}::([0-9a-fA-F]{1,4}(:[0-9a-fA-F]{1,4}){0,3})?")),
            dynamic(re("[0-9a-fA-F]{1,4}(:[0-9a-fA-F]{1,4}){0,3}::([0-9a-fA-F]{1,4}(:[0-9a-fA-F]{1,4}){0,2})?")),
            dynamic(re("[0-9a-fA-F]{1,4}(:[0-9a-fA-F]{1,4}){0,4}::([0-9a-fA-F]{1,4}(:[0-9a-fA-F]{1,4})?)?")),
            dynamic(re("[0-9a-fA-F]{1,4}(:[0-9a-fA-F]{1,4}){0,5}::[0-9a-fA-F]{1,4}?")),
            dynamic(re("[0-9a-fA-F]{1,4}(:[0-9a-fA-F]{1,4}){0,6}::")),
        ]))
        .build()
}

/// Parsed IP address
#[derive(Debug, Clone)]
pub enum IpAddress {
    Ipv4(String),
    Ipv6(String),
}

/// Parse an IP address (IPv4 or IPv6)
pub fn parse_ip(input: &str) -> Result<IpAddress, String> {
    let input = input.trim();

    // Try IPv4 first
    let ipv4_grammar = build_ipv4_grammar();
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&ipv4_grammar, input, &mut arena);

    if parser.parse().is_ok() {
        return Ok(IpAddress::Ipv4(input.to_string()));
    }

    // Try IPv6
    let ipv6_grammar = build_ipv6_grammar();
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&ipv6_grammar, input, &mut arena);

    if parser.parse().is_ok() {
        return Ok(IpAddress::Ipv6(input.to_string()));
    }

    Err(format!("Invalid IP address: {}", input))
}

/// Validate an IPv4 address octet
pub fn validate_ipv4_octet(octet: &str) -> bool {
    octet.parse::<u8>().is_ok()
}

/// Parse and validate IPv4 address
pub fn parse_ipv4_components(input: &str) -> Option<[u8; 4]> {
    let parts: Vec<&str> = input.split('.').collect();
    if parts.len() != 4 {
        return None;
    }

    let mut octets = [0u8; 4];
    for (i, part) in parts.iter().enumerate() {
        octets[i] = part.parse::<u8>().ok()?;
    }
    Some(octets)
}

fn main() {
    println!("IP Address Parser Example");
    println!("========================\n");

    let addresses = [
        // Valid IPv4
        "0.0.0.0",
        "192.168.1.1",
        "255.255.255.255",
        "10.0.0.1",
        "127.0.0.1",
        // Invalid IPv4
        "255.255.255",
        "256.1.1.1",
        "1.2.3.4.5",
        // Valid IPv6
        "::1",
        "::",
        "2001:db8::1",
        "fe80::1",
        "2001:0db8:85a3:0000:0000:8a2e:0370:7334",
        "2001:db8:85a3::8a2e:370:7334",
        "::ffff:192.168.1.1",
        // Invalid IPv6
        "1:2",
        "gggg::1",
    ];

    println!("{:<40} | {:<10} | {}", "Address", "Type", "Components");
    println!("{}", "-".repeat(70));

    for addr in addresses {
        match parse_ip(addr) {
            Ok(IpAddress::Ipv4(ip)) => {
                let components = parse_ipv4_components(&ip);
                println!("{:<40} | {:<10} | {:?}", addr, "IPv4", components);
            }
            Ok(IpAddress::Ipv6(_ip)) => {
                println!("{:<40} | {:<10} | {}", addr, "IPv6", "(hex groups)");
            }
            Err(e) => {
                println!("{:<40} | {:<10} | {}", addr, "INVALID", e);
            }
        }
    }

    // Demonstrate IPv4 component parsing
    println!("\nIPv4 Component Parsing:");
    println!("-----------------------");
    for ip in ["192.168.1.1", "10.0.0.1", "127.0.0.1"] {
        if let Some(octets) = parse_ipv4_components(ip) {
            println!("{} -> {:?}", ip, octets);
        }
    }
}
