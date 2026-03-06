//! Parser DSL - Idiomatic Rust Grammar Definition
//!
//! This module provides a fluent, composable API for defining PEG grammars
//! in Rust, similar to Parslet in Ruby.
//!
//! # Example
//!
//! ```rust
//! use parsanol::portable::parser_dsl::*;
//!
//! // Define a simple grammar
//! let grammar = GrammarBuilder::new()
//!     .rule("greeting", str("hello").then(str("world")))
//!     .build();
//! ```

use super::grammar::{Atom, Grammar};
use std::collections::HashMap;

/// Parslet trait - implemented by all parser combinators
pub trait Parslet: Send + Sync {
    /// Build this parslet into a Grammar
    fn build(self, builder: &mut GrammarBuilder) -> usize;
}

/// Grammar builder for constructing grammars
pub struct GrammarBuilder {
    /// All atoms in the grammar
    atoms: Vec<Atom>,

    /// Named rules and their atom indices
    rules: HashMap<String, usize>,

    /// For tracking forward references
    pending_entities: HashMap<usize, String>,

    /// Track insertion order for rules (first rule = root)
    first_rule: Option<String>,

    /// Last import map (if any)
    last_import: Option<ImportMap>,
}

impl GrammarBuilder {
    /// Create a new grammar builder
    pub fn new() -> Self {
        Self {
            atoms: Vec::new(),
            rules: HashMap::new(),
            pending_entities: HashMap::new(),
            first_rule: None,
            last_import: None,
        }
    }

    /// Add a rule to the grammar
    pub fn rule(mut self, name: &str, parslet: impl Parslet) -> Self {
        let atom_idx = parslet.build(&mut self);
        self.rules.insert(name.to_string(), atom_idx);
        // Track first rule for root
        if self.first_rule.is_none() {
            self.first_rule = Some(name.to_string());
        }
        self
    }

    /// Add a rule to the grammar (mutable version for chaining with import)
    pub fn rule_mut(&mut self, name: &str, parslet: impl Parslet) -> &mut Self {
        let atom_idx = parslet.build(self);
        self.rules.insert(name.to_string(), atom_idx);
        // Track first rule for root
        if self.first_rule.is_none() {
            self.first_rule = Some(name.to_string());
        }
        self
    }

    /// Add an atom directly
    pub fn add_atom(&mut self, atom: Atom) -> usize {
        let idx = self.atoms.len();
        self.atoms.push(atom);
        idx
    }

    /// Update an existing rule to point to a different atom
    pub fn update_rule(&mut self, name: &str, atom_idx: usize) -> &mut Self {
        self.rules.insert(name.to_string(), atom_idx);
        self
    }

    /// Register a forward reference
    pub fn add_forward_ref(&mut self, atom_idx: usize, rule_name: String) {
        self.pending_entities.insert(atom_idx, rule_name);
    }

    /// Build the final grammar
    pub fn build(self) -> Grammar {
        // Resolve any pending entity references
        let mut atoms = self.atoms;
        for (idx, rule_name) in self.pending_entities {
            if let Some(Atom::Entity { atom }) = atoms.get_mut(idx) {
                if let Some(&target_idx) = self.rules.get(&rule_name) {
                    *atom = target_idx;
                }
            }
        }

        // Use first rule as root (preserving insertion order)
        let root = self
            .first_rule
            .and_then(|name| self.rules.get(&name).copied())
            .unwrap_or(0);

        Grammar { atoms, root }
    }

    /// Get the current number of atoms
    pub fn atom_count(&self) -> usize {
        self.atoms.len()
    }

    /// Update an existing atom at the given index
    ///
    /// This is used for implementing recursive grammars where an atom
    /// needs to reference another atom that didn't exist when it was created.
    pub fn update_atom(&mut self, idx: usize, atom: Atom) -> &mut Self {
        if idx < self.atoms.len() {
            self.atoms[idx] = atom;
        }
        self
    }

    /// Get a reference to an atom by index
    pub fn get_atom(&self, idx: usize) -> Option<&Atom> {
        self.atoms.get(idx)
    }

    // ========================================================================
    // Capture, Scope, and Dynamic Helpers
    // ========================================================================

