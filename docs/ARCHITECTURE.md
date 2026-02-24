# Parsanol-rs Architecture

This document provides a comprehensive overview of the Parsanol-rs architecture,
including component diagrams, data flow, and extension points.

## Overview

Parsanol-rs is a high-performance PEG (Parsing Expression Grammar) parser library
written in Rust. It provides:

- **Packrat memoization** for O(n) parsing complexity
- **Arena allocation** for zero-copy AST construction
- **Streaming support** for large file parsing
- **Incremental parsing** for editor integration
- **Rich error reporting** with tree-structured error messages

## Component Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              PUBLIC API                                      │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│   ┌─────────────┐    ┌─────────────┐    ┌─────────────┐    ┌─────────────┐ │
│   │  Grammar    │    │  Portable   │    │ Streaming   │    │ Incremental │ │
│   │  (JSON/DSL) │    │   Parser    │    │   Parser    │    │   Parser    │ │
│   └──────┬──────┘    └──────┬──────┘    └──────┬──────┘    └──────┬──────┘ │
│          │                  │                  │                  │        │
│          └──────────────────┴──────────────────┴──────────────────┘        │
│                                     │                                       │
└─────────────────────────────────────┼───────────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                              CORE ENGINE                                     │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│   ┌─────────────┐    ┌─────────────┐    ┌─────────────┐    ┌─────────────┐ │
│   │    Cache    │    │    Arena    │    │   AstNode   │    │   Transform │ │
│   │ (DenseCache)│    │ (AstArena)  │    │   (Types)   │    │   (DSL)     │ │
│   └─────────────┘    └─────────────┘    └─────────────┘    └─────────────┘ │
│                                                                              │
│   ┌─────────────┐    ┌─────────────┐    ┌─────────────┐    ┌─────────────┐ │
│   │ CharClass   │    │ RegexCache  │    │  Visitor    │    │ SourceMap   │ │
│   │  (Tables)   │    │ (Compiled)  │    │  (Walker)   │    │  (Builder)  │ │
│   └─────────────┘    └─────────────┘    └─────────────┘    └─────────────┘ │
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘
```

## Core Components

### 1. Grammar (`grammar.rs`)

The `Grammar` struct represents a parsed grammar as a vector of atoms:

```rust
pub struct Grammar {
    pub atoms: Vec<Atom>,  // All atoms (referenced by index)
    pub root: usize,       // Index of root atom
}

pub enum Atom {
    Str { pattern: String },
    Re { pattern: String },
    Sequence { atoms: Vec<usize> },
    Alternative { atoms: Vec<usize> },
    Repetition { atom: usize, min: usize, max: Option<usize> },
    Named { name: String, atom: usize },
    Entity { atom: usize },
    Lookahead { atom: usize, positive: bool },
    Cut,
    Ignore { atom: usize },
}
```

**Key Features:**
- Atoms reference each other by index (compact, cache-friendly)
- Supports forward references via `Entity` atom
- JSON-serializable for cross-language compatibility
- `AtomVisitor` trait for grammar analysis

### 2. Parser (`parser.rs`)

The `PortableParser` is the main parsing engine:

```rust
pub struct PortableParser<'a> {
    grammar: &'a Grammar,
    input: &'a str,
    arena: &'a mut AstArena,
    cache: DenseCache,
    // ... configuration and state
}
```

**Key Methods:**
- `parse()` - Main entry point
- `parse_with_config()` - Parse with custom configuration
- `try_atom()` - Parse a single atom with memoization

**Parse Context:**
```rust
pub struct ParseContext<'a> {
    pub arena: &'a mut AstArena,
    pub cache: DenseCache,
    pub cached_nodes: Vec<AstNode>,
    // ... resource tracking
}
```

### 3. Cache (`cache.rs`)

Dense packrat cache with open addressing:

```rust
pub struct CacheEntry {
    pub pos: u32,           // Input position
    pub end_pos: u32,       // End position (if success)
    packed_ast_ref: u32,    // Success flag + AST reference (packed)
    pub atom_id: u16,       // Atom index
}
```

**Cache Entry Packing:**
- High bit of `packed_ast_ref` = success flag
- Lower 31 bits = arena node index
- 16 bytes per entry (14 logical + 2 padding)

**Performance:**
- O(1) lookup with FNV-1a hash
- Linear probing for collision resolution
- Load factor 0.75 for optimal performance

### 4. Arena (`arena.rs`)

Arena allocator for AST nodes:

```rust
pub struct AstArena {
    string_data: Vec<u8>,      // String content pool
    string_pool: Vec<StringPoolEntry>,  // String metadata
    string_hash: HashMap<u64, usize>,   // String interning lookup
    array_pool: Vec<ArrayPoolEntry>,    // Array elements
    hash_pool: Vec<HashPoolEntry>,      // Hash key-value pairs
}
```

**Key Features:**
- String interning for memory efficiency
- Zero-copy input references (`InputRef`)
- O(1) reset for reuse
- Configurable string clearing

### 5. AST (`ast.rs`)

AST node types:

```rust
pub enum AstNode {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    StringRef { pool_index: u32 },
    InputRef { offset: u32, length: u32 },
    Array { pool_index: u32, length: u32 },
    Hash { pool_index: u32, length: u32 },
}
```

**Key Features:**
- All variants are Copy (no heap allocation)
- Position tracking via `SourcePosition`
- Rich error type `ParseError` with source location

### 6. Transform (`transform.rs`)

Pattern-based AST transformation:

```rust
pub struct DirectTransform {
    rules: Vec<TransformRule>,
}

