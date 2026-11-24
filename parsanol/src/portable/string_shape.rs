//! The string-scan shape (rs#574 option A, the value-noise finding's
//! decision): a quote-delimited named rule whose body is
//! `quote *( interior ) quote` emits ONE whole-string span instead of
//! the per-char iteration spans the v2 wire used to carry. The
//! detector and the escape-aware scan kernel live HERE so the bytecode
//! compiler/VM and the classic walker share one shape contract and one
//! kernel — engine parity by construction.
//!
//! Kernel semantics mirror the classic lowering end state exactly:
//! the interior's class arms and the escape pair are FIRST-disjoint
//! (verified at detection), so the ordered choice collapses into the
//! union; a bad interior byte, a short escape form, or a missing
//! closing quote fails the whole rule at the deepest scanned offset —
//! the same failure surface the choice loop leaves behind.

use crate::portable::char_class::{normalize_ranges, utf8_char_len, CharRanges};
use crate::portable::grammar::{Atom, Grammar, RepetitionTag};

/// One post-escape counted form: `lead min*( ranges )` — json's
/// `%x75 4*( hex )` u-hexquad is the canonical member. `count` is the
/// mandatory prefix; when the grammar allows more than `count`
/// iterations (an unbounded or bounded-over-min repetition), the
/// extras fall through to the plain run, which accepts them whenever
/// the counted ranges sit inside the interior union (checked at
/// detection).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CountedEscape {
    /// The form's lead codepoint (e.g. `u`).
    pub lead: u32,
    /// The mandatory iteration count the kernel consumes.
    pub count: u32,
    /// The codepoint ranges each iteration accepts.
    pub ranges: Vec<(u32, u32)>,
    /// True when the grammar's repetition allows MORE than `count`
    /// iterations (detection then requires the counted ranges inside
    /// the interior union).
    #[serde(default)]
    pub open_ended: bool,
}

/// The escape pair arm: `escape_byte ( literal / counted )`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EscapeSpec {
    /// The escape codepoint (e.g. backslash).
    pub byte: u32,
    /// Single-codepoint post-escape literals (json's nine two-char
    /// forms minus the counted one).
    pub literals: Vec<u32>,
    /// At most one counted form.
    pub counted: Option<CountedEscape>,
}

/// The whole-string shape spec: open/close quote, the interior class
/// union, and the optional escape pair.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StringSpanSpec {
    /// The quote codepoint; opens and closes the span.
    pub quote: u32,
    /// Codepoint ranges accepted between escapes (the class arms'
    /// union; the quote and the escape byte are excluded by
    /// construction).
    pub interior: Vec<(u32, u32)>,
    /// The escape pair, when the interior has one.
    pub escape: Option<EscapeSpec>,
}

#[inline]
fn in_ranges(cp: u32, ranges: &[(u32, u32)]) -> bool {
    ranges.iter().any(|&(lo, hi)| cp >= lo && cp <= hi)
}

/// Decode the codepoint at `pos`. None = truncated/invalid sequence.
#[inline]
fn decode_at(input: &[u8], pos: usize) -> Option<(u32, usize)> {
    let b = *input.get(pos)?;
    let len = utf8_char_len(b);
    if pos + len > input.len() {
        return None;
    }
    let cp = if b < 0x80 {
        b as u32
    } else if b < 0xE0 {
        ((u32::from(b) & 0x1F) << 6) | (u32::from(input[pos + 1]) & 0x3F)
    } else if b < 0xF0 {
        ((u32::from(b) & 0x0F) << 12)
            | ((u32::from(input[pos + 1]) & 0x3F) << 6)
            | (u32::from(input[pos + 2]) & 0x3F)
    } else {
        ((u32::from(b) & 0x07) << 18)
            | ((u32::from(input[pos + 1]) & 0x3F) << 12)
            | ((u32::from(input[pos + 2]) & 0x3F) << 6)
            | (u32::from(input[pos + 3]) & 0x3F)
    };
    Some((cp, len))
}

