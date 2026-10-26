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
use std::collections::{BTreeMap, BTreeSet};

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

/// The intermediate atom tree: built from the document, rewritten by
/// the trivia-injection pass, then flattened to indices. The split
/// mirrors the Ruby compiler (build_atom → inject_skip_trivia → the
/// serializer walk), whose index assignment is the parity wire.
#[derive(Clone)]
enum INode {
    Str(String),
    Re(String),
    Seq(Vec<INode>),
    Alt(Vec<INode>),
    Rep {
        child: Box<INode>,
        min: u32,
        max: Option<u32>,
        /// `opt` spelling: the wire tag is `Maybe`, plain repetitions
        /// carry `Repetition`.
        maybe: bool,
    },
    Lookahead {
        child: Box<INode>,
        positive: bool,
    },
    Lookbehind {
        look: LookSpec,
        positive: bool,
    },
    Named {
        name: String,
        child: Box<INode>,
    },
    /// A rule reference. The flatten emits the pre-order Entity slot
    /// and resolves the target's subtree after it (forward target).
    Entity {
        rule: String,
    },
    /// The injected skip wrapper. Identity-marked: sequences never
    /// inject before it and the injection pass never descends into
    /// it. Flattens to `Trivia`, or `TriviaCapture` when the skip
    /// declaration carries a capture suffix.
    SkipWrap(Box<INode>),
}

impl INode {
    /// The VM's atom-level nullability (vm.rb `nullable?`): the
    /// skip-declaration guard uses it verbatim.
    fn nullable(&self) -> bool {
        match self {
            INode::Str(text) => text.is_empty(),
            INode::Re(_) | INode::Lookbehind { .. } => false,
            INode::Seq(items) => items.iter().all(INode::nullable),
            INode::Alt(branches) => branches.iter().any(INode::nullable),
            INode::Rep { min, .. } => *min == 0,
            INode::Lookahead { .. } => true,
            INode::Named { child, .. } => child.nullable(),
            INode::Entity { .. } | INode::SkipWrap(_) => false,
        }
    }
}

/// The left-recursion rewrite's reserved tier prefix (the Ruby
/// compiler's LEFT_RECURSION_PREFIX).
const LEFT_RECURSION_PREFIX: &str = "__left_";

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

/// The prepared compiler: expanded rules, lint findings, and the
/// memoized rule trees behind the envelope assembly.
pub struct Compiler<'a> {
    document: &'a Document,
    tables: &'a Tables,
    /// Owned rule copy: the left-recursion rewrite appends tier rules
    /// (the Ruby compiler mutates its document in place).
    rules: Vec<(String, Node)>,
    atomic_rules: Vec<String>,
    /// Rule name → injected tree (the Ruby atom_for cache).
    trees: BTreeMap<String, INode>,
    building: Vec<String>,
    /// Rule name → top atom index (the Ruby serializer's memo: the
    /// first reference emits the subtree, later references point at
    /// it).
    rule_indices: BTreeMap<String, usize>,
    skip_wrap: Option<INode>,
    /// The memoized wrapper's emitted index: the Ruby wire serializes
    /// the (single, shared) skip wrapper object once — every
    /// injection site points at the same Trivia index.
    skip_wrap_index: Option<usize>,
    order_warnings: Vec<String>,
    modern: bool,
    skip_exempt: Vec<String>,
    skip_markers: Vec<(String, String)>,
    atoms: Vec<Atom>,
}

impl<'a> Compiler<'a> {
    fn new_mode(
        document: &'a Document,
        tables: &'a Tables,
        atomic_rules: &[String],
        mode: Option<&str>,
    ) -> Result<Self, CompileError> {
        let mut compiler = Compiler {
            document,
            tables,
            rules: document.rules.clone(),
            atomic_rules: atomic_rules.to_vec(),
            trees: BTreeMap::new(),
            building: Vec::new(),
            rule_indices: BTreeMap::new(),
            skip_wrap: None,
            skip_wrap_index: None,
            order_warnings: Vec::new(),
            modern: mode == Some("modern"),
            skip_exempt: Vec::new(),
            skip_markers: Vec::new(),
            atoms: Vec::new(),
        };
        compiler.expand_left_recursion()?;
        compiler.skip_exempt = compiler.collect_skip_exempt();
        compiler.skip_markers = compiler.collect_skip_markers();
        // The Ruby compile warms every rule (build errors surface even
        // for entry-unreachable rules), then rejects on lint errors.
        let names: Vec<String> = compiler.rules.iter().map(|(n, _)| n.clone()).collect();
        for name in &names {
            compiler.rule_tree(name)?;
        }
        let errors = compiler.alternatives_errors();
        if !errors.is_empty() {
            return Err(CompileError {
                message: errors.join("\n"),
            });
        }
        compiler.order_warnings = compiler.alternatives_order_warnings();
        Ok(compiler)
    }

    /// The envelope-carried order-dependence findings, in lint order.
    pub fn order_warnings(&self) -> &[String] {
        &self.order_warnings
    }

    /// One entry's portable grammar: the tree, flattened fresh (the
    /// Ruby portable_json).
    pub fn compile_entry_grammar(&mut self, rule: &str) -> Result<Grammar, CompileError> {
        let tree = self.rule_tree(rule)?;
        self.flatten(&tree)
    }

    /// A resync pattern as its own mini-grammar: raw match, no trivia
    /// injection (the pattern delimits junk, it is not syntax).
    pub fn compile_resync_grammar(&mut self, pattern: &Node) -> Result<Grammar, CompileError> {
        let tree = self.build(pattern)?;
        self.flatten(&tree)
    }

