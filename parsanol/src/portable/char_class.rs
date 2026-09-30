//! Character class lookup tables for O(1) membership testing
//!
//! Pre-computed lookup tables for common character classes.
//! Each table is a 256-byte bitmap for O(1) lookup of ASCII characters.
//!
//! For UTF-8 multi-byte characters, we fall back to the regex crate.
//!
//! # CharacterPattern
//!
//! This module also provides [`CharacterPattern`], a unified enum for character
//! class matching that eliminates duplication between parsing and predicate
//! generation.

use std::sync::OnceLock;

/// Static lookup table for pattern -> CharacterPattern
/// Uses OnceLock for lazy initialization (done once, O(1) lookup thereafter)
static PATTERN_MAP: OnceLock<std::collections::HashMap<&'static str, CharacterPattern>> =
    OnceLock::new();

/// Character pattern with unified matching logic
///
/// This enum consolidates character class pattern handling, providing:
/// - Single source of truth for pattern names
/// - Single-char matching via the CHAR_CLASSES table
/// - Bulk matching predicates for SIMD optimization
///
/// # Example
///
/// ```rust
/// use parsanol::portable::char_class::CharacterPattern;
///
/// // Parse from regex pattern
/// let pattern = CharacterPattern::from_pattern("\\d").unwrap();
///
/// // Match single byte
/// assert!(pattern.matches(b'5'));
/// assert!(!pattern.matches(b'a'));
///
/// // Get predicate for bulk matching
/// let pred = pattern.predicate();
/// assert!(pred(b'9'));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CharacterPattern {
    /// Digit: [0-9] or \\d
    Digit,
    /// Non-digit: \\D
    NonDigit,
    /// Lowercase: [a-z]
    Lower,
    /// Uppercase: [A-Z]
    Upper,
    /// Alphabetic: [a-zA-Z]
    Alpha,
    /// Alphanumeric: [a-zA-Z0-9]
    Alnum,
    /// Word character: [a-zA-Z0-9_] or \\w
    Word,
    /// Non-word character: \\W
    NonWord,
    /// Hex digit: [0-9a-fA-F]
    HexDigit,
    /// Whitespace: [ \\t\\n\\r\\f\\v] or \\s
    Space,
    /// Non-whitespace: \\S
    NonSpace,
    /// Blank: [ \\t]
    Blank,
    /// Control characters
    Cntrl,
    /// Printable (not space, not cntrl)
    Graph,
    /// Printable including space
    Print,
    /// Punctuation
    Punct,
    /// Any character: .
    Any,
}

impl CharacterPattern {
    /// Try to parse a regex pattern into a CharacterPattern
    ///
    /// Returns `Some(CharacterPattern)` if the pattern is a known character class,
    /// `None` otherwise (complex regex patterns need the regex crate).
    ///
    /// # Supported Patterns
    ///
    /// | Pattern | CharacterPattern |
    /// |---------|-----------------|
    /// | `.` | Any |
    /// | `[0-9]`, `\\d` | Digit |
    /// | `\\D` | NonDigit |
    /// | `[a-z]` | Lower |
    /// | `[A-Z]` | Upper |
    /// | `[a-zA-Z]`, `[A-Za-z]`, `\\p{L}` | Alpha |
    /// | `[a-zA-Z0-9]`, `[0-9a-zA-Z]` | Alnum |
    /// | `\\w`, `[a-zA-Z0-9_]` | Word |
    /// | `\\W` | NonWord |
    /// | `[0-9a-fA-F]` | HexDigit |
    /// | `\\s`, `[ \\t\\n\\r]` | Space |
    /// | `\\S` | NonSpace |
    /// | `[ \\t]`, `\\h` | Blank |
    #[inline]
    pub fn from_pattern(pattern: &str) -> Option<Self> {
        // Use lazy-initialized static HashMap for O(1) lookup
        PATTERN_MAP
            .get_or_init(|| {
                std::collections::HashMap::from([
                    // Any character
                    (".", Self::Any),
                    // Digit
                    ("[0-9]", Self::Digit),
                    ("\\d", Self::Digit),
                    ("\\D", Self::NonDigit),
                    // Case
                    ("[a-z]", Self::Lower),
                    ("[A-Z]", Self::Upper),
                    // Alpha
                    ("[a-zA-Z]", Self::Alpha),
                    ("[A-Za-z]", Self::Alpha),
                    ("\\p{L}", Self::Alpha),
                    // Alnum
                    ("[a-zA-Z0-9]", Self::Alnum),
                    ("[0-9a-zA-Z]", Self::Alnum),
                    // Word
                    ("\\w", Self::Word),
                    ("[a-zA-Z0-9_]", Self::Word),
                    ("[0-9a-zA-Z_]", Self::Word),
                    ("\\W", Self::NonWord),
                    // Hex
                    ("[0-9a-fA-F]", Self::HexDigit),
                    ("[0-9A-Fa-f]", Self::HexDigit),
                    // Whitespace
                    ("\\s", Self::Space),
                    ("[ \t\n\r]", Self::Space),
                    (r"[ \t\n\r\f\v]", Self::Space),
                    ("\\S", Self::NonSpace),
                    // Blank
                    ("[ \t]", Self::Blank),
                    ("\\h", Self::Blank),
                ])
            })
            .get(pattern)
            .copied()
    }

