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
pub mod parslet_transform;
pub mod regex_cache;
pub mod source_location;
pub mod source_map;
pub mod streaming;
pub mod streaming_builder;
pub mod transform;
pub mod visitor;

// Parallel parsing (always available, uses rayon when feature is enabled)
pub mod parallel;

// Re-export commonly used types
pub use arena::AstArena;
pub use ast::{AstNode, ParseError, ParseResult};
pub use cache::{CacheEntry, DenseCache};
pub use char_class::{utf8_char_len, CharClassTables, CharacterPattern, CHAR_CLASSES};
pub use grammar::Atom;
pub use grammar::AtomTypeCounter;
pub use grammar::AtomVisitor;
pub use grammar::Grammar;
pub use parser::{ParseContext, ParserConfig, PortableParser};
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
pub use incremental::{
    DirtyRegion, DirtyRegionTracker, Edit, IncrementalParser, IncrementalResult,
};

// Re-export streaming parsing types
pub use streaming::{ChunkConfig, ChunkSource, StreamingError, StreamingParser, StreamingResult};

// Re-export streaming builder types
pub use streaming_builder::{
    walk_ast, BuildError, BuildResult, BuilderNodeCounter, BuilderStringCollector, DebugBuilder,
    DepthTracker, StreamingBuilder,
};

// Re-export source location utilities
pub use source_location::{
    get_line_at_offset, offset_to_line_col, SourceContext, SourcePosition, SourceSpan,
};

// Re-export visitor types
pub use visitor::{walk, DefaultVisitor, DepthAnalyzer, NodeCounter, StringCollector, Visitor};

// Re-export source map types
pub use source_map::{SourceMapBuilder, SourceMapCollection, SourceMapped};

// Re-export Parslet-compatible transform
pub use parslet_transform::to_parslet_compatible;

// Re-export parallel parsing types
pub use parallel::{parse_batch_parallel, parse_batch_parallel_owned, ParallelConfig};
