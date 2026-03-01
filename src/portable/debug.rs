//! Developer Experience Tools
//!
//! This module provides debugging, tracing, and visualization tools for
//! developing and debugging parsers.
//!
//! # Features
//! - Parse tracing (step-by-step execution)
//! - Parse tree visualization (pretty printing)
//! - Grammar visualization (Mermaid/DOT diagrams)
//! - Error visualization

use super::arena::AstArena;
use super::ast::AstNode;
use super::grammar::{Atom, Grammar};
use std::fmt::Write;

/// Parse tree pretty printer
pub struct TreePrinter {
    /// Indentation string
    indent: String,
    /// Maximum depth to print
    max_depth: Option<usize>,
}

impl TreePrinter {
    /// Create a new tree printer
    pub fn new() -> Self {
        Self {
            indent: "  ".to_string(),
            max_depth: None,
        }
    }

    /// Set the indentation string
    pub fn indent(mut self, indent: &str) -> Self {
        self.indent = indent.to_string();
        self
    }

    /// Set the maximum depth to print
    pub fn max_depth(mut self, depth: usize) -> Self {
        self.max_depth = Some(depth);
        self
    }

    /// Print an AST node
    pub fn print(&self, node: &AstNode, arena: &AstArena, input: &str) -> String {
        let mut output = String::new();
        self.print_node(node, arena, input, 0, &mut output);
        output
    }

    fn print_node(
        &self,
        node: &AstNode,
        arena: &AstArena,
        input: &str,
        depth: usize,
        output: &mut String,
    ) {
        if let Some(max) = self.max_depth {
            if depth > max {
                writeln!(output, "{}...", &self.indent.repeat(depth)).unwrap();
                return;
            }
        }

        let indent = self.indent.repeat(depth);

        match node {
            AstNode::Nil => {
                writeln!(output, "{}nil", indent).unwrap();
            }
            AstNode::Bool(b) => {
                writeln!(output, "{}{}", indent, b).unwrap();
            }
            AstNode::Int(n) => {
                writeln!(output, "{}{}", indent, n).unwrap();
            }
            AstNode::Float(f) => {
                writeln!(output, "{}{:?}", indent, f).unwrap();
            }
            AstNode::StringRef { pool_index } => {
                let s = arena.get_string(*pool_index as usize);
                writeln!(output, "{}{:?}", indent, s).unwrap();
            }
            AstNode::InputRef { offset, length } => {
                let start = *offset as usize;
                let end = start + *length as usize;
                let s = &input[start..end.min(input.len())];
                writeln!(output, "{}{:?} @ {}..{}", indent, s, offset, end).unwrap();
            }
            AstNode::Array { pool_index, length } => {
                writeln!(output, "{}[", indent).unwrap();
                let items = arena.get_array(*pool_index as usize, *length as usize);
                for item in items {
                    self.print_node(&item, arena, input, depth + 1, output);
                }
                writeln!(output, "{}]", indent).unwrap();
            }
            AstNode::Hash { pool_index, length } => {
                writeln!(output, "{}{{", indent).unwrap();
                let pairs = arena.get_hash_items(*pool_index as usize, *length as usize);
                for (key, value) in pairs {
                    writeln!(output, "{}{}{}:", indent, self.indent, key).unwrap();
                    self.print_node(&value, arena, input, depth + 2, output);
                }
                writeln!(output, "{}}}", indent).unwrap();
            }
        }
    }
}

impl Default for TreePrinter {
    fn default() -> Self {
        Self::new()
    }
}

/// Grammar visualizer
pub struct GrammarVisualizer<'a> {
    grammar: &'a Grammar,
}

impl<'a> GrammarVisualizer<'a> {
    /// Create a new grammar visualizer
    pub fn new(grammar: &'a Grammar) -> Self {
        Self { grammar }
    }