    /// Check if a byte matches this character pattern
    ///
    /// Uses O(1) lookup tables from CHAR_CLASSES.
    #[inline(always)]
    pub fn matches(&self, b: u8) -> bool {
        CHAR_CLASSES.matches_pattern(*self, b)
    }

    /// Get a predicate function for bulk matching
    ///
    /// Returns a function pointer that can be used for SIMD bulk matching.
    #[inline]
    pub fn predicate(&self) -> fn(u8) -> bool {
        match self {
            Self::Digit => |b| CHAR_CLASSES.is_digit(b),
            Self::NonDigit => |b| !CHAR_CLASSES.is_digit(b),
            Self::Lower => |b| CHAR_CLASSES.is_lower(b),
            Self::Upper => |b| CHAR_CLASSES.is_upper(b),
            Self::Alpha => |b| CHAR_CLASSES.is_alpha(b),
            Self::Alnum => |b| CHAR_CLASSES.is_alnum(b),
            Self::Word => |b| CHAR_CLASSES.is_word(b),
            Self::NonWord => |b| !CHAR_CLASSES.is_word(b),
            Self::HexDigit => |b| CHAR_CLASSES.is_hex_digit(b),
            Self::Space => |b| CHAR_CLASSES.is_space(b),
            Self::NonSpace => |b| !CHAR_CLASSES.is_space(b),
            Self::Blank => |b| CHAR_CLASSES.is_blank(b),
            Self::Cntrl => |b| CHAR_CLASSES.is_cntrl(b),
            Self::Punct => |b| CHAR_CLASSES.is_punct(b),
            // For Any: matches any byte (used with proper UTF-8 handling elsewhere)
            Self::Any => |_b| true,
            // Graph and Print are combinations - handle specially
            Self::Graph => |b| (33..127).contains(&b),
            Self::Print => |b| (32..127).contains(&b),
        }
    }

    /// Check if this pattern matches the negation of another
    pub fn is_negation_of(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (Self::Digit, Self::NonDigit)
                | (Self::NonDigit, Self::Digit)
                | (Self::Word, Self::NonWord)
                | (Self::NonWord, Self::Word)
                | (Self::Space, Self::NonSpace)
                | (Self::NonSpace, Self::Space)
        )
    }
}

/// Pre-computed character class lookup tables
///
/// These tables provide O(1) lookup for ASCII character membership.
/// Each boolean array is indexed by the byte value (0-255).
#[derive(Clone, Copy, Debug)]
pub struct CharClassTables {
    /// Digit characters [0-9]
    pub digit: [bool; 256],

    /// Hex digit characters [0-9a-fA-F]
    pub hex_digit: [bool; 256],

    /// Lowercase letters [a-z]
    pub lower: [bool; 256],

    /// Uppercase letters [A-Z]
    pub upper: [bool; 256],

