//! Infix Expression Parser
//!
//! This module provides built-in support for parsing infix expressions with
//! operator precedence and associativity.
//!
//! # Example
//!
//! ```rust
//! use parsanol::portable::parser_dsl::ref_;
//! use parsanol::portable::infix::{infix, Assoc};
//!
//! // Define operators with precedence
//! let builder = infix(
//!     ref_("primary"),       // Primary expression
//!     [("*", 2, Assoc::Left), ("+", 1, Assoc::Left)], // Operators
//! );
//! ```

use super::grammar::Atom;
use super::parser_dsl::{GrammarBuilder, Parslet, Ref, Str};

/// Operator associativity
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Assoc {
    /// Left-associative: a + b + c = (a + b) + c
    Left,
    /// Right-associative: a = b = c = a = (b = c)
    Right,
    /// Non-associative: a op b op c is an error
    NonAssoc,
}

/// An operator definition for infix parsing
#[derive(Clone)]
pub struct Operator<'a> {
    /// The parser for the operator
    pub op: Str<'a>,
    /// Precedence (higher = binds tighter)
    pub precedence: u8,
    /// Associativity
    pub associativity: Assoc,
}

impl<'a> Operator<'a> {
    /// Create a new operator
    pub fn new(op: &'a str, precedence: u8, associativity: Assoc) -> Self {
        Self {
            op: Str(op),
            precedence,
            associativity,
        }
    }
}

/// Builder for infix expression parsers
pub struct InfixBuilder<'a> {
    /// Primary expression parser (atoms like numbers, identifiers, parenthesized expressions)
    primary: Option<Ref<'a>>,
    /// Operators grouped by precedence level
    operators: Vec<(Vec<Str<'a>>, u8, Assoc)>,
    /// Custom rule name for the expression
    name: Option<&'a str>,
}

impl<'a> InfixBuilder<'a> {
    /// Create a new infix builder
    pub fn new() -> Self {
        Self {
            primary: None,
            operators: Vec::new(),
            name: None,
        }
    }

    /// Set the primary expression parser
    pub fn primary(mut self, primary: Ref<'a>) -> Self {
        self.primary = Some(primary);
        self
    }

    /// Add an operator
    pub fn op(mut self, op: &'a str, precedence: u8, associativity: Assoc) -> Self {
        // Find or create precedence level
        if let Some((ops, _, _)) = self
            .operators
            .iter_mut()
            .find(|(_, prec, assoc)| *prec == precedence && *assoc == associativity)
        {
            ops.push(Str(op));
        } else {
            self.operators
                .push((vec![Str(op)], precedence, associativity));
        }
        self
    }

    /// Set the rule name
    pub fn name(mut self, name: &'a str) -> Self {
        self.name = Some(name);
        self
    }

    /// Build the infix parser grammar
    ///
    /// Uses precedence climbing algorithm to generate efficient grammar.
    ///
    /// # Panics
    ///
    /// Panics if `.primary()` was not called before `.build()`.
    pub fn build(self, builder: &mut GrammarBuilder) -> usize {
        let primary_idx = self
            .primary
            .expect("InfixBuilder::primary() must be called before InfixBuilder::build()")
            .build(builder);

        // Sort operators by precedence (highest first)
        let mut operators = self.operators;
        operators.sort_by(|a, b| b.1.cmp(&a.1));

        if operators.is_empty() {
            return primary_idx;
        }

        // Build grammar from highest to lowest precedence
        let mut current_expr = primary_idx;

        for (ops, _prec, assoc) in operators {
            current_expr = Self::build_precedence_level(builder, current_expr, &ops, assoc);
        }

        current_expr
    }