    /// Generate a Mermaid diagram
    pub fn to_mermaid(&self) -> String {
        let mut output = String::new();
        output.push_str("graph TD\n");

        // Add root node
        writeln!(output, "  root[Root: {}]", self.grammar.root).unwrap();

        // Add all atoms
        for (i, atom) in self.grammar.atoms.iter().enumerate() {
            let label = self.atom_label(atom);
            writeln!(output, "  a{}[\"{}: {}\"]", i, i, label).unwrap();

            // Add connections
            match atom {
                Atom::Sequence { atoms } | Atom::Alternative { atoms } => {
                    for &child in atoms {
                        writeln!(output, "  a{} --> a{}", i, child).unwrap();
                    }
                }
                Atom::Repetition { atom, .. } => {
                    writeln!(output, "  a{} --> a{}", i, atom).unwrap();
                }
                Atom::Named { atom, .. } => {
                    writeln!(output, "  a{} --> a{}", i, atom).unwrap();
                }
                Atom::Entity { atom } => {
                    writeln!(output, "  a{} --> a{}", i, atom).unwrap();
                }
                Atom::Lookahead { atom, .. } => {
                    writeln!(output, "  a{} --> a{}", i, atom).unwrap();
                }
                _ => {}
            }
        }

        // Connect root to first atom
        if !self.grammar.atoms.is_empty() {
            writeln!(output, "  root --> a{}", self.grammar.root).unwrap();
        }

        output
    }

    /// Generate a GraphViz DOT diagram
    pub fn to_dot(&self) -> String {
        let mut output = String::new();
        output.push_str("digraph Grammar {\n");
        output.push_str("  rankdir=TB;\n");
        output.push_str("  node [shape=box];\n");

        // Add all atoms
        for (i, atom) in self.grammar.atoms.iter().enumerate() {
            let label = self.atom_label(atom);
            writeln!(output, "  a{} [label=\"{}: {}\"]", i, i, label).unwrap();

            // Add edges
            match atom {
                Atom::Sequence { atoms } | Atom::Alternative { atoms } => {
                    for &child in atoms {
                        writeln!(output, "  a{} -> a{}", i, child).unwrap();
                    }
                }
                Atom::Repetition { atom, .. } => {
                    writeln!(output, "  a{} -> a{}", i, atom).unwrap();
                }
                Atom::Named { atom, .. } => {
                    writeln!(output, "  a{} -> a{}", i, atom).unwrap();
                }
                Atom::Entity { atom } => {
                    writeln!(output, "  a{} -> a{}", i, atom).unwrap();
                }
                Atom::Lookahead { atom, .. } => {
                    writeln!(output, "  a{} -> a{}", i, atom).unwrap();
                }
                _ => {}
            }
        }

        // Mark root
        writeln!(
            output,
            "  a{} [style=filled, fillcolor=lightblue]",
            self.grammar.root
        )
        .unwrap();

        output.push_str("}\n");
        output
    }

    fn atom_label(&self, atom: &Atom) -> String {
        match atom {
            Atom::Str { pattern } => format!("str({:?})", pattern),
            Atom::Re { pattern } => format!("re({:?})", pattern),
            Atom::Sequence { atoms } => format!("seq({})", atoms.len()),
            Atom::Alternative { atoms } => format!("alt({})", atoms.len()),
            Atom::Repetition { atom: _, min, max } => {
                let max_str = max
                    .map(|m| m.to_string())
                    .unwrap_or_else(|| "∞".to_string());
                format!("rep({}..{})", min, max_str)
            }
            Atom::Named { name, .. } => format!("named({:?})", name),
            Atom::Entity { .. } => "entity".to_string(),
            Atom::Lookahead { positive, .. } => {
                if *positive {
                    "lookahead(+)".to_string()
                } else {
                    "lookahead(-)".to_string()
                }
            }
            Atom::Cut => "cut".to_string(),
            Atom::Ignore { atom } => format!("ignore(a{})", atom),
            Atom::Custom { id } => format!("custom({})", id),
        }
    }
}

/// Debug trace for parsing
#[derive(Debug, Clone)]
pub struct ParseTrace {
    /// Trace entries
    pub entries: Vec<TraceEntry>,
}

/// A single trace entry
#[derive(Debug, Clone)]
pub struct TraceEntry {
    /// Position in input
    pub position: usize,
    /// Atom being parsed
    pub atom_id: usize,
    /// What happened
    pub action: TraceAction,
    /// Depth in parse tree
    pub depth: usize,
}