    /// Alphabetic characters [a-zA-Z]
    pub alpha: [bool; 256],

    /// Alphanumeric characters [a-zA-Z0-9]
    pub alnum: [bool; 256],

    /// Word characters [a-zA-Z0-9_]
    pub word: [bool; 256],

    /// Whitespace characters [ \t\n\r]
    pub space: [bool; 256],

    /// Blank characters [ \t]
    pub blank: [bool; 256],

    /// Control characters (ASCII 0-31 and 127)
    pub cntrl: [bool; 256],

    /// Printable characters (not space, not cntrl)
    pub graph: [bool; 256],

    /// Printable characters including space
    pub print: [bool; 256],

    /// Punctuation characters
    pub punct: [bool; 256],
}

impl CharClassTables {
    /// Create all character class tables at compile time
    pub const fn new() -> Self {
        let mut tables = Self {
            digit: [false; 256],
            hex_digit: [false; 256],
            lower: [false; 256],
            upper: [false; 256],
            alpha: [false; 256],
            alnum: [false; 256],
            word: [false; 256],
            space: [false; 256],
            blank: [false; 256],
            cntrl: [false; 256],
            graph: [false; 256],
            print: [false; 256],
            punct: [false; 256],
        };

        // Initialize digit [0-9]
        let mut i = b'0';
        while i <= b'9' {
            tables.digit[i as usize] = true;
            tables.hex_digit[i as usize] = true;
            tables.alnum[i as usize] = true;
            tables.word[i as usize] = true;
            tables.graph[i as usize] = true;
            tables.print[i as usize] = true;
            i += 1;
        }

        // Initialize lowercase [a-z] and hex [a-f]
        i = b'a';
        while i <= b'z' {
            tables.lower[i as usize] = true;
            tables.alpha[i as usize] = true;
            tables.alnum[i as usize] = true;
            tables.word[i as usize] = true;
            tables.graph[i as usize] = true;
            tables.print[i as usize] = true;
            if i <= b'f' {
                tables.hex_digit[i as usize] = true;
            }
            i += 1;
        }

        // Initialize uppercase [A-Z] and hex [A-F]
        i = b'A';
        while i <= b'Z' {
            tables.upper[i as usize] = true;
            tables.alpha[i as usize] = true;
            tables.alnum[i as usize] = true;
            tables.word[i as usize] = true;
            tables.graph[i as usize] = true;
            tables.print[i as usize] = true;
            if i <= b'F' {
                tables.hex_digit[i as usize] = true;
            }
            i += 1;
        }

        // Word character: underscore
        tables.word[b'_' as usize] = true;
        tables.graph[b'_' as usize] = true;
        tables.print[b'_' as usize] = true;

        // Whitespace
        tables.space[b' ' as usize] = true;
        tables.space[b'\t' as usize] = true;
        tables.space[b'\n' as usize] = true;
        tables.space[b'\r' as usize] = true;
        tables.space[0x0C_usize] = true; // form feed
        tables.space[0x0B_usize] = true; // vertical tab

        // Blank (space and tab only)
        tables.blank[b' ' as usize] = true;
        tables.blank[b'\t' as usize] = true;

        // Print includes space
        tables.print[b' ' as usize] = true;

        // Control characters (0-31 and 127)
        i = 0;
        while i < 32 {
            tables.cntrl[i as usize] = true;
            i += 1;
        }
        tables.cntrl[127] = true;

        // Punctuation: printable but not alphanumeric or space
        i = 33;
        while i < 127 {
            if !tables.alnum[i as usize] {
                tables.punct[i as usize] = true;
            }
            i += 1;
        }

        tables
    }

    /// Check if byte is a digit [0-9]
    #[inline(always)]
    pub fn is_digit(&self, b: u8) -> bool {
        self.digit[b as usize]
    }

    /// Check if byte is a hex digit [0-9a-fA-F]
    #[inline(always)]
    pub fn is_hex_digit(&self, b: u8) -> bool {
        self.hex_digit[b as usize]
    }

