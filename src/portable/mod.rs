//! Portable core module for Parsanol
//!
//! This module contains the pure Rust parsing implementation that can be
//! compiled to both native Ruby extensions and WASM for Opal.

pub mod arena;
pub mod ast;
pub mod cache;
pub mod char_class;
pub mod debug;
pub mod error;
pub mod ffi;
pub mod grammar;
pub mod grammar_analysis;
pub mod incremental;
pub mod infix;
pub mod parser;
pub mod parser_dsl;
pub mod regex_cache;
pub mod source_location;
pub mod source_map;
pub mod streaming;
pub mod transform;
pub mod visitor;

// Re-export commonly used types
pub use arena::AstArena;
pub use ast::{AstNode, ParseError, ParseResult};
pub use cache::{CacheEntry, DenseCache};
pub use char_class::{utf8_char_len, CharClassTables, CharacterPattern, CHAR_CLASSES};
pub use grammar::Atom;
pub use grammar::Grammar;
pub use parser::PortableParser;
pub use regex_cache::get_or_compile as get_regex;

// Re-export FFI utilities (including tags and flatten functions)
pub use ffi::{
    flatten_ast, flatten_ast_to_u64, parse_to_flat, TAG_ARRAY_END, TAG_ARRAY_START, TAG_BOOL,
    TAG_FLOAT, TAG_HASH_END, TAG_HASH_KEY, TAG_HASH_START, TAG_INT, TAG_NIL, TAG_STRING,
};

// Re-export grammar analysis types
pub use grammar_analysis::{GrammarAnalyzer, GrammarWarning, WarningKind};

// Re-export transform types
pub use transform::{DirectTransform, TransformError, Value};

// Re-export incremental parsing types
pub use incremental::{DirtyRegion, DirtyRegionTracker, Edit, IncrementalParser, IncrementalResult};

// Re-export streaming parsing types
pub use streaming::{ChunkConfig, ChunkSource, StreamingError, StreamingParser, StreamingResult};

// Re-export source location utilities
pub use source_location::{
    get_line_at_offset, offset_to_line_col, SourceContext, SourcePosition, SourceSpan,
};

// Re-export visitor types
pub use visitor::{DefaultVisitor, DepthAnalyzer, NodeCounter, StringCollector, Visitor, walk};

// Re-export source map types
pub use source_map::{SourceMapped, SourceMapCollection, SourceMapBuilder};