impl StringSpanSpec {
    /// Scan a whole-string span from `start` (at the open quote).
    /// `Ok(end)` = the offset just past the closing quote;
    /// `Err(pos)` = the deepest offset the scan reached when the rule
    /// must fail (a missing quote, a bad interior byte, a short
    /// escape form, or end of input).
    pub fn scan(&self, input: &[u8], start: usize) -> Result<usize, usize> {
        let (cp, len) = decode_at(input, start).ok_or(start)?;
        if cp != self.quote {
            return Err(start);
        }
        let mut pos = start + len;
        loop {
            let b = match input.get(pos) {
                Some(&b) => b,
                None => return Err(pos),
            };
            // ASCII fast path: quote/escape membership on the raw
            // byte, the interior check without a decode. Valid while
            // quote and escape are themselves ASCII — the detector
            // routes the general decode path below otherwise.
            if b < 0x80 && self.quote < 0x80 && self.escape.as_ref().is_none_or(|e| e.byte < 0x80) {
                let cp = u32::from(b);
                if cp == self.quote {
                    return Ok(pos + 1);
                }
                if let Some(esc) = &self.escape {
                    if cp == esc.byte {
                        pos = self.scan_escape(input, pos + 1, esc)?;
                        continue;
                    }
                }
                if in_ranges(cp, &self.interior) {
                    pos += 1;
                    continue;
                }
                return Err(pos);
            }
            let (cp, len) = decode_at(input, pos).ok_or(pos)?;
            if cp == self.quote {
                return Ok(pos + len);
            }
            if let Some(esc) = &self.escape {
                if cp == esc.byte {
                    pos = self.scan_escape(input, pos + len, esc)?;
                    continue;
                }
            }
            if in_ranges(cp, &self.interior) {
                pos += len;
                continue;
            }
            return Err(pos);
        }
    }

    /// Consume one post-escape form starting at `pos` (just past the
    /// escape byte). Returns the offset of the next interior byte.
    fn scan_escape(&self, input: &[u8], pos: usize, esc: &EscapeSpec) -> Result<usize, usize> {
        let (cp, len) = decode_at(input, pos).ok_or(pos)?;
        if esc.literals.contains(&cp) {
            return Ok(pos + len);
        }
        if let Some(counted) = &esc.counted {
            if cp == counted.lead {
                let mut p = pos + len;
                for _ in 0..counted.count {
                    let (h, hl) = decode_at(input, p).ok_or(p)?;
                    if !in_ranges(h, &counted.ranges) {
                        return Err(p);
                    }
                    p += hl;
                }
                return Ok(p);
            }
        }
        Err(pos)
    }
}

/// Follow Entity chains to the underlying atom — the shape hides
/// behind rule-ref wrappers.
fn resolve(grammar: &Grammar, mut idx: usize) -> Option<&Atom> {
    for _ in 0..64 {
        match grammar.get_atom(idx)? {
            Atom::Entity { atom } => idx = *atom,
            a => return Some(a),
        }
    }
    None
}

/// The codepoint of a single-codepoint atom: a one-char Str or a
/// one-codepoint class Re.
fn single_cp(grammar: &Grammar, idx: usize) -> Option<u32> {
    match resolve(grammar, idx)? {
        Atom::Str { pattern } => {
            let mut chars = pattern.chars();
            let (c, extra) = (chars.next()?, chars.next());
            extra.is_none().then_some(c as u32)
        }
        Atom::Re { pattern } => {
            let ranges = CharRanges::from_pattern(pattern)?;
            let cps = ranges.codepoint_ranges();
            (cps.len() == 1 && cps[0].0 == cps[0].1).then_some(cps[0].0)
        }
        _ => None,
    }
}

/// The class union for a bare class / choice-of-classes atom — the
/// same extraction the ScanClass gate uses, ported so the shared
/// detector has no compiler dependency.
fn class_union(grammar: &Grammar, idx: usize, depth: u8) -> Option<Vec<(u32, u32)>> {
    if depth > 64 {
        return None;
    }
    match resolve(grammar, idx)? {
        Atom::Re { pattern } => Some(CharRanges::from_pattern(pattern)?.codepoint_ranges()),
        Atom::Str { pattern } => {
            let mut chars = pattern.chars();
            let (c, extra) = (chars.next()?, chars.next());
            extra.is_none().then(|| vec![(c as u32, c as u32)])
        }
        Atom::Alternative { atoms } => {
            let mut union = Vec::new();
            for &a in atoms {
                union.extend(class_union(grammar, a, depth + 1)?);
            }
            Some(normalize_ranges(union))
        }
        _ => None,
    }
}

