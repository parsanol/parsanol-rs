//! Portable core module for Parsanol
//!
//! This module contains the pure Rust parsing implementation that can be
//! compiled to both native Ruby extensions and WASM for Opal.
//!
//! # Module Organization
//!
//! The module is organized into the following areas:
//!
//! ## Core Types
//! - [`AstArena`] - Arena allocator for AST nodes
//! - [`AstNode`] - AST node type (16 bytes, Copy)
//! - [`Grammar`] - PEG grammar definition
//! - [`PortableParser`] - Main parser type
//! - [`ParseContext`] - Mutable parsing context
//!
//! ## Parser DSL
//! - [`parser_dsl`] - Parser combinator DSL
//!
//! ## Caching
//! - [`DenseCache`] - Dense packrat cache
//! - [`CacheEntry`] - Cache entry type
//!
//! ## Error Handling
//! - [`error`] - Rich error reporting
//!
//! ## Transforms
//! - [`transform`] - AST transformation utilities
//!
//! ## Infix Parsing
//! - [`infix`] - Infix expression parsing with precedence
//!
//! ## Incremental Parsing
//! - [`incremental`] - Incremental parsing support
//!
//! ## Streaming Parsing
//! - [`streaming`] - Streaming parser for large inputs
//!
//! ## Source Location
//! - [`source_location`] - Line/column tracking
//!
//! ## Visitor Pattern
//! - [`visitor`] - AST visitor utilities

// ============================================================================
// Module Declarations
// ============================================================================

pub mod arena;
pub mod ast;
pub mod cache;
pub mod char_class;
pub mod custom;
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
pub mod plugin;
pub mod regex_cache;
pub mod source_location;
pub mod source_map;
pub mod streaming;
pub mod streaming_builder;
pub mod transform;
pub mod visitor;

// Parallel parsing (always available, uses rayon when feature is enabled)
pub mod parallel;

// C ABI for external language bindings
pub mod c_ffi;

// ============================================================================
// Core Types
// ============================================================================

pub use arena::AstArena;
pub use ast::{AstNode, ParseError, ParseResult};
pub use grammar::{Atom, AtomTypeCounter, AtomVisitor, Grammar};
pub use parser::{ParseContext, ParserConfig, PortableParser};

// ============================================================================
// Error Handling
// ============================================================================

pub use error::{ErrorSeverity, RichError};

// ============================================================================
// Caching
// ============================================================================

pub use cache::{CacheEntry, DenseCache};

// ============================================================================
// Character Classes
// ============================================================================

pub use char_class::{utf8_char_len, CharClassTables, CharacterPattern, CHAR_CLASSES};

// ============================================================================
// Regex Cache
// ============================================================================

pub use regex_cache::{get_or_compile as get_regex, stats as regex_stats, CacheStats};

// ============================================================================
// FFI Utilities
// ============================================================================

pub use ffi::{
    flatten_ast, flatten_ast_to_u64, parse_to_flat, TAG_ARRAY_END, TAG_ARRAY_START, TAG_BOOL,
    TAG_FLOAT, TAG_HASH_END, TAG_HASH_KEY, TAG_HASH_START, TAG_INT, TAG_NIL, TAG_STRING,
};

// ============================================================================
// Grammar Analysis
// ============================================================================

pub use grammar_analysis::{GrammarAnalyzer, GrammarWarning, WarningKind};

// ============================================================================
// Transforms
// ============================================================================

pub use transform::{DirectTransform, TransformError, Value};

// ============================================================================
// Incremental Parsing
// ============================================================================

pub use incremental::{
    DirtyRegion, DirtyRegionTracker, Edit, IncrementalParser, IncrementalResult,
};

// ============================================================================
// Streaming Parsing
// ============================================================================

pub use streaming::{ChunkConfig, ChunkSource, StreamingError, StreamingParser, StreamingResult};

pub use streaming_builder::{
    walk_ast, BuildError, BuildResult, BuilderNodeCounter, BuilderStringCollector, DebugBuilder,
    DepthTracker, StreamingBuilder,
};

// ============================================================================
// Source Location
// ============================================================================

pub use source_location::{
    get_line_at_offset, offset_to_line_col, SourceContext, SourcePosition, SourceSpan,
};

// ============================================================================
// Visitor Pattern
// ============================================================================

pub use visitor::{walk, DefaultVisitor, DepthAnalyzer, NodeCounter, StringCollector, Visitor};

// ============================================================================
// Source Map
// ============================================================================

pub use source_map::{SourceMapBuilder, SourceMapCollection, SourceMapped};

// ============================================================================
// Parslet Transform
// ============================================================================

pub use parslet_transform::to_parslet_compatible;

// ============================================================================
// Parallel Parsing
// ============================================================================

pub use parallel::{parse_batch_parallel, parse_batch_parallel_owned, ParallelConfig};

// ============================================================================
// Plugin Architecture
// ============================================================================

pub use plugin::{
    AtomRegistry, ParsanolPlugin, PluginInfo, PluginRegistry, TransformRegistry,
    clear_plugins, get_plugin_info, has_plugin, list_plugins, plugin_count, register_plugin,
    unregister_plugin,
};