    fn build_precedence_level(
        builder: &mut GrammarBuilder,
        operand: usize,
        ops: &[Str<'_>],
        assoc: Assoc,
    ) -> usize {
        // Create operator alternatives
        let op_indices: Vec<usize> = ops.iter().map(|op| op.build(builder)).collect();
        let op_atom = builder.add_atom(Atom::Alternative { atoms: op_indices });

        // Build based on associativity
        match assoc {
            Assoc::Left => {
                // Left-associative: ((a op b) op c) op d
                // Grammar: expr = term (op term)*
                let seq_idx = builder.add_atom(Atom::Sequence {
                    atoms: vec![op_atom, operand],
                });
                let repeat_idx = builder.add_atom(Atom::Repetition {
                    atom: seq_idx,
                    min: 0,
                    max: None,
                });
                builder.add_atom(Atom::Sequence {
                    atoms: vec![operand, repeat_idx],
                })
            }
            Assoc::Right => {
                // Right-associative: a op (b op (c op d))
                // Grammar: expr = operand (op expr)?
                // This requires recursion - we use forward references

                // Create a placeholder entity that will reference the result atom
                let placeholder_idx = builder.add_atom(Atom::Entity { atom: 0 });

                // Build the sequence: op expr (using placeholder for recursive ref)
                let seq_idx = builder.add_atom(Atom::Sequence {
                    atoms: vec![op_atom, placeholder_idx],
                });

                // Make it optional: (op expr)?
                let opt_idx = builder.add_atom(Atom::Repetition {
                    atom: seq_idx,
                    min: 0,
                    max: Some(1),
                });

                // Build the final expression: operand (op expr)?
                let expr_idx = builder.add_atom(Atom::Sequence {
                    atoms: vec![operand, opt_idx],
                });

                // Now resolve the forward reference: the placeholder should point to expr_idx
                // This creates the recursive structure needed for right-associativity
                builder.update_atom(placeholder_idx, Atom::Entity { atom: expr_idx });

                expr_idx
            }
            Assoc::NonAssoc => {
                // Non-associative: a op b (can't chain)
                // Grammar: expr = term (op term)?
                let seq_idx = builder.add_atom(Atom::Sequence {
                    atoms: vec![op_atom, operand],
                });
                let opt_idx = builder.add_atom(Atom::Repetition {
                    atom: seq_idx,
                    min: 0,
                    max: Some(1),
                });
                builder.add_atom(Atom::Sequence {
                    atoms: vec![operand, opt_idx],
                })
            }
        }
    }
}

impl<'a> Default for InfixBuilder<'a> {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience function to create an infix expression parser
///
/// # Arguments
/// * `primary` - Parser for primary expressions (numbers, identifiers, etc.)
/// * `operators` - Iterable of (operator, precedence, associativity) tuples
///
/// # Example
/// ```rust
/// use parsanol::portable::parser_dsl::{ref_, GrammarBuilder};
/// use parsanol::portable::infix::{infix, Assoc};
///
/// let mut builder = GrammarBuilder::new();
/// let expr_idx = infix(
///     ref_("primary"),
///     [("*", 2, Assoc::Left), ("+", 1, Assoc::Left)],
/// ).build(&mut builder);
/// ```
pub fn infix<'a, O>(primary: Ref<'a>, operators: O) -> InfixBuilder<'a>
where
    O: IntoIterator<Item = (&'a str, u8, Assoc)>,
{
    let mut builder = InfixBuilder::new();
    builder.primary = Some(primary);
    for (op, prec, assoc) in operators {
        builder = builder.op(op, prec, assoc);
    }
    builder
}

/// Precedence climbing parser for runtime parsing
///
/// This is an alternative approach that doesn't require generating a grammar.
/// Useful for more dynamic operator tables.
pub struct PrecedenceClimber {
    /// Operators by precedence level
    levels: Vec<PrecedenceLevel>,
}

/// A single precedence level
struct PrecedenceLevel {
    operators: Vec<String>,
    associativity: Assoc,
}

impl PrecedenceClimber {
    /// Create a new precedence climber
    pub fn new() -> Self {
        Self { levels: Vec::new() }
    }

    /// Add a precedence level
    pub fn add_level<I, S>(mut self, operators: I, associativity: Assoc) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.levels.push(PrecedenceLevel {
            operators: operators.into_iter().map(|s| s.into()).collect(),
            associativity,
        });
        self
    }

    /// Get the associativity for an operator
    pub fn associativity(&self, op: &str) -> Option<Assoc> {
        for level in &self.levels {
            if level.operators.iter().any(|o| o == op) {
                return Some(level.associativity);
            }
        }
        None
    }

    /// Check if an operator is defined
    pub fn is_operator(&self, op: &str) -> bool {
        self.levels
            .iter()
            .any(|l| l.operators.iter().any(|o| o == op))
    }
}

impl Default for PrecedenceClimber {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Precedence DSL Macro (Item 3.3)
// ============================================================================

/// Declarative macro for defining infix expression grammars.
///
/// This macro provides a clean DSL for defining operator precedence tables,
/// similar to Parslet's `infix_expression` but with Rust-native syntax.
///
/// # Syntax
///
/// ```rust,ignore
/// precedence! {
///     builder,
///     // Associativity, precedence, operator (one per line)
///     op(Left, 2, "*"),
///     op(Left, 2, "/"),
///     op(Left, 1, "+"),
///     op(Left, 1, "-"),
///     op(Right, 3, "**"),
///     // Primary expression (required, must be last)
///     primary: ref_("number"),
/// }
/// ```
///
/// # Example
///
/// ```rust
/// use parsanol::portable::parser_dsl::{GrammarBuilder, str, ref_};
/// use parsanol::precedence;
///
/// let mut builder = GrammarBuilder::new()
///     .rule("number", str("42"));
///
/// // Build infix expression grammar
/// let expr_idx = precedence! {
///     &mut builder,
///     op(Left, 1, "+"),
///     op(Left, 2, "*"),
///     primary: ref_("number"),
/// };
/// ```
#[macro_export]
macro_rules! precedence {
    // Entry point with builder
    ($builder:expr, $($input:tt)*) => {{
        let mut __infix_builder = $crate::portable::infix::InfixBuilder::new();
        __infix_builder = $crate::precedence!(@process __infix_builder, $($input)*);
        __infix_builder.build(&mut $builder)
    }};

    // Process op! directive with trailing comma
    (@process $builder:ident, op($assoc:ident, $prec:literal, $op:literal), $($rest:tt)*) => {
        {
            let __temp = $builder.op($op, $prec, $crate::portable::infix::Assoc::$assoc);
            $crate::precedence!(@process __temp, $($rest)*)
        }
    };

    // Process op! directive without trailing comma (last before primary)
    (@process $builder:ident, op($assoc:ident, $prec:literal, $op:literal) primary: $primary:expr $(,)?) => {
        $builder.op($op, $prec, $crate::portable::infix::Assoc::$assoc).primary($primary)
    };

    // Process primary only
    (@process $builder:ident, primary: $primary:expr $(,)?) => {
        $builder.primary($primary)
    };

    // Empty case (end of input)
    (@process $builder:ident,) => { $builder };
}

/// Create an operator precedence table at runtime.
///
/// This provides a more flexible alternative to `precedence!` macro when
/// operator tables need to be constructed dynamically.
///
/// # Example
///
/// ```rust
/// use parsanol::portable::infix::{PrecedenceTable, Assoc};
///
/// let table = PrecedenceTable::new()
///     .level(["*", "/"], 2, Assoc::Left)
///     .level(["+", "-"], 1, Assoc::Left)
///     .level(["**"], 3, Assoc::Right);
///
/// assert_eq!(table.precedence("*"), Some(2));
/// assert_eq!(table.precedence("+"), Some(1));
/// assert_eq!(table.associativity("**"), Some(Assoc::Right));
/// ```
#[derive(Debug, Clone, Default)]
pub struct PrecedenceTable {
    /// Operators with their precedence and associativity
    operators: Vec<(String, u8, Assoc)>,
}

impl PrecedenceTable {
    /// Create an empty precedence table
    pub fn new() -> Self {
        Self {
            operators: Vec::new(),
        }
    }