/// The counted form `lead min*( class )` behind a Sequence of exactly
/// [lead, Repetition { min == n }] — exact (max == min) or open-ended
/// (max None / Some(m > min), ABNF's `4*` family).
fn counted_form(grammar: &Grammar, idx: usize, depth: u8) -> Option<CountedEscape> {
    if depth > 64 {
        return None;
    }
    let atoms = match resolve(grammar, idx)? {
        Atom::Sequence { atoms } if atoms.len() == 2 => atoms.clone(),
        _ => return None,
    };
    let lead = single_cp(grammar, atoms[0])?;
    match resolve(grammar, atoms[1])? {
        Atom::Repetition {
            atom,
            min,
            max,
            tag: RepetitionTag::Repetition,
        } if *min >= 1 => {
            let open_ended = match max {
                None => true,
                Some(m) => *m > *min,
            };
            let ranges = class_union(grammar, *atom, depth + 1)?;
            Some(CountedEscape {
                lead,
                count: *min as u32,
                ranges,
                open_ended,
            })
        }
        _ => None,
    }
}

/// The escape pair arm `escape_byte ( post-escape forms )`. The post
/// body is an alternative of single codepoints plus at most one
/// counted form (json's shape), or one such form alone.
fn escape_arm(grammar: &Grammar, idx: usize, depth: u8) -> Option<EscapeSpec> {
    if depth > 64 {
        return None;
    }
    let atoms = match resolve(grammar, idx)? {
        Atom::Sequence { atoms } if atoms.len() == 2 => atoms.clone(),
        _ => return None,
    };
    let byte = single_cp(grammar, atoms[0])?;
    let post = atoms[1];
    let mut literals = Vec::new();
    let mut counted = None;
    // Flattened walk of the post-escape choice: nested alternatives
    // of single codepoints fold into the literal set (the ordered
    // choice over FIRST-disjoint literals is their union).
    let mut queue = vec![post];
    while let Some(node) = queue.pop() {
        match grammar.get_atom(node) {
            Some(Atom::Alternative { atoms: arms }) => {
                queue.extend(arms.iter().copied());
            }
            _ => {
                if let Some(cp) = single_cp(grammar, node) {
                    literals.push(cp);
                } else if counted.is_none() {
                    counted = Some(counted_form(grammar, node, depth + 1)?);
                } else {
                    return None;
                }
            }
        }
    }
    Some(EscapeSpec {
        byte,
        literals,
        counted,
    })
}

/// Detect the whole-string shape on a named rule's body atom.
/// `named_atom_idx` = the atom INSIDE `Atom::Named` (the rule body).
///
/// Body = `quote Repetition{0,∞}( interior ) quote`, the interior an
/// alternative of class arms plus at most one escape pair whose
/// post-escape forms are single codepoints or one counted form. The
/// quote and the escape byte must be excluded from every consumed
/// range — the boundary law: the scan stops at the first unescaped
/// quote, exactly like the choice loop it replaces.
pub fn detect(grammar: &Grammar, named_atom_idx: usize) -> Option<StringSpanSpec> {
    let atoms = match resolve(grammar, named_atom_idx)? {
        Atom::Sequence { atoms } if atoms.len() == 3 => atoms.clone(),
        _ => return None,
    };
    let quote = single_cp(grammar, atoms[0])?;
    if single_cp(grammar, atoms[2])? != quote {
        return None;
    }
    let (interior_idx, min, max, tag) = match resolve(grammar, atoms[1])? {
        Atom::Repetition {
            atom,
            min,
            max,
            tag,
        } => (*atom, *min, *max, *tag),
        _ => return None,
    };
    if min != 0 || max.is_some() || tag != RepetitionTag::Repetition {
        return None;
    }

    let mut ranges = Vec::new();
    let mut escape = None;
    if let Some(Atom::Alternative { atoms: arms }) = grammar.get_atom(interior_idx) {
        for &arm in arms.clone().iter() {
            if let Some(r) = class_union(grammar, arm, 0) {
                ranges.extend(r);
            } else if escape.is_none() {
                escape = Some(escape_arm(grammar, arm, 0)?);
            } else {
                return None;
            }
        }
    } else if let Some(r) = class_union(grammar, interior_idx, 0) {
        ranges.extend(r);
    } else {
        escape = Some(escape_arm(grammar, interior_idx, 0)?);
    }

    let interior = normalize_ranges(ranges);
    // The boundary law: the plain run must not admit the quote (the
    // scan stops at the first unescaped quote, exactly like the
    // choice loop) nor the escape byte (an admitted backslash would
    // let a plain arm outrun the escape arm and diverge from the
    // ordered choice). Post-escape literals may include the quote —
    // the escaped-quote form is the escape pair's whole point.
    if let Some(esc) = &escape {
        if esc.byte == quote || in_ranges(esc.byte, &interior) {
            return None;
        }
        // An open-ended counted form consumes past its mandatory
        // count in the classic loop; the kernel's extras fall through
        // to the plain run, so every counted range must sit inside
        // the interior (json's hex ⊂ the plain run).
        if let Some(counted) = &esc.counted {
            if counted.open_ended
                && !counted
                    .ranges
                    .iter()
                    .all(|&(lo, hi)| interior.iter().any(|&(ilo, ihi)| lo >= ilo && hi <= ihi))
            {
                return None;
            }
        }
    }
    if in_ranges(quote, &interior) {
        return None;
    }
    Some(StringSpanSpec {
        quote,
        interior,
        escape,
    })
}