    /// Check if byte is lowercase [a-z]
    #[inline(always)]
    pub fn is_lower(&self, b: u8) -> bool {
        self.lower[b as usize]
    }

    /// Check if byte is uppercase [A-Z]
    #[inline(always)]
    pub fn is_upper(&self, b: u8) -> bool {
        self.upper[b as usize]
    }

    /// Check if byte is alphabetic [a-zA-Z]
    #[inline(always)]
    pub fn is_alpha(&self, b: u8) -> bool {
        self.alpha[b as usize]
    }

    /// Check if byte is alphanumeric [a-zA-Z0-9]
    #[inline(always)]
    pub fn is_alnum(&self, b: u8) -> bool {
        self.alnum[b as usize]
    }

    /// Check if byte is a word character [a-zA-Z0-9_]
    #[inline(always)]
    pub fn is_word(&self, b: u8) -> bool {
        self.word[b as usize]
    }

    /// Check if byte is whitespace [ \t\n\r]
    #[inline(always)]
    pub fn is_space(&self, b: u8) -> bool {
        self.space[b as usize]
    }

    /// Check if byte is blank [ \t]
    #[inline(always)]
    pub fn is_blank(&self, b: u8) -> bool {
        self.blank[b as usize]
    }

    /// Check if byte is a control character
    #[inline(always)]
    pub fn is_cntrl(&self, b: u8) -> bool {
        self.cntrl[b as usize]
    }

    /// Check if byte is a punctuation character
    #[inline(always)]
    pub fn is_punct(&self, b: u8) -> bool {
        self.punct[b as usize]
    }

    /// Check if a byte matches a character pattern
    ///
    /// This unified method supports both single-char and negated patterns.
    #[inline(always)]
    pub fn matches_pattern(&self, pattern: CharacterPattern, b: u8) -> bool {
        match pattern {
            CharacterPattern::Digit => self.digit[b as usize],
            CharacterPattern::NonDigit => !self.digit[b as usize],
            CharacterPattern::Lower => self.lower[b as usize],
            CharacterPattern::Upper => self.upper[b as usize],
            CharacterPattern::Alpha => self.alpha[b as usize],
            CharacterPattern::Alnum => self.alnum[b as usize],
            CharacterPattern::Word => self.word[b as usize],
            CharacterPattern::NonWord => !self.word[b as usize],
            CharacterPattern::HexDigit => self.hex_digit[b as usize],
            CharacterPattern::Space => self.space[b as usize],
            CharacterPattern::NonSpace => !self.space[b as usize],
            CharacterPattern::Blank => self.blank[b as usize],
            CharacterPattern::Cntrl => self.cntrl[b as usize],
            CharacterPattern::Graph => self.graph[b as usize],
            CharacterPattern::Print => self.print[b as usize],
            CharacterPattern::Punct => self.punct[b as usize],
            CharacterPattern::Any => true,
        }
    }
}

/// Global character class tables (compile-time initialized)
pub static CHAR_CLASSES: CharClassTables = CharClassTables::new();

/// A decoded regex character class: code-point membership without the
/// regex engine.
///
/// The general `CharacterPattern` fast path covers only the fixed list of
/// well-known patterns; every other class pattern (for example the
/// `[\x00-\u{10ffff}]` the PARG compiler emits for `%x00-10FFFF`) used to
/// fall through to `regex::Regex::find` per position, where the meta
/// engine's lazy-DFA determinization dominates the profile on
/// repetition-heavy grammars. Decoding the class once into ranges and
/// matching per character directly removes that per-position engine cost.
///
/// Semantics are inherited from the same HIR the `regex` crate itself
/// compiles to, so membership is exactly what the fallback engine would
/// decide for a pattern whose HIR is a single Unicode class.
#[derive(Debug, Clone)]
pub struct CharRanges {
    /// ASCII membership bitmap: patterns are overwhelmingly ASCII-heavy,
    /// so the hot byte test is a single indexed load.
    ascii: [bool; 128],
    /// Non-ASCII code-point ranges (start, end), sorted and disjoint.
    ranges: Vec<(u32, u32)>,
    /// True when every code point is a member (the whole-input class);
    /// non-ASCII matching then needs no range scan.
    full: bool,
}