    /// Create a capture atom
    ///
    /// Captures the matched text with a name for later reference.
    ///
    /// # Example
    ///
    /// ```rust
    /// use parsanol::portable::parser_dsl::*;
    ///
    /// let grammar = GrammarBuilder::new()
    ///     .rule("greeting", capture("name", str("hello")))
    ///     .build();
    /// ```
    pub fn capture<N: Into<String>, P: Parslet>(&mut self, name: N, parslet: P) -> usize {
        let atom = parslet.build(self);
        self.add_atom(Atom::Capture {
            name: name.into(),
            atom,
        })
    }

    /// Create a scope atom
    ///
    /// Creates an isolated capture scope. Captures made within this scope
    /// are discarded when the scope ends.
    ///
    /// # Example
    ///
    /// ```rust
    /// use parsanol::portable::parser_dsl::*;
    ///
    /// let grammar = GrammarBuilder::new()
    ///     .rule("isolated", scope(str("a").then(str("b"))))
    ///     .build();
    /// ```
    pub fn scope<P: Parslet>(&mut self, parslet: P) -> usize {
        let atom = parslet.build(self);
        self.add_atom(Atom::Scope { atom })
    }

    /// Create a dynamic atom with a callback
    ///
    /// The callback is invoked at parse time to determine which atom to parse.
    /// The callback receives the current parsing context (input, position, captures)
    /// and returns an atom to parse, or None to fail.
    ///
    /// # Example
    ///
    /// ```rust
    /// use parsanol::portable::parser_dsl::*;
    /// use parsanol::portable::dynamic::{DynamicContext, ConstCallback};
    /// use parsanol::portable::grammar::Atom;
    ///
    /// // Create a callback that returns a constant atom
    /// let callback = ConstCallback::new(
    ///     Atom::Str { pattern: "dynamic".to_string() },
    ///     "const_dynamic"
    /// );
    ///
    /// let callback_id = parsanol::portable::dynamic::register_dynamic_callback(Box::new(callback));
    ///
    /// let grammar = GrammarBuilder::new()
    ///     .rule("dynamic_rule", dynamic_with_id(callback_id))
    ///     .build();
    /// ```
    pub fn dynamic(&mut self, callback_id: u64) -> usize {
        self.add_atom(Atom::Dynamic { callback_id })
    }

    /// Import all atoms from another grammar
    ///
    /// This allows composing grammars by importing rules from another grammar.
    /// The optional prefix is added to all imported rule names.
    ///
    /// # Example
    ///
    /// ```rust
    /// use parsanol::portable::parser_dsl::*;
    ///
    /// // Create a JSON value grammar
    /// let json_grammar = GrammarBuilder::new()
    ///     .rule("value", str("null"))
    ///     .build();
    ///
    /// // Import it into another grammar
    /// let mut builder = GrammarBuilder::new();
    /// builder.import(&json_grammar, Some("json"));
    /// builder.rule_mut("request", ref_("json:root"));
    /// let api_grammar = builder.build();
    /// ```
    ///
    /// # Arguments
    ///
    /// * `grammar` - The grammar to import
    /// * `prefix` - Optional prefix for imported rule names (e.g., "json" -> "json:root")
    pub fn import(&mut self, grammar: &Grammar, prefix: Option<&str>) -> &mut Self {
        let base_offset = self.atoms.len();
        let import_map = ImportMap {
            offset: base_offset,
            root: grammar.root + base_offset,
            rule_count: grammar.atoms.len(),
        };

        // Clone and remap all atoms
        for atom in &grammar.atoms {
            let remapped = remap_atom(atom, base_offset);
            self.atoms.push(remapped);
        }

        // Store the root for reference
        if let Some(pfx) = prefix {
            let root_name = format!("{}:root", pfx);
            self.rules.insert(root_name, import_map.root);
        }

        self.last_import = Some(import_map);
        self
    }

    /// Get the last import map (if any)
    #[inline]
    pub fn last_import(&self) -> Option<&ImportMap> {
        self.last_import.as_ref()
    }

    /// Import with explicit rule mappings
    ///
    /// This is a more flexible version that allows specifying which rules
    /// from the imported grammar should be exposed.
    pub fn import_with_rules(
        &mut self,
        grammar: &Grammar,
        prefix: &str,
        rules: &[(&str, usize)],
    ) -> &mut Self {
        self.import(grammar, Some(prefix));

        // Register specific rules
        if let Some(import_map) = &self.last_import {
            for (rule_name, old_idx) in rules {
                let new_idx = old_idx + import_map.offset;
                let prefixed_name = format!("{}:{}", prefix, rule_name);
                self.rules.insert(prefixed_name, new_idx);
            }
        }

        self
    }
}

