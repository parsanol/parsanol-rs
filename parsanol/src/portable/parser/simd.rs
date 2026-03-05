//! SIMD-optimized functions for bulk character operations using memchr
//!
//! These functions are kept for future optimization use.

#![allow(dead_code)]

/// Whitespace bytes for fast lookup
const WHITESPACE: &[u8] = b" \t\n\r\x0b\x0c";

/// Skip all whitespace characters, returning the new position
#[inline]
pub fn skip_whitespace(input: &[u8], pos: usize) -> usize {
    let slice = &input[pos..];
    for (i, &b) in slice.iter().enumerate() {
        if !WHITESPACE.contains(&b) {
            return pos + i;
        }
    }
    input.len()
}

/// Find the position of a specific byte using memchr
#[inline]
pub fn find_byte(input: &[u8], pos: usize, byte: u8) -> Option<usize> {
    if pos >= input.len() {
        return None;
    }
    memchr::memchr(byte, &input[pos..]).map(|i| pos + i)
}

/// Find the position of one of two bytes using memchr2
#[inline]
pub fn find_byte2(input: &[u8], pos: usize, byte1: u8, byte2: u8) -> Option<(usize, usize)> {
    if pos >= input.len() {
        return None;
    }
    memchr::memchr2(byte1, byte2, &input[pos..]).map(|i| {
        let found_byte = input[pos + i];
        let which = if found_byte == byte1 { 0 } else { 1 };
        (pos + i, which)
    })
}

/// Find the position of one of three bytes using memchr3
#[inline]
pub fn find_byte3(
    input: &[u8],
    pos: usize,
    byte1: u8,
    byte2: u8,
    byte3: u8,
) -> Option<(usize, usize)> {
    if pos >= input.len() {
        return None;
    }
    memchr::memchr3(byte1, byte2, byte3, &input[pos..]).map(|i| {
        let found_byte = input[pos + i];
        let which = if found_byte == byte1 {
            0
        } else if found_byte == byte2 {
            1
        } else {
            2
        };
        (pos + i, which)
    })
}

/// Find a substring pattern using memmem
#[inline]
pub fn find_pattern(input: &[u8], pos: usize, pattern: &[u8]) -> Option<usize> {
    if pos >= input.len() || pattern.is_empty() {
        return None;
    }
    memchr::memmem::find(&input[pos..], pattern).map(|i| pos + i)
}

/// Skip all characters matching a predicate
#[inline]
pub fn skip_while<F: Fn(u8) -> bool>(input: &[u8], pos: usize, predicate: F) -> usize {
    let mut current = pos;
    let len = input.len();

    // Process in chunks for better cache locality
    while current + 8 <= len {
        let chunk = &input[current..current + 8];
        let mut stop = false;
        for &b in chunk {
            if !predicate(b) {
                stop = true;
                break;
            }
            current += 1;
        }
        if stop {
            return current;
        }
    }

    while current < len && predicate(input[current]) {
        current += 1;
    }

    current
}

/// Skip digits (0-9)
#[inline]
pub fn skip_digits(input: &[u8], pos: usize) -> usize {
    skip_while(input, pos, |b| b.is_ascii_digit())
}

/// Skip hex digits
#[inline]
pub fn skip_hex(input: &[u8], pos: usize) -> usize {
    skip_while(input, pos, |b| b.is_ascii_hexdigit())
}

/// Skip alphabetic characters
#[inline]
pub fn skip_alpha(input: &[u8], pos: usize) -> usize {
    skip_while(input, pos, |b| b.is_ascii_alphabetic())
}

/// Skip alphanumeric characters
#[inline]
pub fn skip_alphanumeric(input: &[u8], pos: usize) -> usize {
    skip_while(input, pos, |b| b.is_ascii_alphanumeric())
}

/// Find the end of a quoted string, handling escape sequences
#[inline]
pub fn find_string_end(input: &[u8], pos: usize, quote: u8, escape: u8) -> Option<usize> {
    let mut current = pos;
    let len = input.len();

    while current < len {
        let next = find_byte2(input, current, quote, escape)?;
        if input[next.0] == escape {
            current = next.0 + 2;
        } else {
            return Some(next.0 + 1);
        }
    }

    None
}