impl CharRanges {
    /// Decode a regex pattern into a [`CharRanges`] when the pattern is a
    /// pure Unicode character class.
    ///
    /// Returns `None` for anything else (anchors, groups, alternations,
    /// repetitions, byte classes) so callers keep using the regex engine;
    /// the HIR is the same intermediate representation the `regex` crate
    /// builds, so acceptance here is semantics-preserving by construction
    /// (case-insensitive and negated classes arrive already resolved).
    pub fn from_pattern(pattern: &str) -> Option<Self> {
        let hir = regex_syntax::parse(pattern).ok()?;
        match hir.kind() {
            regex_syntax::hir::HirKind::Class(regex_syntax::hir::Class::Unicode(class)) => {
                Some(Self::from_unicode_class(class))
            }
            _ => None,
        }
    }

    fn from_unicode_class(class: &regex_syntax::hir::ClassUnicode) -> Self {
        let mut ascii = [false; 128];
        let mut ranges = Vec::new();
        for item in class.iter() {
            let (lo, hi) = (item.start() as u32, item.end() as u32);
            if lo <= 0x7F {
                for code in lo..=hi.min(0x7F) {
                    ascii[code as usize] = true;
                }
            }
            if hi > 0x7F {
                ranges.push((lo.max(0x80), hi));
            }
        }
        // HIR classes are sorted and disjoint already; the `full` flag is
        // computed exactly rather than inferred, so a class like
        // [\x00-\u{10FFFF}] takes the constant-time non-ASCII path.
        let full = ranges.len() == 1 && ranges[0] == (0x80, char::MAX as u32);
        Self { ascii, ranges, full }
    }

    /// Match the character at byte `pos`; returns its UTF-8 length when it
    /// is a member of the class.
    ///
    /// The input must be valid UTF-8 with `pos` on a char boundary, which
    /// the parser guarantees: it only advances over whole characters.
    #[inline]
    pub fn match_at(&self, bytes: &[u8], pos: usize) -> Option<usize> {
        let first = *bytes.get(pos)?;
        if first < 0x80 {
            return self.ascii[first as usize].then_some(1);
        }
        let len = utf8_char_len(first);
        let slice = bytes.get(pos..pos + len)?;
        let ch = std::str::from_utf8(slice).ok()?.chars().next()?;
        let code = ch as u32;
        if self.full || self.ranges.iter().any(|&(lo, hi)| code >= lo && code <= hi) {
            Some(len)
        } else {
            None
        }
    }
}

/// Get the UTF-8 character length from the first byte
///
/// Returns the number of bytes in the UTF-8 encoded character.
/// For ASCII (0x00-0x7F), returns 1.
/// For multi-byte sequences, returns 2, 3, or 4.
#[inline(always)]
pub fn utf8_char_len(first_byte: u8) -> usize {
    if first_byte & 0x80 == 0 {
        1 // ASCII: 0xxxxxxx
    } else if first_byte & 0xE0 == 0xC0 {
        2 // 2-byte: 110xxxxx
    } else if first_byte & 0xF0 == 0xE0 {
        3 // 3-byte: 1110xxxx
    } else if first_byte & 0xF8 == 0xF0 {
        4 // 4-byte: 11110xxx
    } else {
        1 // Invalid UTF-8, treat as single byte
    }
}