    /// Add a precedence level with multiple operators
    pub fn level<I, S>(mut self, operators: I, precedence: u8, associativity: Assoc) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for op in operators {
            self.operators.push((op.into(), precedence, associativity));
        }
        self
    }

    /// Add a single operator
    pub fn op(mut self, op: impl Into<String>, precedence: u8, associativity: Assoc) -> Self {
        self.operators.push((op.into(), precedence, associativity));
        self
    }

    /// Get the precedence of an operator
    pub fn precedence(&self, op: &str) -> Option<u8> {
        self.operators
            .iter()
            .find(|(o, _, _)| o == op)
            .map(|(_, p, _)| *p)
    }

    /// Get the associativity of an operator
    pub fn associativity(&self, op: &str) -> Option<Assoc> {
        self.operators
            .iter()
            .find(|(o, _, _)| o == op)
            .map(|(_, _, a)| *a)
    }

    /// Check if an operator is defined
    pub fn is_operator(&self, op: &str) -> bool {
        self.operators.iter().any(|(o, _, _)| o == op)
    }

    /// Get all operators at a given precedence level
    pub fn operators_at_level(&self, precedence: u8) -> Vec<&str> {
        self.operators
            .iter()
            .filter(|(_, p, _)| *p == precedence)
            .map(|(o, _, _)| o.as_str())
            .collect()
    }

    /// Get the number of defined operators
    pub fn len(&self) -> usize {
        self.operators.len()
    }

    /// Check if the table is empty
    pub fn is_empty(&self) -> bool {
        self.operators.is_empty()
    }