    /// The rule's atom tree: built once, wrapped with trivia injection
    /// and the entry boundary (the Ruby atom_for).
    fn rule_tree(&mut self, name: &str) -> Result<INode, CompileError> {
        if let Some(tree) = self.trees.get(name) {
            return Ok(tree.clone());
        }
        let Some((_, node)) = self.rules.iter().find(|(n, _)| n == name) else {
            return Err(CompileError {
                message: format!("unknown rule {name:?}"),
            });
        };
        let node = node.clone();
        if self.building.iter().any(|n| n == name) {
            return Err(CompileError {
                message: format!("left recursion detected: {name:?}"),
            });
        }
        self.building.push(name.to_string());
        let tree = self.build(&node)?;
        self.building.pop();

        let atomic = self.atomic_rules.iter().any(|n| n == name);
        let skip_active = self.document.skip_declaration.is_some();
        let exempt = self.skip_exempt.iter().any(|n| n == name);
        let tree = if skip_active && !exempt && !atomic {
            let tree = self.inject(tree)?;
            if self.document.entries.iter().any(|(_, rule)| rule == name) {
                let skip = self.skip_maybe()?;
                INode::Seq(vec![skip.clone(), tree, skip])
            } else {
                tree
            }
        } else {
            tree
        };
        // Modern mode (ruby#304) wraps LAST — the guide's spec table:
        // trivia atoms are in place first, so the pass sees the final
        // tree. Mirrors the Ruby compiler's atom_for order. The
        // unwrapped tree inserts BEFORE the wrap: the wrap's capture
        // probes resolve rule references, and a mutual-reference
        // grammar must hit the cache, not rebuild.
        self.trees.insert(name.to_string(), tree.clone());
        let tree = if self.modern {
            self.wrap_uncaptured(tree)?
        } else {
            tree
        };
        self.trees.insert(name.to_string(), tree.clone());
        Ok(tree)
    }

    /// Modern mode (ruby#304, the #315 spec): every rule reference
    /// whose flattening surfaces no capture is wrapped in
    /// Named(rule_name). Composites recurse; authored captures and
    /// trivia subtrees are untouched.
    fn wrap_uncaptured(&mut self, node: INode) -> Result<INode, CompileError> {
        Ok(match node {
            INode::Named { .. } => node,
            INode::Entity { rule } => {
                let produces = self.produces_capture(&INode::Entity { rule: rule.clone() })?;
                if produces {
                    INode::Entity { rule }
                } else {
                    INode::Named {
                        name: rule.clone(),
                        child: Box::new(INode::Entity { rule }),
                    }
                }
            }
            INode::Seq(items) => {
                let out = items
                    .into_iter()
                    .map(|child| self.wrap_uncaptured(child))
                    .collect::<Result<Vec<_>, _>>()?;
                INode::Seq(out)
            }
            INode::Alt(branches) => {
                let out = branches
                    .into_iter()
                    .map(|branch| self.wrap_uncaptured(branch))
                    .collect::<Result<Vec<_>, _>>()?;
                INode::Alt(out)
            }
            INode::Rep {
                child,
                min,
                max,
                maybe,
            } => INode::Rep {
                child: Box::new(self.wrap_uncaptured(*child)?),
                min,
                max,
                maybe,
            },
            other => other,
        })
    }

    /// Whether the tree surfaces any Named capture: a wrapped entity
    /// (its rule's tree root is Named) counts; entity references
    /// resolve through their rule trees with a visited set.
    fn produces_capture(&mut self, node: &INode) -> Result<bool, CompileError> {
        let mut visited = Vec::new();
        self.produces_capture_walk(node, &mut visited)
    }

    fn produces_capture_walk(
        &mut self,
        node: &INode,
        visited: &mut Vec<String>,
    ) -> Result<bool, CompileError> {
        Ok(match node {
            INode::Named { .. } => true,
            INode::Entity { rule } => {
                if visited.contains(rule) {
                    return Ok(false);
                }
                visited.push(rule.clone());
                let tree = self.rule_tree(rule)?;
                self.produces_capture_walk(&tree, visited)?
            }
            INode::Seq(items) => {
                let mut any = false;
                for child in items {
                    if self.produces_capture_walk(child, visited)? {
                        any = true;
                        break;
                    }
                }
                any
            }
            INode::Alt(branches) => {
                let mut any = false;
                for child in branches {
                    if self.produces_capture_walk(child, visited)? {
                        any = true;
                        break;
                    }
                }
                any
            }
            INode::Rep { child, .. } => self.produces_capture_walk(child, visited)?,
            _ => false,
        })
    }

    /// The skip declaration's references, closed transitively: those
    /// rules build without injection so the skip atom never contains
    /// itself.
    fn collect_skip_exempt(&self) -> Vec<String> {
        let Some(declaration) = self.document.skip_declaration.as_deref() else {
            return Vec::new();
        };
        let mut names = Vec::new();
        collect_node_refs(declaration, &mut names);
        let mut closed: Vec<String> = Vec::new();
        while let Some(name) = names.iter().find(|n| !closed.contains(n)).cloned() {
            closed.push(name.clone());
            if let Some((_, node)) = self.rules.iter().find(|(n, _)| *n == name) {
                collect_node_refs(node, &mut names);
            }
        }
        closed
    }

    /// parsanol-ruby#152: the skip declaration's transitively
    /// referenced rules whose leading literal is a non-whitespace
    /// marker — the comment shapes. Deduped by rule, ordered by first
    /// reference.
    fn collect_skip_markers(&self) -> Vec<(String, String)> {
        let Some(declaration) = self.document.skip_declaration.as_deref() else {
            return Vec::new();
        };
        let mut markers: Vec<(String, String)> = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        let mut stack = vec![declaration];
        while let Some(node) = stack.pop() {
            match node.kind.as_str() {
                "ref" => {
                    let name = node.name.clone().unwrap_or_default();
                    if seen.contains(&name) {
                        continue;
                    }
                    seen.push(name.clone());
                    if let Some((_, rule)) = self.rules.iter().find(|(n, _)| *n == name) {
                        if let Some(literal) = leading_literal(rule) {
                            if !literal.trim().is_empty()
                                && !markers.iter().any(|(marker, _)| *marker == literal)
                            {
                                markers.push((literal, name.clone()));
                            }
                        }
                        stack.push(rule);
                    }
                }
                "seq" | "alt" => stack.extend(
                    node.items
                        .iter()
                        .chain(node.branches.iter())
                        .flatten()
                        .rev(),
                ),
                "rep" | "opt" | "cap" | "pred" => {
                    if let Some(child) = node.child.as_deref() {
                        stack.push(child);
                    }
                }
                _ => {}
            }
        }
        markers
    }