/// Result of importing a grammar
#[derive(Debug, Clone)]
pub struct ImportMap {
    /// The base offset added to all atom indices
    pub offset: usize,
    /// The index of the imported grammar's root in the new grammar
    pub root: usize,
    /// Number of rules imported
    pub rule_count: usize,
}

impl ImportMap {
    /// Map an old index to the new index
    #[inline]
    pub fn map_index(&self, old_idx: usize) -> usize {
        old_idx + self.offset
    }
}

/// Remap atom indices by adding an offset
fn remap_atom(atom: &Atom, offset: usize) -> Atom {
    match atom {
        Atom::Str { pattern } => Atom::Str {
            pattern: pattern.clone(),
        },
        Atom::Re { pattern } => Atom::Re {
            pattern: pattern.clone(),
        },
        Atom::Sequence { atoms } => Atom::Sequence {
            atoms: atoms.iter().map(|&idx| idx + offset).collect(),
        },
        Atom::Alternative { atoms } => Atom::Alternative {
            atoms: atoms.iter().map(|&idx| idx + offset).collect(),
        },
        Atom::Repetition { atom, min, max } => Atom::Repetition {
            atom: atom + offset,
            min: *min,
            max: *max,
        },
        Atom::Named { name, atom } => Atom::Named {
            name: name.clone(),
            atom: atom + offset,
        },
        Atom::Entity { atom } => Atom::Entity {
            atom: atom + offset,
        },
        Atom::Lookahead { atom, positive } => Atom::Lookahead {
            atom: atom + offset,
            positive: *positive,
        },
        Atom::Cut => Atom::Cut,
        Atom::Ignore { atom } => Atom::Ignore {
            atom: atom + offset,
        },
        Atom::Capture { name, atom } => Atom::Capture {
            name: name.clone(),
            atom: atom + offset,
        },
        Atom::Scope { atom } => Atom::Scope {
            atom: atom + offset,
        },
        Atom::Dynamic { callback_id } => Atom::Dynamic {
            callback_id: *callback_id,
        },
        Atom::Custom { id } => Atom::Custom { id: *id },
    }
}

impl Default for GrammarBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Parser Combinators - Fundamental Building Blocks
// ============================================================================

/// Match a literal string
#[derive(Clone, Copy)]
pub struct Str<'a>(pub &'a str);

impl<'a> Parslet for Str<'a> {
    fn build(self, builder: &mut GrammarBuilder) -> usize {
        builder.add_atom(Atom::Str {
            pattern: self.0.to_string(),
        })
    }
}

/// Match a regular expression
#[derive(Clone, Copy)]
pub struct Re<'a>(pub &'a str);

impl<'a> Parslet for Re<'a> {
    fn build(self, builder: &mut GrammarBuilder) -> usize {
        builder.add_atom(Atom::Re {
            pattern: self.0.to_string(),
        })
    }
}

/// Match any single character
#[derive(Clone, Copy, Default)]
pub struct Any;

impl Parslet for Any {
    fn build(self, builder: &mut GrammarBuilder) -> usize {
        builder.add_atom(Atom::Re {
            pattern: ".".to_string(),
        })
    }
}

/// A forward reference to a named rule (for recursive grammars)
#[derive(Clone, Copy)]
pub struct Ref<'a>(pub &'a str);

impl<'a> Parslet for Ref<'a> {
    fn build(self, builder: &mut GrammarBuilder) -> usize {
        let atom_idx = builder.add_atom(Atom::Entity { atom: 0 }); // Placeholder
        builder.add_forward_ref(atom_idx, self.0.to_string());
        atom_idx
    }
}

/// Sequence of two parslets (A >> B matches A then B)
#[derive(Clone, Copy)]
pub struct Sequence2<A, B> {
    first: A,
    second: B,
}

impl<A: Parslet, B: Parslet> Parslet for Sequence2<A, B> {
    fn build(self, builder: &mut GrammarBuilder) -> usize {
        let first_idx = self.first.build(builder);
        let second_idx = self.second.build(builder);
        builder.add_atom(Atom::Sequence {
            atoms: vec![first_idx, second_idx],
        })
    }
}