    /// Convert to an InfixBuilder for grammar generation
    pub fn to_infix_builder<'a>(&'a self, primary: super::parser_dsl::Ref<'a>) -> InfixBuilder<'a> {
        let mut builder = InfixBuilder::new().primary(primary);
        for (op, prec, assoc) in &self.operators {
            builder = builder.op(op, *prec, *assoc);
        }
        builder
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infix_builder() {
        let mut builder = GrammarBuilder::new();

        // First, add the primary rule (a simple number)
        let _primary_idx = builder.add_atom(Atom::Re {
            pattern: "[0-9]+".to_string(),
        });

        // Create infix parser with + and *
        let infix_builder = infix(
            Ref("primary"),
            [("+", 1, Assoc::Left), ("*", 2, Assoc::Left)],
        );

        // Note: This test shows the builder can be created; full testing requires
        // parser integration which needs more setup
        assert!(infix_builder.operators.len() == 2);
    }

    #[test]
    fn test_precedence_climber() {
        let climber = PrecedenceClimber::new()
            .add_level(["*".to_string(), "/".to_string()], Assoc::Left)
            .add_level(["+".to_string(), "-".to_string()], Assoc::Left);

        assert!(climber.is_operator("+"));
        assert!(climber.is_operator("*"));
        assert!(!climber.is_operator("^"));

        assert_eq!(climber.associativity("+"), Some(Assoc::Left));
        assert_eq!(climber.associativity("*"), Some(Assoc::Left));
    }

    #[test]
    fn test_operator_creation() {
        let op = Operator::new("+", 1, Assoc::Left);
        assert_eq!(op.precedence, 1);
        assert_eq!(op.associativity, Assoc::Left);
    }

    #[test]
    fn test_precedence_table() {
        let table = PrecedenceTable::new()
            .level(["*", "/"], 2, Assoc::Left)
            .level(["+", "-"], 1, Assoc::Left)
            .level(["**"], 3, Assoc::Right);

        assert_eq!(table.precedence("*"), Some(2));
        assert_eq!(table.precedence("+"), Some(1));
        assert_eq!(table.precedence("**"), Some(3));
        assert_eq!(table.precedence("%"), None);

        assert_eq!(table.associativity("*"), Some(Assoc::Left));
        assert_eq!(table.associativity("**"), Some(Assoc::Right));

        assert!(table.is_operator("+"));
        assert!(!table.is_operator("%"));

        assert_eq!(table.len(), 5);
    }

    #[test]
    fn test_precedence_table_operators_at_level() {
        let table = PrecedenceTable::new()
            .level(["*", "/"], 2, Assoc::Left)
            .level(["+", "-"], 1, Assoc::Left);

        let level_2 = table.operators_at_level(2);
        assert!(level_2.contains(&"*"));
        assert!(level_2.contains(&"/"));
        assert_eq!(level_2.len(), 2);
    }

    #[test]
    fn test_precedence_table_builder_conversion() {
        let table = PrecedenceTable::new()
            .level(["+", "-"], 1, Assoc::Left)
            .level(["*", "/"], 2, Assoc::Left);

        let builder = GrammarBuilder::new();
        builder.rule("primary", Str("42"));

        let infix_builder = table.to_infix_builder(Ref("primary"));
        assert_eq!(infix_builder.operators.len(), 2);
    }

    #[test]
    fn test_precedence_macro() {
        use super::super::parser_dsl::GrammarBuilder;
        let grammar_builder =
            GrammarBuilder::new().rule("number", super::super::parser_dsl::str("42"));
        let grammar = grammar_builder.build();
        // Verify grammar was created
        assert!(grammar.atom_count() >= 1);
    }

    #[test]
    fn test_precedence_table_to_builder() {
        use super::super::parser_dsl::ref_;

        let mut builder = super::super::parser_dsl::GrammarBuilder::new();

        // Test using PrecedenceTable
        let table = PrecedenceTable::new()
            .level(["+", "-"], 1, Assoc::Left)
            .level(["*", "/"], 2, Assoc::Left);

        let infix = table.to_infix_builder(ref_("number"));
        let _expr_idx = infix.build(&mut builder);
        assert!(builder.atom_count() > 4);
    }

    #[test]
    fn test_precedence_macro_multiple_ops() {
        use super::super::parser_dsl::ref_;

        let mut builder = super::super::parser_dsl::GrammarBuilder::new();

        // Test using InfixBuilder directly for multiple ops per level
        let infix = InfixBuilder::new()
            .primary(ref_("number"))
            .op("+", 1, Assoc::Left)
            .op("-", 1, Assoc::Left)
            .op("*", 2, Assoc::Left)
            .op("/", 2, Assoc::Left);

        let _expr_idx = infix.build(&mut builder);
        assert!(builder.atom_count() > 4);
    }
}