    /// The skip wrapper: an optional match of the skip rule's tree,
    /// memoized (the Ruby skip_maybe_atom). Nullability is the VM's
    /// check — a skip that can match empty loops forever.
    fn skip_maybe(&mut self) -> Result<INode, CompileError> {
        if let Some(wrap) = self.skip_wrap.as_ref() {
            return Ok(wrap.clone());
        }
        let declaration =
            self.document
                .skip_declaration
                .as_deref()
                .ok_or_else(|| CompileError {
                    message: "no skip declaration".to_string(),
                })?;
        let mut inner = self.build(declaration)?;
        while let INode::Entity { rule } = &inner {
            let rule = rule.clone();
            inner = self.rule_tree(&rule)?;
        }
        if inner.nullable() {
            return Err(CompileError {
                message: "skip rule must be non-nullable: a skip that can match \
                          empty loops forever at injection points"
                    .to_string(),
            });
        }
        let wrap = INode::SkipWrap(Box::new(INode::Rep {
            child: Box::new(inner),
            min: 0,
            max: Some(1),
            maybe: false,
        }));
        self.skip_wrap = Some(wrap.clone());
        Ok(wrap)
    }

    /// parsanol-ruby#134: an optional skip match before every terminal
    /// of every sequence, except before lookaheads (their contract is
    /// the raw next character, parsanol-ruby#192) and before other
    /// skip wrappers. Repetition bodies rewrite only when the body is
    /// a sequence containing rule references — char-scan spans stay
    /// verbatim; every other shape passes through untouched. A bare
    /// terminal as a whole rule body takes a leading skip.
    fn inject(&mut self, node: INode) -> Result<INode, CompileError> {
        Ok(match node {
            INode::SkipWrap(_)
            | INode::Lookahead { .. }
            | INode::Lookbehind { .. }
            | INode::Entity { .. } => node,
            INode::Seq(items) => {
                let kids = items
                    .into_iter()
                    .map(|child| self.inject(child))
                    .collect::<Result<Vec<_>, _>>()?;
                let mut out = Vec::new();
                for child in kids {
                    if !matches!(child, INode::SkipWrap(_) | INode::Lookahead { .. }) {
                        out.push(self.skip_maybe()?);
                    }
                    out.push(child);
                }
                if out.len() == 1 {
                    out.pop().unwrap()
                } else {
                    INode::Seq(out)
                }
            }
            INode::Alt(branches) => {
                let out = branches
                    .into_iter()
                    .map(|branch| self.inject(branch))
                    .collect::<Result<Vec<_>, _>>()?;
                INode::Alt(out)
            }
            INode::Rep {
                child,
                min,
                max,
                maybe,
            } => {
                if let INode::Seq(items) = child.as_ref() {
                    if items.iter().any(|c| matches!(c, INode::Entity { .. })) {
                        let mut kids = Vec::new();
                        for item in items {
                            if matches!(item, INode::Entity { .. }) {
                                kids.push(self.skip_maybe()?);
                            }
                            kids.push(item.clone());
                        }
                        let rebuilt = if kids.len() == 1 {
                            kids.pop().unwrap()
                        } else {
                            INode::Seq(kids)
                        };
                        return Ok(INode::Rep {
                            child: Box::new(rebuilt),
                            min,
                            max,
                            maybe,
                        });
                    }
                }
                INode::Rep {
                    child,
                    min,
                    max,
                    maybe,
                }
            }
            INode::Named { name, child } => INode::Named {
                name,
                child: Box::new(self.inject(*child)?),
            },
            terminal @ (INode::Str(_) | INode::Re(_)) => {
                let skip = self.skip_maybe()?;
                INode::Seq(vec![skip, terminal])
            }
        })
    }

