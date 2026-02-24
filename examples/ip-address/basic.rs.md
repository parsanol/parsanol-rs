# IP Address Parser - Rust Implementation

## How to Run

```bash
cargo run --example ip-address/basic --no-default-features
```

## Code Walkthrough

### IPv4 Octet Parsing

Each IPv4 octet is 0-255

```rust
.rule("octet", re(r#"[0-9]{1,3}"#))
.rule("ipv4", seq(vec![
    dynamic(octet), dynamic(str(".")),
    dynamic(octet), dynamic(str(".")),
    dynamic(octet), dynamic(str(".")),
    dynamic(octet),
]))
```

The regex matches 1-3 digits, with post-parse validation ensuring 0-255 range.

### IPv4 Validation

After parsing, octets are validated:

```rust
fn validate_ipv4(octets: &[u8; 4]) -> Result<(), String> {
    for &octet in octets {
        if *octet > 255 {
            return Err(format!("Octet {} exceeds 255", octet));
        }
    }
    Ok(())
}
```

Leading zeros are preserved in parsing but cause ambiguity (e.g., `01` vs `1`).

### IPv6 Full Format

IPv6 has 8 groups of 4 hex digits:

```rust
.rule("ipv6_full", seq(vec![
    dynamic(re("[0-9a-fA-F]{4}")),
    dynamic(seq(vec![dynamic(str(":")), dynamic(re("[0-9a-fA-F]{4}"))]).repeat(7)),
]))
```

Each group represents 16 bits, totaling 128 bits for the address.

### IPv6 Compressed Format

The `::` notation compresses consecutive zero groups

```rust
.rule("ipv6_compressed", seq(vec![
    dynamic(re("[0-9a-fA-F]{0,4}")),
    dynamic(str("::")),
    dynamic(re("[0-9a-fA-F]{0,4}")),
]))
```

`::1` is the loopback (equivalent to `0:0:0:0:0:0:0:0:0:1`), `::` alone is the unspecified address.

### IPv6 Expansion

Compressed addresses are expanded for comparison

```rust
fn expand_ipv6(compressed: &str) -> [u16; 8] {
    let parts: Vec<&str> = compressed.split("::").collect();
    let left: Vec<u16> = parts[0].split(':').filter_map(|p| u16::from_str_radix(p) 16).ok()).collect();
    let right: Vec<u16> = parts[1].split(':').filter_map(|p| u16::from_str_radix(p) 16).ok()).collect();
    let zeros = 8 - left.len() - right.len();
    let mut result = left;
    result.extend(vec![0; zeros / 2]);
    result.extend(right);
    result
}
```

Expansion normalizes addresses for consistent comparison.

## Output Types

```rust
pub enum IpAddress {
    IPv4 { octets: [u8; 4] },
    IPv6 { groups: [u16; 8] },
}

pub struct ParsedIp {
    pub address: IpAddress,
    pub original: String,
}
```

The enum distingu between address families, enabling family-specific operations.

## Design Decisions

### Why Separate Types for IPv4 and IPv6?

The two formats are fundamentally different:
- IPv4: 32-bit, 4 octets
- IPv6: 128-bit, 8 groups

Separate types enable type-safe operations and prevent accidental mixing.

### IPv4-Mapped IPv6

IPv6 can embed IPv4 addresses (`::ffff:192.168.1.1`). This is handled by parsing as IPv6 address with special rendering.