/// Alternative of two parslets (A | B tries A, then B)
#[derive(Clone, Copy)]
pub struct Alternative2<A, B> {
    first: A,
    second: B,
}

impl<A: Parslet, B: Parslet> Parslet for Alternative2<A, B> {
    fn build(self, builder: &mut GrammarBuilder) -> usize {
        let first_idx = self.first.build(builder);
        let second_idx = self.second.build(builder);
        builder.add_atom(Atom::Alternative {
            atoms: vec![first_idx, second_idx],
        })
    }
}

// ============================================================================
// Extended Sequence Types (Sequence3, Sequence4, Sequence5)
// ============================================================================

/// Sequence of three parslets
#[derive(Clone, Copy)]
pub struct Sequence3<A, B, C> {
    first: A,
    second: B,
    third: C,
}

impl<A: Parslet, B: Parslet, C: Parslet> Parslet for Sequence3<A, B, C> {
    fn build(self, builder: &mut GrammarBuilder) -> usize {
        let atoms = vec![
            self.first.build(builder),
            self.second.build(builder),
            self.third.build(builder),
        ];
        builder.add_atom(Atom::Sequence { atoms })
    }
}

/// Sequence of four parslets
#[derive(Clone, Copy)]
pub struct Sequence4<A, B, C, D> {
    first: A,
    second: B,
    third: C,
    fourth: D,
}

impl<A: Parslet, B: Parslet, C: Parslet, D: Parslet> Parslet for Sequence4<A, B, C, D> {
    fn build(self, builder: &mut GrammarBuilder) -> usize {
        let atoms = vec![
            self.first.build(builder),
            self.second.build(builder),
            self.third.build(builder),
            self.fourth.build(builder),
        ];
        builder.add_atom(Atom::Sequence { atoms })
    }
}

/// Sequence of five parslets
#[derive(Clone, Copy)]
pub struct Sequence5<A, B, C, D, E> {
    first: A,
    second: B,
    third: C,
    fourth: D,
    fifth: E,
}

impl<A: Parslet, B: Parslet, C: Parslet, D: Parslet, E: Parslet> Parslet
    for Sequence5<A, B, C, D, E>
{
    fn build(self, builder: &mut GrammarBuilder) -> usize {
        let atoms = vec![
            self.first.build(builder),
            self.second.build(builder),
            self.third.build(builder),
            self.fourth.build(builder),
            self.fifth.build(builder),
        ];
        builder.add_atom(Atom::Sequence { atoms })
    }
}

// ============================================================================
// Extended Alternative Types (Alternative3, Alternative4, Alternative5)
// ============================================================================

/// Alternative of three parslets
#[derive(Clone, Copy)]
pub struct Alternative3<A, B, C> {
    first: A,
    second: B,
    third: C,
}

impl<A: Parslet, B: Parslet, C: Parslet> Parslet for Alternative3<A, B, C> {
    fn build(self, builder: &mut GrammarBuilder) -> usize {
        let atoms = vec![
            self.first.build(builder),
            self.second.build(builder),
            self.third.build(builder),
        ];
        builder.add_atom(Atom::Alternative { atoms })
    }
}

/// Alternative of four parslets
#[derive(Clone, Copy)]
pub struct Alternative4<A, B, C, D> {
    first: A,
    second: B,
    third: C,
    fourth: D,
}

impl<A: Parslet, B: Parslet, C: Parslet, D: Parslet> Parslet for Alternative4<A, B, C, D> {
    fn build(self, builder: &mut GrammarBuilder) -> usize {
        let atoms = vec![
            self.first.build(builder),
            self.second.build(builder),
            self.third.build(builder),
            self.fourth.build(builder),
        ];
        builder.add_atom(Atom::Alternative { atoms })
    }
}

/// Alternative of five parslets
#[derive(Clone, Copy)]
pub struct Alternative5<A, B, C, D, E> {
    first: A,
    second: B,
    third: C,
    fourth: D,
    fifth: E,
}

impl<A: Parslet, B: Parslet, C: Parslet, D: Parslet, E: Parslet> Parslet
    for Alternative5<A, B, C, D, E>
{
    fn build(self, builder: &mut GrammarBuilder) -> usize {
        let atoms = vec![
            self.first.build(builder),
            self.second.build(builder),
            self.third.build(builder),
            self.fourth.build(builder),
            self.fifth.build(builder),
        ];
        builder.add_atom(Atom::Alternative { atoms })
    }
}