/// Membership probe for the routing refusals: does this atom resolve
/// (through Entity refs and Named wraps) to a whole-string shape? A
/// hit means the rule's lowering is the kernel's — a routed table
/// inlining the rule's atoms would resurrect the per-char wire.
pub fn is_string_shaped(grammar: &Grammar, atom_idx: usize) -> bool {
    let mut cur = atom_idx;
    for _ in 0..64 {
        match grammar.get_atom(cur) {
            Some(Atom::Entity { atom }) => cur = *atom,
            Some(Atom::Named { atom, .. }) => cur = *atom,
            _ => break,
        }
    }
    detect(grammar, cur).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The json string rule's shape, hand-built:
    /// `( %x22 *( plain / %x5C post ) %x22 )` with post = the nine
    /// two-char forms (the u-hexquad counted).
    fn json_string_grammar() -> (Grammar, usize) {
        let mut g = Grammar::new();
        let quote = g.add_atom(Atom::Str {
            pattern: "\"".to_string(),
        });
        let plain = g.add_atom(Atom::Re {
            pattern: "[\\x20\\x21\\x23-\\x5b\\x5d-\\x{10ffff}]".to_string(),
        });
        let esc = g.add_atom(Atom::Str {
            pattern: "\\".to_string(),
        });
        let mk_lit = |g: &mut Grammar, c: &str| {
            g.add_atom(Atom::Str {
                pattern: c.to_string(),
            })
        };
        let l_quote = mk_lit(&mut g, "\"");
        let l_bslash = mk_lit(&mut g, "\\");
        let l_slash = mk_lit(&mut g, "/");
        let l_b = mk_lit(&mut g, "b");
        let l_f = mk_lit(&mut g, "f");
        let l_n = mk_lit(&mut g, "n");
        let l_r = mk_lit(&mut g, "r");
        let l_t = mk_lit(&mut g, "t");
        let l_u = mk_lit(&mut g, "u");
        let lits = g.add_atom(Atom::Alternative {
            atoms: vec![l_quote, l_bslash, l_slash, l_b, l_f, l_n, l_r, l_t],
        });
        let hex = g.add_atom(Atom::Re {
            pattern: "[0-9a-fA-F]".to_string(),
        });
        let hexquad = g.add_atom(Atom::Repetition {
            atom: hex,
            min: 4,
            max: Some(4),
            tag: RepetitionTag::Repetition,
        });
        let u_form = g.add_atom(Atom::Sequence {
            atoms: vec![l_u, hexquad],
        });
        let post = g.add_atom(Atom::Alternative {
            atoms: vec![lits, u_form],
        });
        let escape_pair = g.add_atom(Atom::Sequence {
            atoms: vec![esc, post],
        });
        let interior = g.add_atom(Atom::Alternative {
            atoms: vec![plain, escape_pair],
        });
        let rep = g.add_atom(Atom::Repetition {
            atom: interior,
            min: 0,
            max: None,
            tag: RepetitionTag::Repetition,
        });
        let body = g.add_atom(Atom::Sequence {
            atoms: vec![quote, rep, quote],
        });
        let named = g.add_atom(Atom::Named {
            name: "string".to_string(),
            atom: body,
        });
        g.root = named;
        (g, body)
    }

    #[test]
    fn detects_the_json_shape() {
        let (g, body) = json_string_grammar();
        let spec = detect(&g, body).expect("json's string rule is the canonical shape");
        assert_eq!(spec.quote, 0x22);
        // The plain-run union: quote and backslash excluded.
        assert!(!in_ranges(0x22, &spec.interior));
        assert!(!in_ranges(0x5C, &spec.interior));
        assert!(in_ranges(0x20, &spec.interior));
        assert!(in_ranges(0x7E, &spec.interior));
        assert!(in_ranges(0x10_FFFF, &spec.interior));
        let esc = spec.escape.as_ref().expect("the escape pair detected");
        assert_eq!(esc.byte, 0x5C);
        assert_eq!(esc.literals.len(), 8);
        let counted = esc.counted.as_ref().expect("the u-hexquad detected");
        assert_eq!(counted.lead, u32::from(b'u'));
        assert_eq!(counted.count, 4);
    }

    #[test]
    fn scans_plain_escaped_and_counted() {
        let (g, body) = json_string_grammar();
        let spec = detect(&g, body).unwrap();
        // Plain.
        assert_eq!(spec.scan(b"\"abc\"", 0), Ok(5));
        // Empty string.
        assert_eq!(spec.scan(b"\"\"", 0), Ok(2));
        // Escaped quote + backslash + the counted form, one span.
        let s = "\"a\\\"b\\\\c\\u0041\"".as_bytes();
        assert_eq!(spec.scan(s, 0), Ok(s.len()));
        // Multibyte interior (the plain run reaches 0x10FFFF).
        let s = "\"héllo→\"".as_bytes();
        assert_eq!(spec.scan(s, 0), Ok(s.len()));
        // Scan from an offset.
        assert_eq!(spec.scan(b"x\"y\"z", 1), Ok(4));
    }

    #[test]
    fn fails_at_the_deepest_offset() {
        let (g, body) = json_string_grammar();
        let spec = detect(&g, body).unwrap();
        // No opening quote.
        assert_eq!(spec.scan(b"abc", 0), Err(0));
        // Unterminated.
        assert_eq!(spec.scan(b"\"abc", 0), Err(4));
        // A raw control char (0x01 is outside the plain run).
        let bad_ctrl = b"\"a\x01b\"";
        assert_eq!(spec.scan(bad_ctrl, 0), Err(2));
        // A bad escape form.
        assert_eq!(spec.scan(b"\"a\\qb\"", 0), Err(3));
        // A SHORT hexquad: the failure position is the 3rd hex slot.
        assert_eq!(spec.scan(b"\"\\u12x\"", 0), Err(5));
        // End of input mid-escape.
        assert_eq!(spec.scan(b"\"a\\", 0), Err(3));
    }

    #[test]
    fn refuses_the_non_shapes() {
        let mut g = Grammar::new();
        let quote = g.add_atom(Atom::Str {
            pattern: "\"".to_string(),
        });
        let tick = g.add_atom(Atom::Str {
            pattern: "'".to_string(),
        });
        let plain = g.add_atom(Atom::Re {
            pattern: "[a-z]".to_string(),
        });
        let rep_min1 = g.add_atom(Atom::Repetition {
            atom: plain,
            min: 1,
            max: None,
            tag: RepetitionTag::Repetition,
        });
        let rep0 = g.add_atom(Atom::Repetition {
            atom: plain,
            min: 0,
            max: None,
            tag: RepetitionTag::Repetition,
        });
        let all = g.add_atom(Atom::Re {
            pattern: "[\\u{20}-\\u{7e}]".to_string(),
        });
        let rep_all = g.add_atom(Atom::Repetition {
            atom: all,
            min: 0,
            max: None,
            tag: RepetitionTag::Repetition,
        });
        // A mandatory loop (min > 0) is not the shape.
        let seq = g.add_atom(Atom::Sequence {
            atoms: vec![quote, rep_min1, quote],
        });
        assert!(detect(&g, seq).is_none());
        // Mismatched quotes.
        let seq = g.add_atom(Atom::Sequence {
            atoms: vec![quote, rep0, tick],
        });
        assert!(detect(&g, seq).is_none());
        // The quote admitted by the interior (the boundary law breaks).
        let seq = g.add_atom(Atom::Sequence {
            atoms: vec![quote, rep_all, quote],
        });
        assert!(detect(&g, seq).is_none());
        // A body that is not quote Rep quote.
        let seq = g.add_atom(Atom::Sequence {
            atoms: vec![quote, rep0],
        });
        assert!(detect(&g, seq).is_none());
        // is_string_shaped resolves the Named wrap.
        let named = g.add_atom(Atom::Named {
            name: "s".to_string(),
            atom: seq,
        });
        assert!(!is_string_shaped(&g, named));
    }

    /// The regression pin for the whole-string shape on the fleet's
    /// json grammar (skip-guarded: no sibling checkout, no test): the
    /// real artifact's string rule detects, the classic walker and
    /// the VM emit BYTE-IDENTICAL trees, and each string hash carries
    /// ONE whole-span value (quotes included) — the per-char
    /// iteration spans are gone from the wire.
    #[test]
    fn artifact_json_string_span_differential() {
        use crate::portable::arena::AstArena;
        use crate::portable::bytecode::compile_bytecode;
        use crate::portable::bytecode::vm::{BytecodeVM, VMConfig};
        use crate::portable::visitor::{walk, Visitor};
        use crate::PargArtifact;
        let Ok(text) = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../benchmarks/sota/json-artifact.json"
        )) else {
            eprintln!("skip: no sibling benchmarks/sota checkout");
            return;
        };
        struct M(String);
        impl Visitor for M {
            fn visit_input_ref(&mut self, offset: u32, length: u32, input: &str) {
                self.0.push_str(&format!(
                    "in:{:?}@{};",
                    &input[offset as usize..offset as usize + length as usize],
                    offset
                ));
            }
            fn visit_string_ref(&mut self, pool: u32, arena: &crate::portable::arena::AstArena) {
                self.0
                    .push_str(&format!("s:{:?};", arena.get_string(pool as usize)));
            }
            fn visit_array_start(&mut self, _: u32, length: u32) {
                self.0.push_str(&format!("[{length}("));
            }
            fn visit_array_end(&mut self, _: u32, _: u32) {
                self.0.push_str(")];");
            }
            fn visit_hash_start(&mut self, _: u32, _: u32) {
                self.0.push('{');
            }
            fn visit_hash_key(&mut self, key: &str) {
                self.0.push_str(&format!("{key:?}->"));
            }
            fn visit_hash_end(&mut self, _: u32, _: u32) {
                self.0.push_str("};");
            }
        }
        let artifact = PargArtifact::from_json(&text).unwrap();
        let grammar = artifact.grammar("main").unwrap();
        // The string rule must detect on the real artifact's atoms.
        let string_body = grammar
            .atoms
            .iter()
            .find_map(|a| match a {
                Atom::Named { name, atom } if name == "string" => Some(*atom),
                _ => None,
            })
            .expect("the json artifact has a string rule");
        assert!(
            detect(&grammar, string_body).is_some(),
            "the real json string rule must detect the whole-string shape"
        );

        let input = r#"{"a": "bc", "e": "d\"f\\gA", "n": []}"#;
        let program = compile_bytecode(artifact.grammar("main").unwrap()).unwrap();
        let mut vm_arena = AstArena::for_input(input.len());
        let mut vm = BytecodeVM::new(&program, input, &mut vm_arena, VMConfig::default());
        let vm_result = vm.run().unwrap();
        let vm_tree = walk(&vm_result.value, &vm_arena, input, M(String::new())).0;

        let mut w_arena = AstArena::for_input(input.len());
        let grammar2 = artifact.grammar("main").unwrap();
        let mut parser = crate::portable::PortableParser::new(&grammar2, input, &mut w_arena);
        let classic = parser.parse_with_end_pos().unwrap();
        let classic_tree = walk(&classic.value, &w_arena, input, M(String::new())).0;

        // Engine parity: byte-identical trees on both engines.
        assert_eq!(vm_tree, classic_tree, "vm and classic trees diverged");
        // The wire shape: each string hash carries ONE whole-span
        // value; no :sequence/:repetition envelope remains inside a
        // string hash.
        assert!(
            vm_tree.contains(r#"{"string"->in:"\"bc\""@6;};"#),
            "the whole-string span (quotes folded in) missing: {vm_tree}"
        );
        assert!(
            !vm_tree.contains(r#"{"string"->[4"#),
            "a per-char string envelope survived on the wire: {vm_tree}"
        );
        // Every string rule visit — keys and values — carries exactly
        // ONE whole-span value (3 value strings + 2 member keys).
        assert_eq!(
            vm_tree.matches(r#"{"string"->in:"#).count(),
            5,
            "each string must carry exactly ONE whole-span value: {vm_tree}"
        );
    }
}
