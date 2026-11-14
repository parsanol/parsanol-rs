//! A grammar paired with its cached analyses (rs#486): construct once,
//! parse many. `PortableParser::new` recomputes the pure grammar
//! analyses per construction — fine for one-shot parses, wasteful for
//! the parse-many pattern (consumer harnesses that parse many inputs
//! against one grammar). The prepared form computes once and hands the
//! shared analysis to every parse through
//! `PortableParser::with_analysis`, matching `Grammar::parse`'s
//! construction defaults exactly.

use super::arena::AstArena;
use super::ast::{AstNode, ParseError, ParseResult};
use super::grammar::Grammar;
use super::parser::PortableParser;
use super::parser_analysis::ParserAnalysis;
use std::sync::Arc;

/// A grammar with its analyses computed once (see the module docs).
#[derive(Debug, Clone)]
pub struct PreparedGrammar {
    pub(crate) grammar: Arc<Grammar>,
    pub(crate) analysis: Arc<ParserAnalysis>,
}

impl PreparedGrammar {
    /// Compute the analyses once and keep them for every parse.
    pub fn new(grammar: Grammar) -> Self {
        let analysis = Arc::new(ParserAnalysis::compute(&grammar));
        Self {
            grammar: Arc::new(grammar),
            analysis,
        }
    }

    /// The wrapped grammar.
    pub fn grammar(&self) -> &Grammar {
        &self.grammar
    }

    /// Parse like `Grammar::parse`, reading the prepared analyses.
    pub fn parse(&self, input: &str) -> Result<AstNode, ParseError> {
        let mut arena = AstArena::for_input(input.len());
        let mut parser =
            PortableParser::with_analysis(&self.grammar, input, &mut arena, &self.analysis);
        parser.parse()
    }

    /// Parse like `Grammar::parse_with_pos`, reading the prepared
    /// analyses.
    pub fn parse_with_pos(&self, input: &str) -> Result<ParseResult, ParseError> {
        let mut arena = AstArena::for_input(input.len());
        let mut parser =
            PortableParser::with_analysis(&self.grammar, input, &mut arena, &self.analysis);
        parser.parse_with_end_pos()
    }
}