pub enum Value {
    Nil,
    Bool(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Array(Vec<Value>),
    Hash(IndexMap<String, Value>),
}
```

### 7. Visitor (`visitor.rs`)

Tree walking visitor pattern:

```rust
pub trait Visitor {
    fn visit_nil(&mut self) -> ControlFlow<()>;
    fn visit_bool(&mut self, value: bool) -> ControlFlow<()>;
    fn visit_int(&mut self, value: i64) -> ControlFlow<()>;
    // ... other variants
}
```

**Built-in Visitors:**
- `NodeCounter` - Count nodes by type
- `StringCollector` - Collect all strings
- `DepthAnalyzer` - Measure tree depth

## Data Flow

### Standard Parsing Flow

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│   Grammar    │     │    Input     │     │   Parser     │     │   AST/Error  │
│    (JSON)    │────▶│   (String)   │────▶│ (Packrat)    │────▶│   (Result)   │
└──────────────┘     └──────────────┘     └──────────────┘     └──────────────┘
                            │                    │
                            │                    ▼
                            │            ┌──────────────┐
                            │            │    Cache     │
                            │            │  (Memoize)   │
                            │            └──────────────┘
                            │                    │
                            ▼                    ▼
                     ┌──────────────┐     ┌──────────────┐
                     │    Arena     │     │  Parse Tree  │
                     │ (Allocator)  │◀────│   (Nodes)    │
                     └──────────────┘     └──────────────┘
```

### Incremental Parsing Flow

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│  Previous    │     │    Edit      │     │   Dirty      │
│  Parse Tree  │────▶│   (Change)   │────▶│   Regions    │
└──────────────┘     └──────────────┘     └──────────────┘
                            │
                            ▼
                     ┌──────────────┐     ┌──────────────┐
                     │  Invalidate  │     │   Reparse    │
                     │    Cache     │────▶│   Changed    │
                     └──────────────┘     └──────────────┘
                                                │
                                                ▼
                                         ┌──────────────┐
                                         │    Merged    │
                                         │  Parse Tree  │
                                         └──────────────┘
```

### Streaming Parsing Flow

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│    Chunk     │     │   Sliding    │     │   Partial    │
│   Source     │────▶│    Window    │────▶│    Result    │
└──────────────┘     └──────────────┘     └──────────────┘
       │                                          │
       │ (repeat)                                 ▼
       │                                   ┌──────────────┐
       └──────────────────────────────────▶│   Callback   │
                                           └──────────────┘
```

## Extension Points

### 1. Custom Atoms

Create custom atom types by implementing pattern matching:

```rust
// In grammar.rs, add new Atom variant
pub enum Atom {
    // ... existing variants
    Custom { id: String, config: CustomConfig },
}

// In parser.rs, add matching logic
fn parse_atom(&mut self, atom: &Atom, pos: usize) -> Result<...> {
    match atom {
        Atom::Custom { id, config } => self.parse_custom(id, config, pos),
        // ... other variants
    }
}
```

### 2. Custom Visitors

Implement the `Visitor` trait:

```rust
struct MyVisitor;

impl Visitor for MyVisitor {
    fn visit_hash(&mut self, pool: &[HashPoolEntry], start: usize, len: usize) -> ControlFlow<()> {
        // Custom hash processing
        ControlFlow::Continue(())
    }
}

// Usage
walk(&arena, &root_node, &mut MyVisitor);
```

### 3. Custom Transforms

Add transformation rules:

```rust
let mut transform = DirectTransform::new();
transform.add_rule("number", |node, arena| {
    // Custom number transformation
    Ok(Value::Integer(extract_int(node, arena)?))
});
```

### 4. Grammar Analysis

Implement `AtomVisitor` for grammar analysis:

```rust
struct RecursionDetector {
    visited: HashSet<usize>,
    has_recursion: bool,
}

impl AtomVisitor for RecursionDetector {
    fn visit_entity(&mut self, atom: usize) {
        if self.visited.contains(&atom) {
            self.has_recursion = true;
        }
    }
}
```

## Performance Characteristics

| Operation | Time Complexity | Space Complexity |
|-----------|-----------------|------------------|
| Parse | O(n) | O(n × m) |
| Cache lookup | O(1) amortized | - |
| Arena allocation | O(1) | - |
| String interning | O(1) amortized | - |
| Reset arena | O(1) | - |
| Tree walk | O(n) | O(depth) |

Where:
- n = input length
- m = grammar size (atom count)

## Memory Layout

### Cache Entry (16 bytes)
```
┌────────────────────────────────────────────────────────┐
│ pos (4) │ end_pos (4) │ packed_ast_ref (4) │ atom_id (2) + padding (2) │
└────────────────────────────────────────────────────────┘
```

### AST Node (8 bytes)
```
┌─────────────────────────────────┐
│ tag (1) │ payload (7)           │
└─────────────────────────────────┘
```

### Arena Memory Pools
```
┌─────────────────────────────────────────────────────────┐
│ string_data │ string_pool │ array_pool │ hash_pool      │
│ (bytes)     │ (entries)   │ (entries)  │ (entries)      │
└─────────────────────────────────────────────────────────┘
```

## Error Handling

### Error Types

```rust
pub enum ParseError {
    Failed { position: usize },
    Incomplete { position: usize },
    InvalidGrammar { message: String },
    InputTooLarge { size: usize, max: usize },
    RecursionLimitExceeded { depth: usize, max: usize },
    MemoryLimitExceeded { used: usize, max: usize },
    TimeoutExceeded { elapsed_ms: u64, max_ms: u64 },
    Internal { message: String },
}
```

### Rich Error Reporting

```rust
pub struct RichError {
    pub message: String,
    pub span: SourceSpan,
    pub context: Option<String>,
    pub children: Vec<RichError>,
    pub severity: ErrorSeverity,
}

impl RichError {
    pub fn deepest(&self) -> &RichError;
    pub fn leaves(&self) -> Vec<&RichError>;
    pub fn to_tree(&self) -> String;
    pub fn format_with_source(&self, source: &str) -> String;
}
```

## Configuration

### ParserConfig

```rust
let config = ParserConfig::builder()
    .max_input_size(10_000_000)      // 10 MB max
    .max_recursion_depth(1000)        // Recursion limit
    .timeout_ms(5000)                 // 5 second timeout
    .max_memory(100_000_000)          // 100 MB memory limit
    .build();

let result = parser.parse_with_config(input, config);
```

### Arena Options

```rust
// Create arena sized for input
let mut arena = AstArena::for_input(input.len());

// Reset for reuse (preserve strings)
arena.reset();

// Reset and clear strings (for long-running processes)
arena.reset_with_options(true);
```

## Testing Strategy

1. **Unit Tests** - Each component tested in isolation
2. **Integration Tests** - Full parsing pipelines
3. **Property Tests** - Invariants over random inputs
4. **Benchmark Tests** - Performance regression detection

## Future Improvements

1. **SIMD Optimization** - Vectorized character class matching
2. **Parallel Parsing** - Independent subtrees in parallel
3. **Incremental Compilation** - Compile grammar to native code
4. **WASM Support** - Browser/Node.js compatibility