/// Trace action
#[derive(Debug, Clone)]
pub enum TraceAction {
    /// Started parsing an atom
    Enter,
    /// Successfully matched
    Match {
        /// The length of the matched input
        length: usize,
    },
    /// Failed to match
    Fail,
    /// Cache hit
    CacheHit,
}

impl ParseTrace {
    /// Create a new empty trace
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Add an entry
    pub fn add(&mut self, entry: TraceEntry) {
        self.entries.push(entry);
    }

    /// Format as a readable string
    pub fn format(&self, grammar: &Grammar) -> String {
        let mut output = String::new();
        for entry in &self.entries {
            let indent = "  ".repeat(entry.depth);
            let _atom_name = grammar
                .get_atom(entry.atom_id)
                .map(|a| format!("{:?}", a))
                .unwrap_or_else(|| "unknown".to_string());

            match &entry.action {
                TraceAction::Enter => {
                    writeln!(
                        output,
                        "{}-> Enter atom {} at {}",
                        indent, entry.atom_id, entry.position
                    )
                    .unwrap();
                }
                TraceAction::Match { length } => {
                    writeln!(output, "{}   Match: {} bytes", indent, length).unwrap();
                }
                TraceAction::Fail => {
                    writeln!(output, "{}   Fail", indent).unwrap();
                }
                TraceAction::CacheHit => {
                    writeln!(output, "{}   Cache hit", indent).unwrap();
                }
            }
        }
        output
    }
}

impl Default for ParseTrace {
    fn default() -> Self {
        Self::new()
    }
}

/// Source code formatter for showing parse context
pub struct SourceFormatter;

impl SourceFormatter {
    /// Format a source line with position marker
    pub fn format_line(input: &str, offset: usize, context_lines: usize) -> String {
        let mut output = String::new();

        // Find line boundaries
        let mut lines: Vec<(usize, usize)> = Vec::new(); // (start, end)
        let mut line_start = 0;
        for (i, ch) in input.char_indices() {
            if ch == '\n' {
                lines.push((line_start, i));
                line_start = i + 1;
            }
        }
        lines.push((line_start, input.len()));

        // Find current line
        let current_line = lines
            .iter()
            .position(|&(start, end)| offset >= start && offset <= end)
            .unwrap_or(0);

        // Print context lines
        let start_line = current_line.saturating_sub(context_lines);
        let end_line = (current_line + context_lines + 1).min(lines.len());

        for (i, &(line_start, line_end)) in lines.iter().enumerate().take(end_line).skip(start_line)
        {
            let line_num = i + 1;
            let line_content = &input[line_start..line_end];

            // Line number and content
            writeln!(output, "{:4} | {}", line_num, line_content).unwrap();

            // Position marker for current line
            if i == current_line {
                let col = offset - line_start;
                write!(output, "     | ").unwrap();
                for _ in 0..col {
                    output.push(' ');
                }
                output.push_str("^\n");
            }
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::super::grammar::Grammar;
    use super::*;

    #[test]
    fn test_tree_printer() {
        let arena = AstArena::new();
        let node = arena.input_ref(0, 5);

        let printer = TreePrinter::new();
        let output = printer.print(&node, &arena, "hello world");

        assert!(output.contains("hello"));
    }

    #[test]
    fn test_grammar_visualizer() {
        let grammar = Grammar::new();
        let viz = GrammarVisualizer::new(&grammar);

        let mermaid = viz.to_mermaid();
        assert!(mermaid.contains("graph TD"));

        let dot = viz.to_dot();
        assert!(dot.contains("digraph"));
    }

    #[test]
    fn test_source_formatter() {
        let input = "line one\nline two\nline three";
        let formatted = SourceFormatter::format_line(input, 10, 1);

        assert!(formatted.contains("line two"));
    }

    #[test]
    fn test_parse_trace() {
        let mut trace = ParseTrace::new();
        trace.add(TraceEntry {
            position: 0,
            atom_id: 0,
            action: TraceAction::Enter,
            depth: 0,
        });
        trace.add(TraceEntry {
            position: 0,
            atom_id: 0,
            action: TraceAction::Match { length: 5 },
            depth: 0,
        });

        assert_eq!(trace.entries.len(), 2);
    }
}
