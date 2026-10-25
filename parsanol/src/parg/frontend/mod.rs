//! The PARG frontend (rs#298): PARG source → artifact, byte-parity
//! with the Ruby compiler. Slice 1 = the lexer.
//!
//! Token-for-token parity with `parsanol-ruby`'s lexer.rb is the
//! contract: same alternation order, same dropped classes (whitespace,
//! `#` comments), same value derivations (`%x…` strips the `%x`, and
//! so on), same CHARACTER-based offsets. The parity corpus
//! (`parsanol/tests/fixtures/token_parity`) holds the Ruby streams.
pub mod compile;
pub mod parser;
pub use parser::{parse as parse_source, Document, ParseError};

/// Lexical error: the offset (in characters) and the offending char.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    /// Character offset of the offending character.
    pub offset: usize,
    /// The character (or a description, at end of input).
    pub found: String,
}

impl std::fmt::Display for LexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "unexpected character {:?} at offset {}",
            self.found, self.offset
        )
    }
}

impl std::error::Error for LexError {}

/// Lexical token: kind, derived value, character offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// The lexical class (the Ruby name is the parity wire).
    pub kind: TokenKind,
    /// The derived value: `%x30-39` carries `30-39`, `%i"iso"` carries
    /// `"iso"`, everything else carries the matched text.
    pub value: String,
    /// Character offset in the source (Ruby's regex offsets).
    pub offset: usize,
}

/// The lexical classes, in the Ruby lexer's alternation order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// `\r\n` or `\n` (kept — the Ruby lexer keeps newlines).
    Newline,
    /// `## …` doc comment (ships in the artifact).
    Doc,
    /// `%x…` — value strips the `%x`.
    Hex,
    /// `%p{…}` — value keeps the braces.
    Category,
    /// `%i"…"` — value strips the `%i`.
    Istring,
    /// `%s"…"` — value strips the `%s`.
    Sstring,
    /// `"…"` — value keeps the quotes.
    Str,
    /// `->`
    Arrow,
    /// `[0-9]+`
    Num,
    /// `[A-Za-z_][A-Za-z0-9_]*`
    Ident,
    /// One of `= / ( ) [ ] { } * ! & : , . < -`
    Punct,
}

impl TokenKind {
    /// The Ruby lexer's name for the kind — the parity wire.
    pub fn as_ruby_name(self) -> &'static str {
        match self {
            TokenKind::Newline => "newline",
            TokenKind::Doc => "doc",
            TokenKind::Hex => "hex",
            TokenKind::Category => "category",
            TokenKind::Istring => "istr",
            TokenKind::Sstring => "sstr",
            TokenKind::Str => "str",
            TokenKind::Arrow => "arrow",
            TokenKind::Num => "num",
            TokenKind::Ident => "ident",
            TokenKind::Punct => "punct",
        }
    }
}