/// Repetition (A.repeat(n, m) matches A n to m times)
#[derive(Clone, Copy)]
pub struct Repeat<P> {
    inner: P,
    min: usize,
    max: Option<usize>,
}

impl<P: Parslet> Parslet for Repeat<P> {
    fn build(self, builder: &mut GrammarBuilder) -> usize {
        let inner_idx = self.inner.build(builder);
        builder.add_atom(Atom::Repetition {
            atom: inner_idx,
            min: self.min,
            max: self.max,
        })
    }
}

/// Named capture (A.label("name") captures A as "name")
#[derive(Clone, Copy)]
pub struct Named<'a, P> {
    inner: P,
    name: &'a str,
}

impl<'a, P: Parslet> Parslet for Named<'a, P> {
    fn build(self, builder: &mut GrammarBuilder) -> usize {
        let inner_idx = self.inner.build(builder);
        builder.add_atom(Atom::Named {
            name: self.name.to_string(),
            atom: inner_idx,
        })
    }
}

/// Lookahead (A.lookahead() doesn't consume input)
#[derive(Clone, Copy)]
pub struct Lookahead<P> {
    inner: P,
    positive: bool,
}

impl<P: Parslet> Parslet for Lookahead<P> {
    fn build(self, builder: &mut GrammarBuilder) -> usize {
        let inner_idx = self.inner.build(builder);
        builder.add_atom(Atom::Lookahead {
            atom: inner_idx,
            positive: self.positive,
        })
    }
}

/// Cut operator (commit to this branch, prevent backtracking)
#[derive(Clone, Copy, Default)]
pub struct Cut;

impl Parslet for Cut {
    fn build(self, builder: &mut GrammarBuilder) -> usize {
        builder.add_atom(Atom::Cut)
    }
}

/// A type-erased parslet (for heterogeneous sequences/choices)
pub struct ErasedParslet(Box<dyn DynParslet>);

/// Trait for type-erased parslets
pub trait DynParslet: Send + Sync {
    /// Build this parslet into a grammar
    fn build_boxed(self: Box<Self>, builder: &mut GrammarBuilder) -> usize;
}

impl<P: Parslet + 'static> DynParslet for P {
    fn build_boxed(self: Box<Self>, builder: &mut GrammarBuilder) -> usize {
        (*self).build(builder)
    }
}

impl Parslet for ErasedParslet {
    fn build(self, builder: &mut GrammarBuilder) -> usize {
        self.0.build_boxed(builder)
    }
}

/// Convert any parslet to a type-erased one
pub fn dynamic<P: Parslet + 'static>(p: P) -> ErasedParslet {
    ErasedParslet(Box::new(p))
}

/// A sequence of multiple parslets
pub struct Sequence<P>(pub Vec<P>);

impl<P: Parslet> Parslet for Sequence<P> {
    fn build(self, builder: &mut GrammarBuilder) -> usize {
        let indices: Vec<usize> = self.0.into_iter().map(|p| p.build(builder)).collect();
        builder.add_atom(Atom::Sequence { atoms: indices })
    }
}

/// A choice of multiple parslets
pub struct Choice<P>(pub Vec<P>);

impl<P: Parslet> Parslet for Choice<P> {
    fn build(self, builder: &mut GrammarBuilder) -> usize {
        let indices: Vec<usize> = self.0.into_iter().map(|p| p.build(builder)).collect();
        builder.add_atom(Atom::Alternative { atoms: indices })
    }
}

/// Capture parslet - stores matched text with a name
pub struct Capture<'a, P: Parslet> {
    name: &'a str,
    inner: P,
}

impl<'a, P: Parslet> Capture<'a, P> {
    /// Create a new capture parslet
    pub fn new(name: &'a str, inner: P) -> Self {
        Self { name, inner }
    }
}

impl<'a, P: Parslet> Parslet for Capture<'a, P> {
    fn build(self, builder: &mut GrammarBuilder) -> usize {
        let inner_idx = self.inner.build(builder);
        builder.add_atom(Atom::Capture {
            name: self.name.to_string(),
            atom: inner_idx,
        })
    }
}

