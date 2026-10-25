//! The PARG compiler (rs#298 slice 4a): document → portable atom
//! stream, mirroring the Ruby compiler's `build_atom` for the
//! structural core. The parity contract is the CONFORMANCE CORPUS
//! itself — every artifact is a Ruby-compiled envelope, so the test
//! compiles the `.parg` source and diffs the atom stream and root.
//!
//! Slice-4b scope (skipped by the parity test, listed there): trivia
//! injection (the skip declaration), left-recursion tier rewriting,
//! use-import merging, and table expressions.

use super::parser::{Document, Node};
use crate::portable::grammar::{Atom, Grammar, LookSpec, RepetitionTag};
use std::collections::BTreeMap;

/// The Ruby compiler's builtin class table (ABNF core rules), mirrored
/// verbatim — a builtin reference compiles to its class pattern unless
/// the grammar defines its own rule of the same name.
fn builtin_ranges(name: &str) -> Option<Vec<(u32, u32)>> {
    let ranges = match name {
        "DIGIT" => vec![(0x30, 0x39)],
        "ALPHA" => vec![(0x41, 0x5A), (0x61, 0x7A)],
        "ALNUM" => vec![(0x30, 0x39), (0x41, 0x5A), (0x61, 0x7A)],
        "WORD" => vec![(0x30, 0x39), (0x41, 0x5A), (0x5F, 0x5F), (0x61, 0x7A)],
        "HEXDIG" => vec![(0x30, 0x39), (0x41, 0x46), (0x61, 0x66)],
        "ANY" => vec![(0x00, 0x10FFFF)],
        "BIT" => vec![(0x30, 0x31)],
        "CHAR" => vec![(0x01, 0x7F)],
        "CR" => vec![(0x0D, 0x0D)],
        "CTL" => vec![(0x00, 0x1F), (0x7F, 0x7F)],
        "DQUOTE" => vec![(0x22, 0x22)],
        "HTAB" => vec![(0x09, 0x09)],
        "LF" => vec![(0x0A, 0x0A)],
        "OCTET" => vec![(0x00, 0xFF)],
        "SP" => vec![(0x20, 0x20)],
        "VCHAR" => vec![(0x21, 0x7E)],
        "WSP" => vec![(0x20, 0x20), (0x09, 0x09)],
        _ => return None,
    };
    Some(ranges)
}

/// The Ruby `class_pattern`: singletons escape as one codepoint, short
/// ranges spell each codepoint, long ranges use the `-` operator.
fn class_pattern(ranges: &[(u32, u32)]) -> String {
    let mut body = String::new();
    for &(lo, hi) in ranges {
        if lo == hi {
            body.push_str(&escape_codepoint(lo));
        } else if hi - lo + 1 > 2 {
            body.push_str(&endpoint(lo));
            body.push('-');
            body.push_str(&endpoint(hi));
        } else {
            for code in lo..=hi {
                body.push_str(&escape_codepoint(code));
            }
        }
    }
    format!("[{body}]")
}

fn endpoint(code: u32) -> String {
    if code > 0xFFFF {
        format!("\\u{{{code:x}}}")
    } else if code < 0x7F {
        format!("\\x{code:02x}")
    } else {
        format!("\\u{code:04x}")
    }
}

fn escape_codepoint(code: u32) -> String {
    if code < 0x20 || code == 0x7F {
        format!("\\x{code:02x}")
    } else if code > 0xFFFF {
        format!("\\u{{{code:x}}}")
    } else if code > 0x7E {
        format!("\\u{code:04x}")
    } else if matches!(code as u8 as char, '\\' | ']' | '[' | '^' | '-') {
        format!("\\{}", code as u8 as char)
    } else {
        (code as u8 as char).to_string()
    }
}

/// Ruby's `Regexp.escape` for the `(?i:…)` fold form.
fn regexp_escape(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        if matches!(
            ch,
            '.' | '*'
                | '?'
                | '+'
                | '^'
                | '$'
                | '|'
                | '('
                | ')'
                | '['
                | ']'
                | '{'
                | '}'
                | '\\'
                | '-'
                | '#'
        ) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// A compilation failure: the Ruby compiler's CompileError tone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileError {
    /// The human message.
    pub message: String,
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for CompileError {}

/// The embedded table manifest: table name → rows (objects keyed by
/// column). The artifact carries it so no engine touches the
/// filesystem.
pub type Tables = BTreeMap<String, Vec<BTreeMap<String, String>>>;

struct Compiler<'a> {
    document: &'a Document,
    tables: &'a Tables,
    atoms: Vec<Atom>,
    rule_indices: BTreeMap<String, usize>,
    building: Vec<String>,
}