/// Check if a byte is the start of a multi-byte UTF-8 sequence
#[inline(always)]
pub fn is_utf8_continuation(byte: u8) -> bool {
    (byte & 0xC0) == 0x80
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_digit() {
        assert!(CHAR_CLASSES.is_digit(b'0'));
        assert!(CHAR_CLASSES.is_digit(b'5'));
        assert!(CHAR_CLASSES.is_digit(b'9'));
        assert!(!CHAR_CLASSES.is_digit(b'a'));
        assert!(!CHAR_CLASSES.is_digit(b'A'));
        assert!(!CHAR_CLASSES.is_digit(b' '));
    }

    #[test]
    fn test_alpha() {
        assert!(CHAR_CLASSES.is_alpha(b'a'));
        assert!(CHAR_CLASSES.is_alpha(b'z'));
        assert!(CHAR_CLASSES.is_alpha(b'A'));
        assert!(CHAR_CLASSES.is_alpha(b'Z'));
        assert!(!CHAR_CLASSES.is_alpha(b'0'));
        assert!(!CHAR_CLASSES.is_alpha(b'_'));
    }

    #[test]
    fn test_word() {
        assert!(CHAR_CLASSES.is_word(b'a'));
        assert!(CHAR_CLASSES.is_word(b'Z'));
        assert!(CHAR_CLASSES.is_word(b'0'));
        assert!(CHAR_CLASSES.is_word(b'_'));
        assert!(!CHAR_CLASSES.is_word(b' '));
        assert!(!CHAR_CLASSES.is_word(b'-'));
    }

    #[test]
    fn test_hex_digit() {
        assert!(CHAR_CLASSES.is_hex_digit(b'0'));
        assert!(CHAR_CLASSES.is_hex_digit(b'9'));
        assert!(CHAR_CLASSES.is_hex_digit(b'a'));
        assert!(CHAR_CLASSES.is_hex_digit(b'f'));
        assert!(CHAR_CLASSES.is_hex_digit(b'A'));
        assert!(CHAR_CLASSES.is_hex_digit(b'F'));
        assert!(!CHAR_CLASSES.is_hex_digit(b'g'));
        assert!(!CHAR_CLASSES.is_hex_digit(b'G'));
    }

    #[test]
    fn test_space() {
        assert!(CHAR_CLASSES.is_space(b' '));
        assert!(CHAR_CLASSES.is_space(b'\t'));
        assert!(CHAR_CLASSES.is_space(b'\n'));
        assert!(CHAR_CLASSES.is_space(b'\r'));
        assert!(!CHAR_CLASSES.is_space(b'a'));
        assert!(!CHAR_CLASSES.is_space(b'0'));
    }

    #[test]
    fn test_utf8_char_len() {
        // ASCII
        assert_eq!(utf8_char_len(b'a'), 1);
        assert_eq!(utf8_char_len(b'Z'), 1);
        assert_eq!(utf8_char_len(b'0'), 1);

        // 2-byte UTF-8 (e.g., é, ñ)
        assert_eq!(utf8_char_len(0xC3), 2);

        // 3-byte UTF-8 (e.g., 中, 文)
        assert_eq!(utf8_char_len(0xE4), 3);

        // 4-byte UTF-8 (e.g., emoji)
        assert_eq!(utf8_char_len(0xF0), 4);
    }

    #[test]
    fn test_utf8_continuation() {
        assert!(is_utf8_continuation(0x80));
        assert!(is_utf8_continuation(0xBF));
        assert!(!is_utf8_continuation(b'a'));
        assert!(!is_utf8_continuation(0xC0));
    }

    #[test]
    fn test_character_pattern_from_str() {
        // Digit patterns
        assert_eq!(
            CharacterPattern::from_pattern("[0-9]"),
            Some(CharacterPattern::Digit)
        );
        assert_eq!(
            CharacterPattern::from_pattern("\\d"),
            Some(CharacterPattern::Digit)
        );
        assert_eq!(
            CharacterPattern::from_pattern("\\D"),
            Some(CharacterPattern::NonDigit)
        );

        // Word patterns
        assert_eq!(
            CharacterPattern::from_pattern("\\w"),
            Some(CharacterPattern::Word)
        );
        assert_eq!(
            CharacterPattern::from_pattern("\\W"),
            Some(CharacterPattern::NonWord)
        );

        // Space patterns
        assert_eq!(
            CharacterPattern::from_pattern("\\s"),
            Some(CharacterPattern::Space)
        );
        assert_eq!(
            CharacterPattern::from_pattern("\\S"),
            Some(CharacterPattern::NonSpace)
        );

        // Any
        assert_eq!(
            CharacterPattern::from_pattern("."),
            Some(CharacterPattern::Any)
        );

        // Unknown
        assert_eq!(CharacterPattern::from_pattern("[a-z]+"), None);
    }

    #[test]
    fn test_character_pattern_matches() {
        let digit = CharacterPattern::from_pattern("\\d").unwrap();
        assert!(digit.matches(b'0'));
        assert!(digit.matches(b'9'));
        assert!(!digit.matches(b'a'));

        let word = CharacterPattern::from_pattern("\\w").unwrap();
        assert!(word.matches(b'a'));
        assert!(word.matches(b'Z'));
        assert!(word.matches(b'0'));
        assert!(word.matches(b'_'));
        assert!(!word.matches(b' '));

        let non_digit = CharacterPattern::from_pattern("\\D").unwrap();
        assert!(!non_digit.matches(b'0'));
        assert!(non_digit.matches(b'a'));
    }

    #[test]
    fn test_character_pattern_predicate() {
        let space = CharacterPattern::from_pattern("\\s").unwrap();
        let pred = space.predicate();
        assert!(pred(b' '));
        assert!(pred(b'\t'));
        assert!(pred(b'\n'));
        assert!(!pred(b'a'));
    }

    #[test]
    fn test_negation() {
        let digit = CharacterPattern::Digit;
        let non_digit = CharacterPattern::NonDigit;
        assert!(digit.is_negation_of(&non_digit));
        assert!(non_digit.is_negation_of(&digit));

        let word = CharacterPattern::Word;
        let non_word = CharacterPattern::NonWord;
        assert!(word.is_negation_of(&non_word));

        assert!(!digit.is_negation_of(&word));
    }

    #[test]
    fn test_char_ranges_decode() {
        // The shape the PARG compiler emits for `%x00-10FFFF`.
        let all = CharRanges::from_pattern("[\\x00-\\u{10ffff}]").unwrap();
        assert_eq!(all.match_at(b"x", 0), Some(1));
        assert_eq!(all.match_at("é".as_bytes(), 0), Some(2));
        assert_eq!(all.match_at(b"", 0), None);

        // Negation is resolved by the HIR.
        let no_quote = CharRanges::from_pattern("[^\\\\\"]").unwrap();
        assert_eq!(no_quote.match_at(b"a", 0), Some(1));
        assert_eq!(no_quote.match_at(b"\"", 0), None);
        assert_eq!(no_quote.match_at(b"\\", 0), None);

        // Ranges, case-insensitive flags, and unicode categories all
        // decode to direct membership.
        let hex = CharRanges::from_pattern("(?i)[0-9a-f]").unwrap();
        assert_eq!(hex.match_at(b"A", 0), Some(1));
        assert_eq!(hex.match_at(b"g", 0), None);

        let letters = CharRanges::from_pattern("[\\p{L}]+");
        // `+` is a repetition, not a pure class: must NOT decode.
        assert!(letters.is_none());

        // Non-class patterns keep the regex engine.
        assert!(CharRanges::from_pattern("[a-z]+").is_none());
        assert!(CharRanges::from_pattern("(a|b)").is_none());
        assert!(CharRanges::from_pattern("^a").is_none());
    }

    #[test]
    fn test_char_ranges_matches_regex_engine() {
        // Membership must agree with the regex crate for the same
        // pattern, per character, across the ASCII block and a sample of
        // multi-byte planes.
        let class = CharRanges::from_pattern("[\\x00-\\u{00FF}\\u{4E00}-\\u{9FFF}]").unwrap();
        let re = regex::Regex::new("[\\x00-\\u{00FF}\\u{4E00}-\\u{9FFF}]").unwrap();
        let text = "aZ9 é字\u{FFFE}\u{10FFFE}";
        let bytes = text.as_bytes();
        for (pos, ch) in text.char_indices() {
            let expected = re.is_match(ch.encode_utf8(&mut [0u8; 4]));
            assert_eq!(
                class.match_at(bytes, pos),
                expected.then(|| ch.len_utf8()),
                "mismatch at {pos} for {ch:?}"
            );
        }
    }
}