    /// The Ruby build_atom: document node → intermediate tree.
    fn build(&mut self, node: &Node) -> Result<INode, CompileError> {
        match node.kind.as_str() {
            "lit" => {
                let text = node.text.clone().unwrap_or_default();
                if text.is_empty() {
                    return Err(CompileError {
                        message: "empty string literal".to_string(),
                    });
                }
                if node.fold == Some(true) {
                    Ok(INode::Re(format!("(?i:{})", regexp_escape(&text))))
                } else {
                    Ok(INode::Str(text))
                }
            }
            "class" => Ok(INode::Re(class_pattern(
                node.ranges.as_deref().unwrap_or_default(),
            ))),
            "category" => Ok(INode::Re(format!(
                "[\\p{{{}}}]",
                node.code.clone().unwrap_or_default()
            ))),
            "seq" => {
                let items = node
                    .items
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .map(|child| self.build(child))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(if items.len() == 1 {
                    items.into_iter().next().unwrap()
                } else {
                    INode::Seq(items)
                })
            }
            "alt" => {
                let branches = node
                    .branches
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .map(|child| self.build(child))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(if branches.len() == 1 {
                    branches.into_iter().next().unwrap()
                } else {
                    INode::Alt(branches)
                })
            }
            "rep" => {
                let child = self.build(node.child.as_deref().ok_or_else(|| CompileError {
                    message: "rep without a child".to_string(),
                })?)?;
                Ok(INode::Rep {
                    child: Box::new(child),
                    min: node.min.unwrap_or(0),
                    max: node.max,
                    maybe: false,
                })
            }
            "opt" => {
                let child = self.build(node.child.as_deref().ok_or_else(|| CompileError {
                    message: "opt without a child".to_string(),
                })?)?;
                Ok(INode::Rep {
                    child: Box::new(child),
                    min: 0,
                    max: Some(1),
                    maybe: true,
                })
            }
            "pred" => {
                let behind = node.behind == Some(true);
                let child = node.child.as_deref().ok_or_else(|| CompileError {
                    message: "pred without a child".to_string(),
                })?;
                let positive = node.positive.unwrap_or(false);
                if !behind {
                    let inner = self.build(child)?;
                    Ok(INode::Lookahead {
                        child: Box::new(inner),
                        positive,
                    })
                } else {
                    let look = match child.kind.as_str() {
                        "lit" => {
                            let text = child.text.clone().unwrap_or_default();
                            if child.fold == Some(true) {
                                LookSpec::Regex {
                                    source: format!("(?i:{})", regexp_escape(&text)),
                                }
                            } else {
                                LookSpec::Literal {
                                    count: text.len() as u64,
                                    pattern: text,
                                }
                            }
                        }
                        "class" => LookSpec::Regex {
                            source: class_pattern(child.ranges.as_deref().unwrap_or_default()),
                        },
                        "ref" => {
                            let name = child.name.clone().unwrap_or_default();
                            match builtin_ranges(&name) {
                                Some(ranges) => LookSpec::Regex {
                                    source: class_pattern(&ranges),
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
                    Ok(INode::Lookbehind { look, positive })
                }
            }
            "cap" => {
                let child = node.child.as_deref().ok_or_else(|| CompileError {
                    message: "cap without a child".to_string(),
                })?;
                // `"" as name` is the zero-width presence marker.
                if child.kind == "lit" && child.text.as_deref().map(str::is_empty) == Some(true) {
                    return Ok(INode::Named {
                        name: node.name.clone().unwrap_or_default(),
                        child: Box::new(INode::Str(String::new())),
                    });
                }
                let inner = self.build(child)?;
                Ok(INode::Named {
                    name: node.name.clone().unwrap_or_default(),
                    child: Box::new(inner),
                })
            }
            "ref" => {
                let name = node.name.clone().unwrap_or_default();
                let is_rule = self.rules.iter().any(|(n, _)| *n == name);
                if !is_rule {
                    if let Some(ranges) = builtin_ranges(&name) {
                        return Ok(INode::Re(class_pattern(&ranges)));
                    }
                    return Err(CompileError {
                        message: format!("unknown rule {name:?}"),
                    });
                }
                Ok(INode::Entity { rule: name })
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
                Ok(INode::Alt(values.into_iter().map(INode::Str).collect()))
            }
            other => Err(CompileError {
                message: format!("unknown node kind {other}"),
            }),
        }
    }

    // ---- left-recursion tier rewriting (the ANTLR 4 model) ----

    fn expand_left_recursion(&mut self) -> Result<(), CompileError> {
        if self
            .rules
            .iter()
            .any(|(name, _)| name.starts_with(LEFT_RECURSION_PREFIX))
        {
            return Err(CompileError {
                message: format!(
                    "rule names beginning with {LEFT_RECURSION_PREFIX:?} are reserved \
                     for the compiler's left-recursion expansion"
                ),
            });
        }
        let names: Vec<String> = self.rules.iter().map(|(name, _)| name.clone()).collect();
        for name in names {
            let Some((_, body)) = self.rules.iter().find(|(n, _)| *n == name) else {
                continue;
            };
            let body = body.clone();
            if !self.left_recursive_body(&name, &body) {
                continue;
            }
            self.expand_left_recursive_rule(&name, &body)?;
        }
        Ok(())
    }

    fn left_recursive_body(&self, name: &str, body: &Node) -> bool {
        let branches: Vec<&Node> = match body.kind.as_str() {
            "alt" => body
                .branches
                .as_deref()
                .unwrap_or_default()
                .iter()
                .collect(),
            _ => vec![body],
        };
        branches
            .iter()
            .any(|branch| self.leftmost_refs(branch, true).contains(&name.to_string()))
    }

    fn expand_left_recursive_rule(&mut self, name: &str, body: &Node) -> Result<(), CompileError> {
        let branches: Vec<Node> = match body.kind.as_str() {
            "alt" => body.branches.as_deref().unwrap_or_default().to_vec(),
            _ => vec![body.clone()],
        };
        let mut left = Vec::new();
        let mut primary = Vec::new();
        for branch in branches {
            if self
                .leftmost_refs(&branch, true)
                .contains(&name.to_string())
            {
                left.push(branch);
            } else {
                primary.push(branch);
            }
        }
        if primary.is_empty() {
            return Err(CompileError {
                message: format!(
                    "rule {name:?} is left-recursive and has no non-recursive alternative to anchor the rewrite"
                ),
            });
        }

        let mut prev = format!("{LEFT_RECURSION_PREFIX}{name}_primary");
        let primary_body = if primary.len() == 1 {
            primary.into_iter().next().unwrap()
        } else {
            Node {
                kind: "alt".to_string(),
                branches: Some(primary),
                ..Node::default()
            }
        };
        self.set_rule(&prev, primary_body);
        for (index, branch) in left.into_iter().enumerate() {
            let tier = format!("{LEFT_RECURSION_PREFIX}{name}_{}", index + 1);
            let suffix = self.left_recursive_suffix(name, &branch)?;
            let suffix = substitute_refs(&suffix, name, &prev);
            self.set_rule(
                &tier,
                Node {
                    kind: "seq".to_string(),
                    items: Some(vec![
                        Node {
                            kind: "ref".to_string(),
                            name: Some(prev.clone()),
                            ..Node::default()
                        },
                        Node {
                            kind: "rep".to_string(),
                            child: Some(Box::new(suffix)),
                            min: Some(0),
                            max: None,
                            ..Node::default()
                        },
                    ]),
                    ..Node::default()
                },
            );
            prev = tier;
        }
        self.set_rule(
            name,
            Node {
                kind: "ref".to_string(),
                name: Some(prev),
                ..Node::default()
            },
        );
        Ok(())
    }

    fn set_rule(&mut self, name: &str, node: Node) {
        if let Some(slot) = self.rules.iter_mut().find(|(n, _)| n == name) {
            slot.1 = node;
        } else {
            self.rules.push((name.to_string(), node));
        }
    }

    /// A left-recursive alternative must BEGIN with a bare reference to
    /// the rule itself; everything after it is the per-iteration
    /// suffix, which must consume input.
    fn left_recursive_suffix(&self, name: &str, branch: &Node) -> Result<Node, CompileError> {
        if branch.kind == "ref" {
            return Err(CompileError {
                message: format!(
                    "left-recursive alternative in rule {name:?} consists only of the recursive reference and would loop forever"
                ),
            });
        }
        let head_ok = branch.kind == "seq"
            && branch
                .items
                .as_deref()
                .and_then(|items| items.first())
                .is_some_and(|head| head.kind == "ref" && head.name.as_deref() == Some(name));
        if !head_ok {
            return Err(CompileError {
                message: format!(
                    "left-recursive alternative in rule {name:?} must begin with a bare reference to {name:?} (write `{name} <rest>`; captures and predicates around the leading reference are not supported)"
                ),
            });
        }
        let rest: Vec<Node> = branch
            .items
            .as_deref()
            .unwrap_or_default()
            .iter()
            .skip(1)
            .cloned()
            .collect();
        let rest_node = if rest.len() == 1 {
            rest.into_iter().next().unwrap()
        } else {
            Node {
                kind: "seq".to_string(),
                items: Some(rest),
                ..Node::default()
            }
        };
        if self.node_nullable(&rest_node) {
            return Err(CompileError {
                message: format!(
                    "left-recursive alternative in rule {name:?} can match empty after the leading reference and would loop forever at the fold"
                ),
            });
        }
        Ok(rest_node)
    }

    /// The compiler's node-level nullable? (first_set contains EPS),
    /// the leftmost walk's stopping condition.
    fn node_nullable(&self, node: &Node) -> bool {
        self.node_first_epsp(node, &mut Vec::new())
    }

    fn node_first_epsp(&self, node: &Node, stack: &mut Vec<String>) -> bool {
        match node.kind.as_str() {
            "lit" => node.text.as_deref().unwrap_or_default().is_empty(),
            "class" | "category" | "table" => false,
            "seq" => node
                .items
                .as_deref()
                .unwrap_or_default()
                .iter()
                .all(|child| self.node_first_epsp(child, stack)),
            "alt" => node
                .branches
                .as_deref()
                .unwrap_or_default()
                .iter()
                .any(|child| self.node_first_epsp(child, stack)),
            "rep" => node.min.unwrap_or(0) == 0,
            "opt" => true,
            // A positive pred can start with its body's first set; a
            // negative one constrains nothing (ANY). Behind-guards are
            // ANY either way.
            "pred" => {
                if node.behind == Some(true) {
                    false
                } else {
                    node.positive.unwrap_or(false)
                        && node
                            .child
                            .as_deref()
                            .is_some_and(|child| self.node_first_epsp(child, stack))
                }
            }
            "cap" => node
                .child
                .as_deref()
                .is_some_and(|child| self.node_first_epsp(child, stack)),
            "ref" => {
                let name = node.name.clone().unwrap_or_default();
                if stack.contains(&name) {
                    return true; // ANY + EPS at the cycle guard
                }
                stack.push(name);
                let epsp = self
                    .rules
                    .iter()
                    .find(|(n, _)| *n == stack[stack.len() - 1])
                    .is_some_and(|(_, rule)| self.node_first_epsp(rule, stack));
                stack.pop();
                epsp
            }
            _ => false,
        }
    }

    /// Leftmost rule references, pre-order through nullable heads.
    /// `foldable` stops at predicates (they cannot anchor a fold).
    fn leftmost_refs(&self, node: &Node, foldable: bool) -> Vec<String> {
        let mut refs = Vec::new();
        self.leftmost_walk(node, foldable, &mut refs);
        refs.dedup();
        refs
    }

    fn leftmost_walk(&self, node: &Node, foldable: bool, refs: &mut Vec<String>) {
        match node.kind.as_str() {
            "ref" => {
                let name = node.name.clone().unwrap_or_default();
                if !refs.contains(&name) {
                    refs.push(name);
                }
            }
            "seq" => {
                for item in node.items.as_deref().unwrap_or_default() {
                    self.leftmost_walk(item, foldable, refs);
                    if !self.node_nullable(item) {
                        break;
                    }
                }
            }
            "alt" => {
                for branch in node.branches.as_deref().unwrap_or_default() {
                    self.leftmost_walk(branch, foldable, refs);
                }
            }
            "rep" | "opt" => {
                if let Some(child) = node.child.as_deref() {
                    self.leftmost_walk(child, foldable, refs);
                }
            }
            "pred" => {
                if !foldable {
                    if let Some(child) = node.child.as_deref() {
                        self.leftmost_walk(child, foldable, refs);
                    }
                }
            }
            "cap" => {
                if let Some(child) = node.child.as_deref() {
                    self.leftmost_walk(child, foldable, refs);
                }
            }
            _ => {}
        }
    }

    // ---- flatten ----

    /// The pre-order walk to the wire: the Entity emits at its own
    /// slot with a forward target index; the skip wrapper duplicates
    /// at every injection site (the Ruby wire duplicates it too).
    fn flatten(&mut self, tree: &INode) -> Result<Grammar, CompileError> {
        self.atoms.clear();
        self.rule_indices.clear();
        self.skip_wrap_index = None;
        let root = self.walk(tree)?;
        Ok(Grammar {
            atoms: std::mem::take(&mut self.atoms),
            root,
            no_cache: Vec::new(),
        })
    }

    fn push(&mut self, atom: Atom) -> usize {
        self.atoms.push(atom);
        self.atoms.len() - 1
    }

    fn walk(&mut self, node: &INode) -> Result<usize, CompileError> {
        Ok(match node {
            INode::Str(text) => self.push(Atom::Str {
                pattern: text.clone(),
            }),
            INode::Re(pattern) => self.push(Atom::Re {
                pattern: pattern.clone(),
            }),
            INode::Seq(items) => {
                let indices = items
                    .iter()
                    .map(|child| self.walk(child))
                    .collect::<Result<Vec<_>, _>>()?;
                self.push(Atom::Sequence { atoms: indices })
            }
            INode::Alt(branches) => {
                let indices = branches
                    .iter()
                    .map(|child| self.walk(child))
                    .collect::<Result<Vec<_>, _>>()?;
                self.push(Atom::Alternative { atoms: indices })
            }
            INode::Rep {
                child,
                min,
                max,
                maybe,
            } => {
                let child = self.walk(child)?;
                self.push(Atom::Repetition {
                    atom: child,
                    min: *min as usize,
                    max: max.map(|value| value as usize),
                    tag: if *maybe {
                        RepetitionTag::Maybe
                    } else {
                        RepetitionTag::Repetition
                    },
                })
            }
            INode::Lookahead { child, positive } => {
                let child = self.walk(child)?;
                self.push(Atom::Lookahead {
                    atom: child,
                    positive: *positive,
                })
            }
            INode::Lookbehind { look, positive } => self.push(Atom::Lookbehind {
                look: look.clone(),
                positive: *positive,
            }),
            INode::Named { name, child } => {
                let child = self.walk(child)?;
                self.push(Atom::Named {
                    name: name.clone(),
                    atom: child,
                })
            }
            INode::Entity { rule } => {
                let slot = self.push(Atom::Entity { atom: usize::MAX });
                let target = if let Some(&index) = self.rule_indices.get(rule) {
                    index
                } else {
                    let tree = self.rule_tree(rule)?;
                    let index = self.walk(&tree)?;
                    self.rule_indices.insert(rule.clone(), index);
                    index
                };
                self.atoms[slot] = Atom::Entity { atom: target };
                slot
            }
            INode::SkipWrap(inner) => {
                if let Some(index) = self.skip_wrap_index {
                    return Ok(index);
                }
                let inner = self.walk(inner)?;
                let index = if self.document.skip_capture.is_some() {
                    self.push(Atom::TriviaCapture {
                        atom: inner,
                        rules: self.skip_markers.clone(),
                        whitespace: None,
                    })
                } else {
                    self.push(Atom::Trivia { atom: inner })
                };
                self.skip_wrap_index = Some(index);
                index
            }
        })
    }
}

/// A node first set: UTF-8 first bytes / class codepoints, plus the
/// ANY and EPS markers (the Ruby compute_first convention).
#[derive(Default, Clone)]
struct FirstSet {
    bytes: BTreeSet<u32>,
    any: bool,
    eps: bool,
}

impl FirstSet {
    fn intersects(&self, other: &FirstSet) -> bool {
        self.bytes.iter().any(|byte| other.bytes.contains(byte)) || (self.any && other.any)
    }
}

impl<'a> Compiler<'a> {
    /// The compiler's node-level first set (compute_first), the
    /// order-lint's analysis substrate.
    fn first_set(&self, node: &Node, stack: &mut Vec<String>) -> FirstSet {
        match node.kind.as_str() {
            "lit" => {
                let text = node.text.as_deref().unwrap_or_default();
                let mut set = FirstSet::default();
                if text.is_empty() {
                    set.eps = true;
                    return set;
                }
                let Some(first) = text.chars().next() else {
                    set.eps = true;
                    return set;
                };
                let push_char = |out: &mut BTreeSet<u32>, ch: char| {
                    let mut buffer = [0u8; 4];
                    for byte in ch.encode_utf8(&mut buffer).bytes() {
                        out.insert(byte as u32);
                    }
                };
                push_char(&mut set.bytes, first);
                if node.fold == Some(true) {
                    for upper in first.to_uppercase() {
                        push_char(&mut set.bytes, upper);
                    }
                    for lower in first.to_lowercase() {
                        push_char(&mut set.bytes, lower);
                    }
                }
                set
            }
            "class" => {
                let ranges = node.ranges.as_deref().unwrap_or_default();
                let total: u64 = ranges
                    .iter()
                    .map(|(lo, hi)| (*hi as u64).saturating_sub(*lo as u64) + 1)
                    .sum();
                let mut set = FirstSet::default();
                if total > 256 {
                    set.any = true;
                    return set;
                }
                for (lo, hi) in ranges {
                    for point in *lo..=*hi {
                        set.bytes.insert(point);
                    }
                }
                set
            }
            // Categories and unknown kinds are ANY.
            "category" => FirstSet {
                bytes: BTreeSet::new(),
                any: true,
                eps: false,
            },
            "seq" => {
                let mut set = FirstSet::default();
                let items = node.items.as_deref().unwrap_or_default();
                let mut all_eps = true;
                for item in items {
                    let item_first = self.first_set(item, stack);
                    set.bytes.extend(item_first.bytes.iter().copied());
                    if item_first.any {
                        set.any = true;
                    }
                    if item_first.any || !item_first.eps {
                        all_eps = false;
                        break;
                    }
                }
                set.eps = all_eps && items.iter().all(|item| self.first_set(item, stack).eps);
                set
            }
            "alt" => {
                let mut set = FirstSet::default();
                for branch in node.branches.as_deref().unwrap_or_default() {
                    let branch_first = self.first_set(branch, stack);
                    set.bytes.extend(branch_first.bytes.iter().copied());
                    set.any |= branch_first.any;
                    set.eps |= branch_first.eps;
                }
                set
            }
            "rep" => {
                let mut set = node
                    .child
                    .as_deref()
                    .map(|child| self.first_set(child, stack))
                    .unwrap_or_default();
                if node.min.unwrap_or(0) == 0 {
                    set.eps = true;
                }
                set
            }
            "opt" => {
                let mut set = node
                    .child
                    .as_deref()
                    .map(|child| self.first_set(child, stack))
                    .unwrap_or_default();
                set.eps = true;
                set
            }
            "pred" => {
                // Behind-guards constrain nothing ahead; a negative
                // lookahead cannot claim a first byte either.
                if node.behind == Some(true) || node.positive != Some(true) {
                    return FirstSet {
                        bytes: BTreeSet::new(),
                        any: true,
                        eps: false,
                    };
                }
                node.child
                    .as_deref()
                    .map(|child| self.first_set(child, stack))
                    .unwrap_or_default()
            }
            "cap" => node
                .child
                .as_deref()
                .map(|child| self.first_set(child, stack))
                .unwrap_or_default(),
            "ref" => {
                let name = node.name.clone().unwrap_or_default();
                if stack.contains(&name) {
                    return FirstSet {
                        bytes: BTreeSet::new(),
                        any: true,
                        eps: true,
                    };
                }
                stack.push(name.clone());
                let set = if let Some((_, rule)) = self.rules.iter().find(|(n, _)| *n == name) {
                    self.first_set(rule, stack)
                } else if let Some(ranges) = builtin_ranges(&name) {
                    let class = Node {
                        kind: "class".to_string(),
                        ranges: Some(ranges),
                        ..Node::default()
                    };
                    self.first_set(&class, stack)
                } else {
                    FirstSet {
                        bytes: BTreeSet::new(),
                        any: true,
                        eps: false,
                    }
                };
                stack.pop();
                set
            }
            "table" => {
                let table = node.table.clone().unwrap_or_default();
                let column = node.column.clone().unwrap_or_default();
                let mut set = FirstSet::default();
                if let Some(rows) = self.tables.get(&table) {
                    for value in rows.iter().filter_map(|row| row.get(&column)) {
                        if let Some(first) = value.chars().next() {
                            let mut buffer = [0u8; 4];
                            for byte in first.encode_utf8(&mut buffer).bytes() {
                                set.bytes.insert(byte as u32);
                            }
                        }
                    }
                }
                set
            }
            _ => FirstSet {
                bytes: BTreeSet::new(),
                any: true,
                eps: false,
            },
        }
    }

    fn node_eps(&self, node: &Node) -> bool {
        self.first_set(node, &mut Vec::new()).eps
    }

    /// The Alternatives lint's errors: identical branches, literal
    /// shadowing, and the nullable branch that shadows non-nullable
    /// later branches (all compile rejections).
    fn alternatives_errors(&self) -> Vec<String> {
        let mut errors = Vec::new();
        for (name, node) in &self.rules {
            if node.kind != "alt" {
                continue;
            }
            let branches = node.branches.as_deref().unwrap_or_default();
            for (index, branch) in branches.iter().enumerate() {
                if self.node_eps(branch) && index < branches.len() - 1 {
                    let later = &branches[index + 1..];
                    if !later.iter().all(|candidate| self.node_eps(candidate)) {
                        errors.push(format!(
                            "rule {name}: branch {} can match empty input and shadows all later branches",
                            index + 1
                        ));
                    }
                }
                for (offset, later_branch) in branches[index + 1..].iter().enumerate() {
                    let later_number = index + offset + 2;
                    if let Some(error) =
                        self.compare_branches(name, branch, index + 1, later_branch, later_number)
                    {
                        errors.push(error);
                    }
                }
            }
        }
        errors
    }

    /// The Alternatives lint's envelope warnings: every ordered pair
    /// whose order decides the match, in rule/pair order.
    fn alternatives_order_warnings(&self) -> Vec<String> {
        let mut warnings = Vec::new();
        for (name, node) in &self.rules {
            if node.kind != "alt" {
                continue;
            }
            let branches = node.branches.as_deref().unwrap_or_default();
            for (index, branch) in branches.iter().enumerate() {
                for (offset, later_branch) in branches[index + 1..].iter().enumerate() {
                    if let Some(warning) = self.order_warning(
                        name,
                        branch,
                        index + 1,
                        later_branch,
                        index + offset + 2,
                    ) {
                        warnings.push(warning);
                    }
                }
            }
        }
        warnings
    }

    fn compare_branches(
        &self,
        name: &str,
        earlier: &Node,
        earlier_number: usize,
        later: &Node,
        later_number: usize,
    ) -> Option<String> {
        if let (Some(earlier_lit), Some(later_lit)) =
            (literal_string(earlier), literal_string(later))
        {
            if earlier_lit == later_lit {
                return Some(format!(
                    "rule {name}: branches {earlier_number} and {later_number} are identical"
                ));
            }
            if later_lit.starts_with(&earlier_lit) {
                return Some(format!(
                    "rule {name}: branch {earlier_number} ({earlier_lit:?}) shadows branch {later_number} ({later_lit:?}) - reorder longest-first or the shorter always wins"
                ));
            }
            // A case-insensitive earlier literal matches every casing
            // the later literal could: the later branch is dead (the
            // same class as the prefix shadow above).
            if earlier.fold == Some(true) && later_lit.eq_ignore_ascii_case(&earlier_lit) {
                return Some(format!(
                    "rule {name}: branch {later_number} ({later_lit:?}) can never match - branch {earlier_number} is case-insensitive and already matches {earlier_lit:?} in every casing"
                ));
            }
        }
        None
    }

    fn order_warning(
        &self,
        name: &str,
        earlier: &Node,
        earlier_number: usize,
        later: &Node,
        later_number: usize,
    ) -> Option<String> {
        if self
            .compare_branches(name, earlier, earlier_number, later, later_number)
            .is_some()
        {
            // The pair is claimed by an error (identical, shadowed, or
            // case-shadowed) — the envelope carries no warning for it.
            return None;
        }
        let mut stack = Vec::new();
        let first_earlier = self.first_set(earlier, &mut stack);
        let first_later = self.first_set(later, &mut stack);
        if first_earlier.any || first_later.any {
            return Some(format!(
                "rule {name}: branches {earlier_number} and {later_number} are order-dependent (first set not statically known)"
            ));
        }
        if !first_earlier.intersects(&first_later) {
            return None;
        }
        Some(format!(
            "rule {name}: branches {earlier_number} and {later_number} are order-dependent (shared first bytes; ordered choice is decisive)"
        ))
    }
}

/// The literal spelling of a node, when one exists (lit, or an
/// all-literal sequence): the shadowing check's operand.
fn literal_string(node: &Node) -> Option<String> {
    match node.kind.as_str() {
        "lit" => node.text.clone(),
        "seq" => {
            let items = node.items.as_deref().unwrap_or_default();
            if items.iter().all(|child| child.kind == "lit") {
                Some(
                    items
                        .iter()
                        .filter_map(|child| child.text.clone())
                        .collect(),
                )
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Rule references inside a node, in order (the skip-exemption
/// seed).
fn collect_node_refs(node: &Node, names: &mut Vec<String>) {
    match node.kind.as_str() {
        "ref" => {
            let name = node.name.clone().unwrap_or_default();
            if !names.contains(&name) {
                names.push(name);
            }
        }
        "seq" | "alt" => {
            for child in node.items.iter().chain(node.branches.iter()).flatten() {
                collect_node_refs(child, names);
            }
        }
        "rep" | "opt" | "cap" | "pred" => {
            if let Some(child) = node.child.as_deref() {
                collect_node_refs(child, names);
            }
        }
        _ => {}
    }
}

/// The first literal in a rule body's pre-order walk (refs are not
/// followed — the caller resolves them one level at a time).
fn leading_literal(node: &Node) -> Option<String> {
    match node.kind.as_str() {
        "lit" => node.text.clone(),
        "seq" | "alt" => node
            .items
            .iter()
            .chain(node.branches.iter())
            .flatten()
            .find_map(leading_literal),
        "rep" | "opt" | "cap" | "pred" => node.child.as_deref().and_then(leading_literal),
        _ => None,
    }
}

fn substitute_refs(node: &Node, from: &str, to: &str) -> Node {
    if node.kind == "ref" {
        if node.name.as_deref() == Some(from) {
            return Node {
                kind: "ref".to_string(),
                name: Some(to.to_string()),
                ..Node::default()
            };
        }
        return node.clone();
    }
    match node.kind.as_str() {
        "seq" => Node {
            kind: "seq".to_string(),
            items: Some(
                node.items
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .map(|child| substitute_refs(child, from, to))
                    .collect(),
            ),
            ..Node::default()
        },
        "alt" => Node {
            kind: "alt".to_string(),
            branches: Some(
                node.branches
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .map(|child| substitute_refs(child, from, to))
                    .collect(),
            ),
            ..Node::default()
        },
        "rep" => Node {
            kind: "rep".to_string(),
            child: Some(Box::new(substitute_refs(
                node.child.as_deref().expect("rep child"),
                from,
                to,
            ))),
            min: node.min,
            max: node.max,
            ..Node::default()
        },
        "opt" => Node {
            kind: "opt".to_string(),
            child: Some(Box::new(substitute_refs(
                node.child.as_deref().expect("opt child"),
                from,
                to,
            ))),
            ..Node::default()
        },
        "pred" => Node {
            kind: "pred".to_string(),
            positive: node.positive,
            behind: node.behind,
            child: Some(Box::new(substitute_refs(
                node.child.as_deref().expect("pred child"),
                from,
                to,
            ))),
            ..Node::default()
        },
        "cap" => Node {
            kind: "cap".to_string(),
            name: node.name.clone(),
            child: Some(Box::new(substitute_refs(
                node.child.as_deref().expect("cap child"),
                from,
                to,
            ))),
            ..Node::default()
        },
        _ => node.clone(),
    }
}

/// Compile a document into the portable grammar for `entry` — the
/// atoms stream and root the Ruby compiler's entry envelope carries
/// (`{atoms, root}`; bindings ride the envelope separately).
pub fn compile_entry_mode(
    document: &Document,
    entry: &str,
    tables: &Tables,
    atomic_rules: &[String],
    mode: Option<&str>,
) -> Result<Grammar, CompileError> {
    let rule = document
        .entries
        .iter()
        .find(|(name, _)| name == entry)
        .map(|(_, rule)| rule.clone())
        .ok_or_else(|| CompileError {
            message: format!("unknown entry {entry:?}"),
        })?;

    let mut compiler = Compiler::new_mode(document, tables, atomic_rules, mode)?;
    let tree = compiler.rule_tree(&rule)?;
    compiler.flatten(&tree)
}

/// The pre-modern shape (no capture mode): none of the structural
/// fixtures declare one.
pub fn compile_entry_with(
    document: &Document,
    entry: &str,
    tables: &Tables,
    atomic_rules: &[String],
) -> Result<Grammar, CompileError> {
    compile_entry_mode(document, entry, tables, atomic_rules, None)
}

/// Compile without `atomic` rules — the pre-4b shape (none of the
/// structural fixtures declare one).
pub fn compile_entry(
    document: &Document,
    entry: &str,
    tables: &Tables,
) -> Result<Grammar, CompileError> {
    compile_entry_with(document, entry, tables, &[])
}

/// The prepared compiler behind the envelope assembly: expanded,
/// warmed, lints run.
pub fn compiler_for<'a>(
    document: &'a Document,
    tables: &'a Tables,
    atomic_rules: &[String],
) -> Result<Compiler<'a>, CompileError> {
    compiler_for_mode(document, tables, atomic_rules, None)
}

/// The prepared compiler with the header's capture mode (ruby#304's
/// `mode "modern"`); none is the classic dialect.
pub fn compiler_for_mode<'a>(
    document: &'a Document,
    tables: &'a Tables,
    atomic_rules: &[String],
    mode: Option<&str>,
) -> Result<Compiler<'a>, CompileError> {
    Compiler::new_mode(document, tables, atomic_rules, mode)
}
