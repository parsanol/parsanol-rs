//! The source-level rule table: rule definitions, their doc comments,
//! signatures, and every reference — extracted from the .parg text.
//! The compiled artifact carries atoms, not source spans, so the LSP's
//! navigation features read the document text directly.

use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub struct RuleDef {
    /// The rule name.
    pub name: String,
    /// The definition line's `name` occurrence (0-based byte offsets).
    pub name_start: usize,
    pub name_end: usize,
    /// The whole rule's first line (the signature).
    pub line_start: usize,
    pub line_end: usize,
    /// The 0-based line index of the definition.
    pub line: usize,
    /// The consecutive `#`/`##` comment lines immediately above.
    pub docs: Vec<String>,
}

/// One name occurrence at an offset (a definition or a reference).
#[derive(Debug, Clone)]
pub struct Occurrence {
    pub name: String,
    pub start: usize,
    pub end: usize,
    pub is_def: bool,
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Every identifier occurrence that matches a known rule name, with
/// the definition flag keyed on the following non-space char `=`.
pub fn occurrences(source: &str, names: &[String]) -> Vec<Occurrence> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for names_iter in 0..1 {
        let _ = names_iter;
        let mut i = 0usize;
        while i < bytes.len() {
            // Skip strings and comments so their contents never match.
            let b = bytes[i];
            if b == b'"' {
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                i += 1;
                continue;
            }
            if b == b'#' {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            if is_ident_byte(b) && (i == 0 || !is_ident_byte(bytes[i - 1])) {
                let start = i;
                while i < bytes.len() && is_ident_byte(bytes[i]) {
                    i += 1;
                }
                let word = &source[start..i];
                if !names.iter().any(|n| n == word) {
                    continue;
                }
                // A definition: the next non-space char is `=`.
                let mut j = i;
                while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                    j += 1;
                }
                let is_def = j < bytes.len() && bytes[j] == b'=';
                out.push(Occurrence {
                    name: word.to_string(),
                    start,
                    end: i,
                    is_def,
                });
                continue;
            }
            i += 1;
        }
    }
    out
}

/// The rule definitions with their doc blocks.
pub fn rule_defs(source: &str) -> Vec<RuleDef> {
    let names: Vec<String> = source
        .lines()
        .filter_map(|l| {
            let t = l.trim_start();
            let name = t.split('=').next()?.trim();
            let name = name.split_whitespace().next()?;
            let name = name.trim();
            if name.is_empty()
                || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                || !name.bytes().next().is_some_and(|b| b.is_ascii_lowercase())
            {
                return None;
            }
            // The entry lines use `:`; only `=` lines define rules.
            Some(name.to_string())
        })
        .collect();
    let mut out = Vec::new();
    for occ in occurrences(source, &names).into_iter().filter(|o| o.is_def) {
        // The definition's line + the doc block above.
        let line_start = source[..occ.start].rfind('\n').map(|p| p + 1).unwrap_or(0);
        let line_end = source[occ.start..]
            .find('\n')
            .map(|p| occ.start + p)
            .unwrap_or(source.len());
        let line_no = source[..occ.start].matches('\n').count();
        let mut docs = Vec::new();
        let mut cursor = line_start;
        loop {
            let prev_end = cursor
                .checked_sub(1)
                .and_then(|_| source[..cursor.saturating_sub(1)].rfind('\n'))
                .map(|p| p + 1);
            let Some(ps) = prev_end else { break };
            let line = source[ps..cursor - 1].trim();
            if line.starts_with('#') {
                docs.push(line.trim_start_matches('#').trim().to_string());
                cursor = ps;
            } else {
                break;
            }
        }
        docs.reverse();
        out.push(RuleDef {
            name: occ.name.clone(),
            name_start: occ.start,
            name_end: occ.end,
            line_start,
            line_end,
            line: line_no,
            docs,
        });
    }
    out.sort_by_key(|r| r.name_start);
    out
}

pub fn defs_of<'a>(defs: &'a [RuleDef], name: &str) -> Vec<&'a RuleDef> {
    defs.iter().filter(|r| r.name == name).collect()
}

/// LSP Position for a byte offset (UTF-16-free: the LSP tolerates
/// line/character pairs; the character column = the UTF-8 byte column,
/// which matches for the ASCII-dominant grammar sources).
pub fn position_of(source: &str, offset: usize) -> Value {
    let clamped = offset.min(source.len());
    let before = &source[..clamped];
    let line = before.matches('\n').count();
    let col = before.len() - before.rfind('\n').map(|p| p + 1).unwrap_or(0);
    json!({ "line": line, "character": col })
}

pub fn range_of(source: &str, start: usize, end: usize) -> Value {
    json!({ "start": position_of(source, start), "end": position_of(source, end) })
}

/// The word at an offset (the identifier containing it).
pub fn word_at(source: &str, offset: usize) -> Option<(String, usize, usize)> {
    let bytes = source.as_bytes();
    let offset = offset.min(bytes.len());
    if offset > 0
        && !is_ident_byte(bytes[offset - 1])
        && (offset >= bytes.len() || !is_ident_byte(bytes[offset]))
    {
        return None;
    }
    let mut start = offset;
    while start > 0 && is_ident_byte(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = offset.min(bytes.len());
    while end < bytes.len() && is_ident_byte(bytes[end]) {
        end += 1;
    }
    if start == end {
        return None;
    }
    Some((source[start..end].to_string(), start, end))
}

/// The keyword completions of the grammar language.
pub const KEYWORDS: &[&str] = &[
    "grammar", "version", "entry", "test", "accept", "reject", "mode", "edition", "as", "use",
    "import",
];

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "grammar G version \"1.0.0\" {\n  # digit docs\n  digit = %x30-39\n  x_digit = digit / \"X\"\n  entry main: x_digit\n}\n";

    #[test]
    fn defs_and_references() {
        let defs = rule_defs(DOC);
        assert_eq!(defs.len(), 2, "digit + x_digit");
        assert_eq!(defs[0].name, "digit");
        assert_eq!(defs[0].docs, vec!["digit docs"]);
        let occ = occurrences(DOC, &["digit".to_string(), "x_digit".to_string()]);
        let digit_refs: Vec<_> = occ.iter().filter(|o| o.name == "digit").collect();
        // The def + the x_digit reference.
        assert_eq!(digit_refs.len(), 2);
        assert!(digit_refs[0].is_def);
        assert!(!digit_refs[1].is_def);
    }

    #[test]
    fn word_and_position() {
        let (w, s, _e) = word_at(DOC, DOC.find("digit /").unwrap()).unwrap();
        assert_eq!(w, "digit");
        let pos = position_of(DOC, s);
        assert_eq!(pos["line"], 3);
    }
}