impl<'a> Compiler<'a> {
    /// The rule's atom index: built once, referenced by Entity.
    fn rule_index(&mut self, name: &str) -> Result<usize, CompileError> {
        if let Some(&index) = self.rule_indices.get(name) {
            return Ok(index);
        }
        let Some(node) = self
            .document
            .rules
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, node)| node)
        else {
            // A builtin reference with no local rule compiles to the
            // class pattern directly (the caller handles this before
            // reaching the rule path).
            return Err(CompileError {
                message: format!("unknown rule {name:?}"),
            });
        };
        if self.building.iter().any(|n| n == name) {
            return Err(CompileError {
                message: format!("left recursion detected: {name:?}"),
            });
        }
        self.building.push(name.to_string());
        let index = self.build(node)?;
        self.building.pop();
        self.rule_indices.insert(name.to_string(), index);
        Ok(index)
    }

    fn build(&mut self, node: &Node) -> Result<usize, CompileError> {
        let atom = match node.kind.as_str() {
            "lit" => {
                let text = node.text.clone().unwrap_or_default();
                if text.is_empty() {
                    return Err(CompileError {
                        message: "empty string literal".to_string(),
                    });
                }
                if node.fold == Some(true) {
                    Atom::Re {
                        pattern: format!("(?i:{})", regexp_escape(&text)),
                    }
                } else {
                    Atom::Str { pattern: text }
                }
            }
            "class" => Atom::Re {
                pattern: class_pattern(node.ranges.as_deref().unwrap_or_default()),
            },
            "category" => Atom::Re {
                pattern: format!("[\\p{{{}}}]", node.code.clone().unwrap_or_default()),
            },
            "seq" => {
                let items = self.index_list(node.items.as_deref().unwrap_or_default())?;
                Atom::Sequence { atoms: items }
            }
            "alt" => {
                let branches = self.index_list(node.branches.as_deref().unwrap_or_default())?;
                Atom::Alternative { atoms: branches }
            }
            "rep" => {
                let child = self.build(node.child.as_deref().ok_or_else(|| CompileError {
                    message: "rep without a child".to_string(),
                })?)?;
                Atom::Repetition {
                    atom: child,
                    min: node.min.unwrap_or(0) as usize,
                    max: node.max.map(|value| value as usize),
                    tag: RepetitionTag::Repetition,
                }
            }
            "opt" => {
                let child = self.build(node.child.as_deref().ok_or_else(|| CompileError {
                    message: "opt without a child".to_string(),
                })?)?;
                Atom::Repetition {
                    atom: child,
                    min: 0,
                    max: Some(1),
                    tag: RepetitionTag::Maybe,
                }
            }
            "pred" => {
                let behind = node.behind == Some(true);
                let child = node.child.as_deref().ok_or_else(|| CompileError {
                    message: "pred without a child".to_string(),
                })?;
                let positive = node.positive.unwrap_or(false);
                if !behind {
                    let inner = self.build(child)?;
                    return Ok(self.push(Atom::Lookahead {
                        atom: inner,
                        positive,
                    }));
                }
                // Behind-guard forms (rs#190): a case-sensitive literal
                // is the fixed byte window; a class, builtin reference,
                // or fold literal is the end-anchored flanking regex.
                let atom = match child.kind.as_str() {
                    "lit" => {
                        let text = child.text.clone().unwrap_or_default();
                        if child.fold == Some(true) {
                            Atom::Lookbehind {
                                look: LookSpec::Regex {
                                    source: format!("(?i:{})", regexp_escape(&text)),
                                },
                                positive,
                            }
                        } else {
                            Atom::Lookbehind {
                                look: LookSpec::Literal {
                                    count: text.len() as u64,
                                    pattern: text,
                                },
                                positive,
                            }
                        }
                    }
                    "class" => Atom::Lookbehind {
                        look: LookSpec::Regex {
                            source: class_pattern(child.ranges.as_deref().unwrap_or_default()),
                        },
                        positive,
                    },
                    "ref" => {
                        let name = child.name.clone().unwrap_or_default();
                        match builtin_ranges(&name) {
                            Some(ranges) => Atom::Lookbehind {
                                look: LookSpec::Regex {
                                    source: class_pattern(&ranges),
                                },
                                positive,
                            },
                            None => {
                                return Err(CompileError {
                                    message: format!(
                                        "lookbehind body must be a literal or character class (got rule {name:?})"
                                    ),
                                })
                            }
                        }
                    }
                    _ => {
                        return Err(CompileError {
                            message: "lookbehind body must be a literal or character class"
                                .to_string(),
                        })
                    }
                };
                return Ok(self.push(atom));
            }
            "cap" => {
                let child = node.child.as_deref().ok_or_else(|| CompileError {
                    message: "cap without a child".to_string(),
                })?;
                // `"" as name` is the zero-width presence marker: an
                // always-succeeding empty Str under the capture.
                if child.kind == "lit" && child.text.as_deref().map(str::is_empty) == Some(true) {
                    let empty = self.push(Atom::Str {
                        pattern: String::new(),
                    });
                    return Ok(self.push(Atom::Named {
                        name: node.name.clone().unwrap_or_default(),
                        atom: empty,
                    }));
                }
                let inner = self.build(child)?;
                Atom::Named {
                    name: node.name.clone().unwrap_or_default(),
                    atom: inner,
                }
            }
            "ref" => {
                let name = node.name.clone().unwrap_or_default();
                let is_rule = self.document.rules.iter().any(|(n, _)| *n == name);
                if !is_rule {
                    if let Some(ranges) = builtin_ranges(&name) {
                        // QoL shorthand: the builtin class pattern,
                        // unless the grammar defines its own rule.
                        return Ok(self.push(Atom::Re {
                            pattern: class_pattern(&ranges),
                        }));
                    }
                    return Err(CompileError {
                        message: format!("unknown rule {name:?}"),
                    });
                }
                if let Some(&index) = self.rule_indices.get(&name) {
                    return Ok(self.push(Atom::Entity { atom: index }));
                }
                // The Ruby wire emits the Entity at its PRE-ORDER slot,
                // then builds the target's subtree after it — the
                // Entity's target index points forward.
                let entity = self.push(Atom::Entity { atom: usize::MAX });
                let target = self.rule_index(&name)?;
                self.atoms[entity] = Atom::Entity { atom: target };
                return Ok(entity);
            }
            "table" => {
                let table = node.table.clone().unwrap_or_default();
                let column = node.column.clone().unwrap_or_default();
                let rows = self.tables.get(&table).ok_or_else(|| CompileError {
                    message: format!("table {table:?} not found"),
                })?;
                let mut values: Vec<String> = rows
                    .iter()
                    .filter_map(|row| row.get(&column))
                    .filter(|value| !value.is_empty())
                    .cloned()
                    .collect();
                values.sort();
                values.dedup();
                values.sort_by_key(|value| std::cmp::Reverse(value.len()));
                if values.is_empty() {
                    return Err(CompileError {
                        message: format!("table {table:?} has no column {column:?}"),
                    });
                }
                let items = values
                    .into_iter()
                    .map(|value| self.push(Atom::Str { pattern: value }))
                    .collect::<Vec<_>>();
                Atom::Alternative { atoms: items }
            }
            other => {
                return Err(CompileError {
                    message: format!("unknown node kind {other}"),
                })
            }
        };
        Ok(self.push(atom))
    }

    fn index_list(&mut self, nodes: &[Node]) -> Result<Vec<usize>, CompileError> {
        nodes.iter().map(|child| self.build(child)).collect()
    }

    fn push(&mut self, atom: Atom) -> usize {
        self.atoms.push(atom);
        self.atoms.len() - 1
    }
}

/// Compile a document into the portable grammar for `entry` — the
/// atoms stream and root the Ruby compiler's entry envelope carries
/// (`{atoms, root}`; bindings ride the envelope separately).
pub fn compile_entry(
    document: &Document,
    entry: &str,
    tables: &Tables,
) -> Result<Grammar, CompileError> {
    let rule = document
        .entries
        .iter()
        .find(|(name, _)| name == entry)
        .map(|(_, rule)| rule.clone())
        .ok_or_else(|| CompileError {
            message: format!("unknown entry {entry:?}"),
        })?;

    let mut compiler = Compiler {
        document,
        tables,
        atoms: Vec::new(),
        rule_indices: BTreeMap::new(),
        building: Vec::new(),
    };
    let root = compiler.rule_index(&rule)?;
    Ok(Grammar {
        atoms: compiler.atoms,
        root,
        no_cache: Vec::new(),
    })
}