/// Lex `source` into the Ruby compiler's token stream.
pub fn lex(source: &str) -> Result<Vec<Token>, LexError> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0usize; // char index
    let mut offset = 0usize; // ruby offsets are character offsets

    // A quoted string body: `\\. | [^"\\]` per the Ruby regex, then
    // the closing quote. Returns the raw text INCLUDING quotes.
    let string_body = |i: &mut usize, chars: &[char], offset: &mut usize| -> Result<(), LexError> {
        // caller consumed the opening quote already
        loop {
            let Some(&ch) = chars.get(*i) else {
                return Err(LexError {
                    offset: *offset,
                    found: "end of input in string".to_string(),
                });
            };
            *i += 1;
            *offset += 1;
            if ch == '"' {
                return Ok(());
            }
            if ch == '\\' {
                let Some(&escaped) = chars.get(*i) else {
                    return Err(LexError {
                        offset: *offset,
                        found: "end of input in escape".to_string(),
                    });
                };
                let _ = escaped; // consumed for the offset; the value keeps the raw text
                *i += 1;
                *offset += 1;
            }
        }
    };

    while i < chars.len() {
        let ch = chars[i];
        let start = i;
        let start_offset = offset;

        // 1. newline: \r\n or \n
        if ch == '\n' || (ch == '\r' && chars.get(i + 1) == Some(&'\n')) {
            let len = if ch == '\r' { 2 } else { 1 };
            i += len;
            offset += len;
            let value: String = chars[start..i].iter().collect();
            tokens.push(Token {
                kind: TokenKind::Newline,
                value,
                offset: start_offset,
            });
            continue;
        }
        // 2. ws (dropped)
        if ch == ' ' || ch == '\t' {
            while i < chars.len() && (chars[i] == ' ' || chars[i] == '\t') {
                i += 1;
                offset += 1;
            }
            continue;
        }
        // 3/4. doc and comment
        if ch == '#' {
            let is_doc = chars.get(i + 1) == Some(&'#');
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
                offset += 1;
            }
            if is_doc {
                tokens.push(Token {
                    kind: TokenKind::Doc,
                    value: chars[start..i].iter().collect(),
                    offset: start_offset,
                });
            }
            continue;
        }
        // 5-8. the % family
        if ch == '%' {
            let Some(&next) = chars.get(i + 1) else {
                return Err(LexError {
                    offset: start_offset,
                    found: "%".to_string(),
                });
            };
            match next {
                'x' => {
                    let mut j = i + 2;
                    let mut count = 0;
                    while count < 6 && chars.get(j).is_some_and(|c| c.is_ascii_hexdigit()) {
                        j += 1;
                        count += 1;
                    }
                    if count < 2 {
                        return Err(LexError {
                            offset: start_offset,
                            found: "%x".to_string(),
                        });
                    }
                    if chars.get(j) == Some(&'-') {
                        j += 1;
                        let mut hi = 0;
                        while hi < 6 && chars.get(j).is_some_and(|c| c.is_ascii_hexdigit()) {
                            j += 1;
                            hi += 1;
                        }
                        if hi < 2 {
                            return Err(LexError {
                                offset: start_offset,
                                found: "%x-".to_string(),
                            });
                        }
                    } else if chars.get(j) == Some(&'.') {
                        while chars.get(j) == Some(&'.') {
                            j += 1;
                            let mut part = 0;
                            while part < 6 && chars.get(j).is_some_and(|c| c.is_ascii_hexdigit()) {
                                j += 1;
                                part += 1;
                            }
                            if part < 2 {
                                return Err(LexError {
                                    offset: start_offset,
                                    found: "%x.".to_string(),
                                });
                            }
                        }
                    }
                    let value: String = chars[i + 2..j].iter().collect();
                    offset += j - i;
                    i = j;
                    tokens.push(Token {
                        kind: TokenKind::Hex,
                        value,
                        offset: start_offset,
                    });
                    continue;
                }
                'p' if chars.get(i + 2) == Some(&'{') => {
                    let mut j = i + 3;
                    let mut count = 0;
                    while count < 2 && chars.get(j).is_some_and(|c| c.is_ascii_alphabetic()) {
                        j += 1;
                        count += 1;
                    }
                    if count == 0 || chars.get(j) != Some(&'}') {
                        return Err(LexError {
                            offset: start_offset,
                            found: "%p".to_string(),
                        });
                    }
                    j += 1;
                    let value: String = chars[i + 2..j].iter().collect();
                    offset += j - i;
                    i = j;
                    tokens.push(Token {
                        kind: TokenKind::Category,
                        value,
                        offset: start_offset,
                    });
                    continue;
                }
                'i' | 's' if chars.get(i + 2) == Some(&'"') => {
                    let kind = if next == 'i' {
                        TokenKind::Istring
                    } else {
                        TokenKind::Sstring
                    };
                    let mut j = i + 3;
                    let mut off = offset + 3;
                    string_body(&mut j, &chars, &mut off)?;
                    let value: String = chars[i + 2..j].iter().collect();
                    offset += j - i;
                    i = j;
                    tokens.push(Token {
                        kind,
                        value,
                        offset: start_offset,
                    });
                    continue;
                }
                _ => {
                    return Err(LexError {
                        offset: start_offset,
                        found: "%".to_string(),
                    })
                }
            }
        }
        // 9. string
        if ch == '"' {
            let mut j = i + 1;
            let mut off = offset + 1;
            string_body(&mut j, &chars, &mut off)?;
            let value: String = chars[i..j].iter().collect();
            offset += j - i;
            i = j;
            tokens.push(Token {
                kind: TokenKind::Str,
                value,
                offset: start_offset,
            });
            continue;
        }
        // 10. arrow (before punct: `-` is also a punct char)
        if ch == '-' && chars.get(i + 1) == Some(&'>') {
            i += 2;
            offset += 2;
            tokens.push(Token {
                kind: TokenKind::Arrow,
                value: "->".to_string(),
                offset: start_offset,
            });
            continue;
        }
        // 11. num
        if ch.is_ascii_digit() {
            let mut j = i;
            while chars.get(j).is_some_and(|c| c.is_ascii_digit()) {
                j += 1;
            }
            let value: String = chars[i..j].iter().collect();
            offset += j - i;
            i = j;
            tokens.push(Token {
                kind: TokenKind::Num,
                value,
                offset: start_offset,
            });
            continue;
        }
        // 12. ident
        if ch.is_ascii_alphabetic() || ch == '_' {
            let mut j = i;
            while chars
                .get(j)
                .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_')
            {
                j += 1;
            }
            let value: String = chars[i..j].iter().collect();
            offset += j - i;
            i = j;
            tokens.push(Token {
                kind: TokenKind::Ident,
                value,
                offset: start_offset,
            });
            continue;
        }
        // 13. punct
        if matches!(
            ch,
            '=' | '/'
                | '('
                | ')'
                | '['
                | ']'
                | '{'
                | '}'
                | '*'
                | '!'
                | '&'
                | ':'
                | ','
                | '.'
                | '<'
                | '-'
        ) {
            i += 1;
            offset += 1;
            tokens.push(Token {
                kind: TokenKind::Punct,
                value: ch.to_string(),
                offset: start_offset,
            });
            continue;
        }

        return Err(LexError {
            offset: start_offset,
            found: ch.to_string(),
        });
    }

    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexes_the_canonical_skeleton() {
        let tokens = lex("grammar T version \"1\" {\n  x = %x30-39\n}\n").unwrap();
        let kinds: Vec<_> = tokens.iter().map(|t| t.kind.as_ruby_name()).collect();
        assert_eq!(
            kinds,
            vec![
                "ident", "ident", "ident", "str", "punct", "newline", "ident", "punct", "hex",
                "newline", "punct", "newline"
            ]
        );
        let hex = tokens.iter().find(|t| t.kind == TokenKind::Hex).unwrap();
        assert_eq!(hex.value, "30-39");
    }

    #[test]
    fn offsets_count_characters_not_bytes() {
        // Multibyte text lives in strings (bare CJK lexes in neither
        // engine): "日本語" = 5 chars / 9 bytes, so `x` sits at char 6
        // — a byte offset would say 10.
        let tokens = lex("\"日本語\" x").unwrap();
        let x = tokens.iter().find(|t| t.value == "x").unwrap();
        assert_eq!(x.offset, 6);
        let string = tokens.iter().find(|t| t.kind == TokenKind::Str).unwrap();
        assert_eq!(string.value, "\"日本語\"");
    }

    #[test]
    fn errors_name_the_offender() {
        let err = lex("x = %d15").unwrap_err();
        assert_eq!(err.offset, 4);
        assert_eq!(err.found, "%");
    }
}