/// Scope parslet - creates an isolated capture scope
pub struct Scope<P: Parslet> {
    inner: P,
}

impl<P: Parslet> Scope<P> {
    /// Create a new scope parslet
    pub fn new(inner: P) -> Self {
        Self { inner }
    }
}

impl<P: Parslet> Parslet for Scope<P> {
    fn build(self, builder: &mut GrammarBuilder) -> usize {
        let inner_idx = self.inner.build(builder);
        builder.add_atom(Atom::Scope { atom: inner_idx })
    }
}

/// DynamicAtom parslet - invokes a callback at parse time to get the atom to parse
/// This is different from `Dynamic` (type-erased parslet).
pub struct DynamicAtom {
    callback_id: u64,
}

impl DynamicAtom {
    /// Create a new dynamic atom parslet
    pub fn new(callback_id: u64) -> Self {
        Self { callback_id }
    }
}

impl Parslet for DynamicAtom {
    fn build(self, builder: &mut GrammarBuilder) -> usize {
        builder.add_atom(Atom::Dynamic {
            callback_id: self.callback_id,
        })
    }
}

// ============================================================================
// Extension trait for Parslet
// ============================================================================

/// Extension trait for Parslet with builder methods
pub trait ParsletExt: Parslet + Sized {
    /// Name the result
    fn label(self, name: &str) -> Named<'_, Self> {
        Named { inner: self, name }
    }

    /// Repeat this parser
    fn repeat(self, min: usize, max: Option<usize>) -> Repeat<Self> {
        Repeat {
            inner: self,
            min,
            max,
        }
    }

    /// Match zero or more times
    fn many(self) -> Repeat<Self> {
        Repeat {
            inner: self,
            min: 0,
            max: None,
        }
    }

    /// Match one or more times
    fn many1(self) -> Repeat<Self> {
        Repeat {
            inner: self,
            min: 1,
            max: None,
        }
    }

    /// Match optional (zero or one time)
    fn optional(self) -> Repeat<Self> {
        Repeat {
            inner: self,
            min: 0,
            max: Some(1),
        }
    }

    /// Positive lookahead (must match, doesn't consume)
    fn lookahead(self) -> Lookahead<Self> {
        Lookahead {
            inner: self,
            positive: true,
        }
    }

    /// Negative lookahead (must NOT match, doesn't consume)
    fn not_ahead(self) -> Lookahead<Self> {
        Lookahead {
            inner: self,
            positive: false,
        }
    }

    /// Sequence: A >> B
    fn then<B: Parslet>(self, other: B) -> Sequence2<Self, B> {
        Sequence2 {
            first: self,
            second: other,
        }
    }

    /// Alternative: A | B
    fn or<B: Parslet>(self, other: B) -> Alternative2<Self, B> {
        Alternative2 {
            first: self,
            second: other,
        }
    }

    /// Ignore the result (returns Nil, doesn't capture)
    /// This is useful for whitespace, delimiters, etc.
    fn ignore(self) -> Ignore<Self> {
        Ignore { inner: self }
    }
}

/// Ignore wrapper - matches but discards the result
#[derive(Clone, Copy)]
pub struct Ignore<P> {
    inner: P,
}

impl<P: Parslet> Parslet for Ignore<P> {
    fn build(self, builder: &mut GrammarBuilder) -> usize {
        let inner_idx = self.inner.build(builder);
        builder.add_atom(Atom::Ignore { atom: inner_idx })
    }
}

impl<T: Parslet + Sized> ParsletExt for T {}

// ============================================================================
// Operator Overloading (>> for sequence, | for alternative)
// ============================================================================

use std::ops::{BitOr, Shr};

// Shr (>>) for Sequence2 + third item -> Sequence3
impl<A: Parslet, B: Parslet, C: Parslet> Shr<C> for Sequence2<A, B> {
    type Output = Sequence3<A, B, C>;
    fn shr(self, rhs: C) -> Self::Output {
        Sequence3 {
            first: self.first,
            second: self.second,
            third: rhs,
        }
    }
}

// Shr (>>) for Sequence3 + fourth item -> Sequence4
impl<A: Parslet, B: Parslet, C: Parslet, D: Parslet> Shr<D> for Sequence3<A, B, C> {
    type Output = Sequence4<A, B, C, D>;
    fn shr(self, rhs: D) -> Self::Output {
        Sequence4 {
            first: self.first,
            second: self.second,
            third: self.third,
            fourth: rhs,
        }
    }
}

// Shr (>>) for Sequence4 + fifth item -> Sequence5
impl<A: Parslet, B: Parslet, C: Parslet, D: Parslet, E: Parslet> Shr<E> for Sequence4<A, B, C, D> {
    type Output = Sequence5<A, B, C, D, E>;
    fn shr(self, rhs: E) -> Self::Output {
        Sequence5 {
            first: self.first,
            second: self.second,
            third: self.third,
            fourth: self.fourth,
            fifth: rhs,
        }
    }
}

// BitOr (|) for Alternative2 + third item -> Alternative3
impl<A: Parslet, B: Parslet, C: Parslet> BitOr<C> for Alternative2<A, B> {
    type Output = Alternative3<A, B, C>;
    fn bitor(self, rhs: C) -> Self::Output {
        Alternative3 {
            first: self.first,
            second: self.second,
            third: rhs,
        }
    }
}

// BitOr (|) for Alternative3 + fourth item -> Alternative4
impl<A: Parslet, B: Parslet, C: Parslet, D: Parslet> BitOr<D> for Alternative3<A, B, C> {
    type Output = Alternative4<A, B, C, D>;
    fn bitor(self, rhs: D) -> Self::Output {
        Alternative4 {
            first: self.first,
            second: self.second,
            third: self.third,
            fourth: rhs,
        }
    }
}

// BitOr (|) for Alternative4 + fifth item -> Alternative5
impl<A: Parslet, B: Parslet, C: Parslet, D: Parslet, E: Parslet> BitOr<E>
    for Alternative4<A, B, C, D>
{
    type Output = Alternative5<A, B, C, D, E>;
    fn bitor(self, rhs: E) -> Self::Output {
        Alternative5 {
            first: self.first,
            second: self.second,
            third: self.third,
            fourth: self.fourth,
            fifth: rhs,
        }
    }
}

// ============================================================================
// Helper Functions (similar to Parslet)
// ============================================================================

/// Match a literal string
pub fn str(s: &str) -> Str<'_> {
    Str(s)
}

/// Match a regular expression
pub fn re(pattern: &str) -> Re<'_> {
    Re(pattern)
}

/// Match any single character
pub fn any() -> Any {
    Any
}

/// Forward reference to a rule
pub fn ref_(name: &str) -> Ref<'_> {
    Ref(name)
}

/// Cut (commit to this branch)
pub fn cut() -> Cut {
    Cut
}

/// Create a sequence from multiple parslets
pub fn seq<I, P>(items: I) -> Sequence<P>
where
    I: IntoIterator<Item = P>,
{
    Sequence(items.into_iter().collect())
}

/// Create a choice from multiple parslets
pub fn choice<I, P>(items: I) -> Choice<P>
where
    I: IntoIterator<Item = P>,
{
    Choice(items.into_iter().collect())
}

// ============================================================================
// Capture, Scope, and Dynamic Helpers
// ============================================================================

/// Create a capture parslet
///
/// # Example
///
/// ```rust
/// use parsanol::portable::parser_dsl::*;
///
/// let grammar = GrammarBuilder::new()
///     .rule("greeting", capture("name", str("hello")))
///     .build();
/// ```
pub fn capture<'a, P: Parslet>(name: &'a str, inner: P) -> Capture<'a, P> {
    Capture::new(name, inner)
}

/// Create a scope parslet
///
/// # Example
///
/// ```rust
/// use parsanol::portable::parser_dsl::*;
///
/// let grammar = GrammarBuilder::new()
///     .rule("isolated", scope(str("a").then(str("b"))))
///     .build();
/// ```
pub fn scope<P: Parslet>(inner: P) -> Scope<P> {
    Scope::new(inner)
}

/// Create a dynamic parslet with a pre-registered callback ID
///
/// # Example
///
/// ```rust
/// use parsanol::portable::parser_dsl::*;
/// use parsanol::portable::dynamic::{ConstCallback, register_dynamic_callback};
/// use parsanol::portable::grammar::Atom;
///
/// let callback = ConstCallback::new(
///     Atom::Str { pattern: "dynamic".to_string() },
///     "const_dynamic"
/// );
/// let callback_id = register_dynamic_callback(Box::new(callback));
///
/// let grammar = GrammarBuilder::new()
///     .rule("dynamic_rule", dynamic_with_id(callback_id))
///     .build();
/// ```
pub fn dynamic_with_id(callback_id: u64) -> DynamicAtom {
    DynamicAtom::new(callback_id)
}

// ============================================================================
// Macro for declarative grammar definition
// ============================================================================

/// Macro for building grammars declaratively
///
/// # Example
///
/// ```rust
/// use parsanol::portable::parser_dsl::grammar;
///
/// let grammar = grammar! {
///     "hello" => str("hello"),
///     "world" => str("world"),
/// };
/// ```
#[macro_export]
macro_rules! grammar {
    ($($name:expr => $parslet:expr),* $(,)?) => {{
        use $crate::portable::parser_dsl::*;
        let mut builder = GrammarBuilder::new();
        $(
            builder = builder.rule($name, $parslet);
        )*
        builder.build()
    }};
}

// Re-export at crate root
pub use crate::grammar;

// ============================================================================
// Ergonomic Macros for Arbitrary-Length Sequences and Alternatives
// ============================================================================

/// Create a sequence of parslets with dynamic boxing (ergonomic macro)
///
/// This macro provides an ergonomic way to create sequences of any length.
/// It wraps each element in `dynamic()` for heterogeneous type support.
///
/// # When to use
///
/// - **Use `all![]` when**: You need >5 elements, or heterogeneous parslet types
/// - **Use `.then()` / `>>` when**: You have <=5 elements of different types and want zero-allocation
/// - **Use `seq()` when**: All elements are the same type and you want Vec-based construction
///
/// # Examples
///
/// ```
/// use parsanol::portable::parser_dsl::*;
///
/// // Works for any length
/// let parser = all![str("a"), str("b"), str("c"), str("d"), str("e"), str("f")];
///
/// // Heterogeneous types work too
/// let parser = all![str("hello"), re("[0-9]+"), str("world")];
///
/// // Trailing comma is allowed
/// let parser = all![
///     str("a"),
///     str("b"),
///     str("c"),
/// ];
/// ```
#[macro_export]
macro_rules! parsanol_all {
    ($($p:expr),+ $(,)?) => {
        $crate::portable::parser_dsl::Sequence(vec![
            $($crate::portable::parser_dsl::dynamic($p)),+
        ])
    };
}

/// Create a choice/alternative of parslets with dynamic boxing (ergonomic macro)
///
/// This macro provides an ergonomic way to create alternatives of any length.
/// It wraps each element in `dynamic()` for heterogeneous type support.
///
/// # When to use
///
/// - **Use `oneof![]` when**: You need >5 alternatives, or heterogeneous parslet types
/// - **Use `.or()` / `|` when**: You have <=5 alternatives of different types and want zero-allocation
/// - **Use `choice()` when**: All alternatives are the same type and you want Vec-based construction
///
/// # Examples
///
/// ```
/// use parsanol::portable::parser_dsl::*;
///
/// // Works for any length
/// let parser = oneof![str("+"), str("-"), str("*"), str("/"), str("%"), str("^")];
///
/// // Heterogeneous types work too
/// let parser = oneof![str("true"), str("false"), re("yes|no")];
///
/// // Trailing comma is allowed
/// let parser = oneof![
///     str("a"),
///     str("b"),
///     str("c"),
/// ];
/// ```
#[macro_export]
macro_rules! parsanol_oneof {
    ($($p:expr),+ $(,)?) => {
        $crate::portable::parser_dsl::Choice(vec![
            $($crate::portable::parser_dsl::dynamic($p)),+
        ])
    };
}

// Re-export macros with shorter names at crate root
/// Alias for `parsanol_all!` - create a sequence of any length
#[macro_export]
macro_rules! all {
    ($($p:expr),+ $(,)?) => {
        $crate::parsanol_all![$($p),+]
    };
}

/// Alias for `parsanol_oneof!` - create a choice of any length
#[macro_export]
macro_rules! oneof {
    ($($p:expr),+ $(,)?) => {
        $crate::parsanol_oneof![$($p),+]
    };
}

// Re-export macros at module level
pub use crate::{all, oneof, parsanol_all, parsanol_oneof};

#[cfg(test)]
mod tests;
