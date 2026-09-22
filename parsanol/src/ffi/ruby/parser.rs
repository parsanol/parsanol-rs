//! Parser functions for Ruby FFI
//!
//! # Public API
//!
//! ## High-Level API (Recommended)
//!
//! ```ruby
//! result = Parsanol::Native.parse(grammar, input)
//! # Returns Parslet-compatible AST with lazy line/column support
//! ```
//!
//! ## Raw API (For Custom Transformation)
//!
//! ```ruby
//! result = Parsanol::Native.parse_raw(grammar, input)
//! # Returns raw intermediate format (no transformation)
//! ```
//!
//! # Architecture
//!
//! The parsing pipeline consists of:
//! 1. **Rust parsing** - Fast parsing with packrat memoization
//! 2. **Rust transformation** - `to_parslet_compatible` produces Parslet-compatible AST
//! 3. **Batch encoding** - Flat u64 array for efficient FFI transfer
//! 4. **Ruby decoding** - BatchDecoder produces Ruby Hash/Array/Slice objects
//!
//! # Slice Objects
//!
//! Slice objects support lazy line/column computation:
//!
//! ```ruby
//! slice = result[:name]
//! slice.offset            # => 42 (always available)
//! slice.content           # => "hello" (always available)
//! slice.line_and_column   # => [5, 1] (computed lazily on first call)
//! ```
//!
//! # Batch Format
//!
//! The batch format uses tagged u64 values for efficient FFI:
//!
//! | Tag | Value | Description |
//! |-----|-------|-------------|
//! | 0x00 | - | nil |
//! | 0x01 | 0 or 1 | bool |
//! | 0x02 | value | int |
//! | 0x03 | IEEE bits | float |
//! | 0x04 | offset, length | Slice reference |
//! | 0x05-0x06 | ... | array |
//! | 0x07-0x08 | ... | hash |
//! | 0x09 | len, data... | hash key |
//! | 0x0A | len, data... | inline string |
//! | 0x0B | len, data... | symbol |
//! | 0x0C | items... | repetition marker |
//! | 0x0D | items... | sequence marker |

use crate::ffi::ruby::cache::LruCache;
use crate::ffi::shared::flatten_ast_to_u64;
use crate::portable::bytecode::{parse_with_vm_capped, Program, VmCappedOutcome};
use crate::portable::{
    to_parslet_compatible, AstArena, Atom, DenseCache, Grammar, ParseError, PortableParser,
};
use magnus::{value::ReprValue, Error, RString, Ruby, Value};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::builder::RubyBuilder;
use super::transform::transform_ast;

/// Default maximum number of grammars to cache.
/// This prevents unbounded memory growth during batch processing.
const DEFAULT_GRAMMAR_CACHE_SIZE: usize = 100;

type GrammarCache = LruCache<u64, Grammar>;

/// Thread-safe global grammar cache with bounded LRU eviction
static GRAMMAR_CACHE: std::sync::OnceLock<Mutex<GrammarCache>> = std::sync::OnceLock::new();

/// Programs compiled for grammars in the LRU cache, keyed by the same
/// structure hash, so one-shot paths (parse_fresh) skip recompilation.
/// Bounded LRU: capture-derived dynamic fragments mint a fresh structure
/// hash on every parse (parsanol-ruby#93), so an unbounded map retained
/// multi-megabyte programs across parses and ballooned RSS to 2+ GB.
static PROGRAM_CACHE: std::sync::OnceLock<Mutex<LruCache<u64, Arc<Program>>>> =
    std::sync::OnceLock::new();

/// Programs are large (EXPRESS compiles to ~10k instructions); keep the
/// resident set small while still covering a working set of fragments.
const DEFAULT_PROGRAM_CACHE_SIZE: usize = 32;

fn get_program_cache() -> &'static Mutex<LruCache<u64, Arc<Program>>> {
    PROGRAM_CACHE.get_or_init(|| Mutex::new(LruCache::new(DEFAULT_PROGRAM_CACHE_SIZE)))
}

/// Grammars whose one-shot (parse_fresh) parses tripped the VM budget.
/// Cleared when it grows past the cap; entries are one u64 each.
static VM_STICKY_OFF: std::sync::OnceLock<Mutex<std::collections::HashSet<u64>>> =
    std::sync::OnceLock::new();

const VM_STICKY_OFF_CAP: usize = 4096;

fn vm_sticky_off(hash: u64) -> bool {
    VM_STICKY_OFF
        .get_or_init(|| Mutex::new(std::collections::HashSet::new()))
        .lock()
        .unwrap()
        .contains(&hash)
}

fn mark_vm_sticky_off(hash: u64) {
    let mut set = VM_STICKY_OFF
        .get_or_init(|| Mutex::new(std::collections::HashSet::new()))
        .lock()
        .unwrap();
    if set.len() >= VM_STICKY_OFF_CAP {
        set.clear();
    }
    set.insert(hash);
}

fn cached_program(hash: u64, grammar: &Grammar) -> Option<Arc<Program>> {
    {
        let guard = get_program_cache().lock().unwrap();
        if let Some(program) = guard.get(&hash) {
            return Some(program.clone());
        }
    }
    let compiled = compile_program_on_big_stack(grammar).map(Arc::new);
    if let Some(program) = &compiled {
        get_program_cache()
            .lock()
            .unwrap()
            .insert(hash, program.clone());
    }
    compiled
}

/// Compilation recurses over the grammar's atom tree, and a deep grammar
/// (the EXPRESS grammar is ~2,273 atoms) can exceed the Ruby thread's
/// stack guard mid-call. Compiling on a dedicated thread with a large
/// stack keeps registration safe; the program itself is plain data.
fn compile_program_on_big_stack(grammar: &Grammar) -> Option<Program> {
    let grammar = grammar.clone();
    match std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || crate::portable::bytecode::compile_bytecode(grammar).ok())
    {
        Ok(handle) => handle.join().ok().flatten(),
        Err(_) => None,
    }
}

/// Grammar registered by explicit handle, avoiding the per-call JSON
/// marshal + hash of the LRU path. `has_dynamic` gates the zero-copy
/// input borrow: a Dynamic atom calls back into Ruby during the parse,
/// and we must not hold a borrowed `&str` across that.
#[derive(Clone)]
struct HandleEntry {
    grammar: Arc<Grammar>,
    has_dynamic: bool,
    /// Program compiled once at registration for the bytecode VM tier.
    /// None when the grammar uses atoms the VM cannot express (e.g.
    /// Dynamic) — those grammars stay on the packrat engine entirely.
    program: Option<Arc<Program>>,
    /// Sticky VM disable: set once a parse tripped the backtrack budget,
    /// so backtracking-heavy grammars stop paying the retry cost.
    vm_disabled: Arc<std::sync::atomic::AtomicBool>,
}

/// Inputs at or above this size parse on the bytecode VM when the
/// grammar compiled; below it the packrat tree-walker wins on setup
/// cost (2 KB: VM -16%; 64 KB: VM +176% — TODO.max-perf/4 phase 1).
const VM_INPUT_THRESHOLD: usize = 8 * 1024;

/// Backtrack budget for a VM parse. Measured rates: linear grammars
/// backtrack ~0.0001/byte (the KV bench grammar: 2 backtracks for
/// 47.5 KB), while the EXPRESS grammar runs ~16/byte and the memo-less
/// VM loses to packrat there. The budget sits six orders of magnitude
/// above the linear class and trips the heavy class within ~0.1% of
/// its work, falling back to the walker for that parse and sticking
/// the handle off.
#[inline]
fn vm_backtrack_budget(input_len: usize) -> u64 {
    input_len as u64 / 64 + 16
}

static HANDLE_MAP: std::sync::OnceLock<Mutex<HashMap<u64, HandleEntry>>> =
    std::sync::OnceLock::new();
static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);

fn get_handle_map() -> &'static Mutex<HashMap<u64, HandleEntry>> {
    HANDLE_MAP.get_or_init(|| Mutex::new(HashMap::new()))
}

fn get_grammar_cache() -> &'static Mutex<GrammarCache> {
    GRAMMAR_CACHE.get_or_init(|| Mutex::new(GrammarCache::new(DEFAULT_GRAMMAR_CACHE_SIZE)))
}

/// Clear the grammar cache, freeing all cached grammars.
///
/// This is useful for batch processing scenarios where you want to
/// limit memory usage by clearing unused grammars.
///
/// # Example
///
/// ```ruby
/// Parsanol::Native.clear_grammar_cache
/// ```
pub fn clear_grammar_cache() {
    if let Some(cache) = GRAMMAR_CACHE.get() {
        if let Ok(mut guard) = cache.lock() {
            guard.clear();
        }
    }
    if let Some(map) = HANDLE_MAP.get() {
        if let Ok(mut guard) = map.lock() {
            guard.clear();
        }
    }
}

/// Get the current number of cached grammars.
pub fn grammar_cache_size() -> usize {
    if let Some(cache) = GRAMMAR_CACHE.get() {
        if let Ok(guard) = cache.lock() {
            return guard.len();
        }
    }
    0
}

/// Get the grammar cache capacity.
pub fn grammar_cache_capacity() -> usize {
    if let Some(cache) = GRAMMAR_CACHE.get() {
        if let Ok(guard) = cache.lock() {
            return guard.capacity();
        }
    }
    DEFAULT_GRAMMAR_CACHE_SIZE
}

fn hash_string(s: &str) -> u64 {
    let mut hasher = ahash::AHasher::default();
    s.hash(&mut hasher);
    hasher.finish()
}

/// Deserialize grammar from JSON, applying optimizations.
/// The grammar is cached by hash after optimization.
fn load_grammar(grammar_json: &str) -> Result<Grammar, serde_json::Error> {
    Grammar::from_json(grammar_json)
}

/// Check if native extension is available
pub fn is_available() -> bool {
    true
}

// ============================================================================
// Handle-based API
//
// The JSON-string API pays a marshal + hash of the grammar JSON on every
// call. Registering once and passing a small integer handle removes that
// per-call cost entirely, and lets `parse_handle` borrow the input string
// instead of copying it.
// ============================================================================

/// Register a grammar JSON and return a handle for `parse_handle`.
///
/// # Example
///
/// ```ruby
/// handle = Parsanol::Native._register_grammar(grammar_json)
/// result = Parsanol::Native._parse_handle(handle, input)
/// ```
pub fn register_grammar(grammar_json: String) -> Result<u64, Error> {
    let ruby = Ruby::get().unwrap();
    let grammar: Grammar = load_grammar(&grammar_json)
        .map_err(|e| Error::new(ruby.exception_arg_error(), e.to_string()))?;
    let has_dynamic = grammar
        .atoms
        .iter()
        .any(|a| matches!(a, Atom::Dynamic { .. }));
    // Precompile the VM program once; grammars the VM cannot express
    // (compile error, currently Dynamic/Custom) keep the packrat engine.
    // The compiled-program artifact cache (TODO.perf/1) short-circuits
    // cold-start compiles for grammars seen by any earlier process.
    let artifact_key = crate::portable::bytecode::artifact_cache::grammar_key(&grammar_json);
    let program = crate::portable::bytecode::artifact_cache::load(artifact_key)
        .map(Arc::new)
        .or_else(|| {
            let compiled = compile_program_on_big_stack(&grammar)?;
            crate::portable::bytecode::artifact_cache::store(artifact_key, &compiled);
            Some(Arc::new(compiled))
        });
    let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
    get_handle_map().lock().unwrap().insert(
        handle,
        HandleEntry {
            grammar: Arc::new(grammar),
            has_dynamic,
            program,
            vm_disabled: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        },
    );
    Ok(handle)
}

/// Release a grammar previously registered with `register_grammar`.
/// Returns true when a handle was removed.
pub fn release_grammar(handle: u64) -> bool {
    get_handle_map().lock().unwrap().remove(&handle).is_some()
}

/// Parse with a registered grammar handle, borrowing the input string
/// (zero copy) when the grammar has no Dynamic atoms.
pub fn parse_handle(handle: u64, input: RString) -> Result<Value, Error> {
    let ruby = Ruby::get().unwrap();
    let entry = get_handle_map().lock().unwrap().get(&handle).cloned();
    let Some(entry) = entry else {
        return Err(Error::new(
            ruby.exception_arg_error(),
            format!("unknown grammar handle: {}", handle),
        ));
    };

    // SAFETY: the borrowed &str is used for the duration of this call only.
    // Nothing mutates the input string while we hold the reference: Ruby
    // code only re-enters via Dynamic atoms (excluded by `has_dynamic`);
    // object allocation during transform can run the GC but cannot move or
    // mutate a string (compaction requires an explicit GC.compact that
    // user code cannot run inside this call).
    let input_str: &str = unsafe { input.as_str()? };

    let program = entry
        .program
        .as_deref()
        .filter(|_| !entry.vm_disabled.load(Ordering::Relaxed));
    if entry.has_dynamic {
        let owned = input_str.to_string();
        parse_with_grammar(&ruby, &entry.grammar, program, &owned, &entry.vm_disabled)
    } else {
        parse_with_grammar(
            &ruby,
            &entry.grammar,
            program,
            input_str,
            &entry.vm_disabled,
        )
    }
}

/// Parse with a registered grammar handle and return the parslet-shaped
/// AST as a flat event stream: [[events...], [strings...]].
///
/// Single FFI return, no per-node Ruby objects — the opcode stream
/// mirrors `parse_native`'s output exactly (see `portable::events` for
/// the opcode table); consumers replay the stream or attach domain
/// handling directly to the opcodes.
pub fn parse_handle_events(handle: u64, input: RString) -> Result<Value, Error> {
    let ruby = Ruby::get().unwrap();
    let entry = get_handle_map().lock().unwrap().get(&handle).cloned();
    let Some(entry) = entry else {
        return Err(Error::new(
            ruby.exception_arg_error(),
            format!("unknown grammar handle: {}", handle),
        ));
    };

    // SAFETY: same borrow discipline as parse_handle.
    let input_str: &str = unsafe { input.as_str()? };

    let mut arena = AstArena::for_input(input_str.len());
    arena.set_input(input_str.to_string());
    let mut parser = PortableParser::new(&entry.grammar, input_str, &mut arena);
    let raw = parser.parse().map_err(|e| {
        let diagnostics = parser.failure_diagnostics();
        Error::new(
            ruby.exception_runtime_error(),
            native_failure_message(&e, diagnostics),
        )
    })?;
    let shaped = to_parslet_compatible(&raw, &mut arena, input_str);
    let (events, strings) = crate::portable::events::linearize_events(&shaped, &mut arena);

    // Pack the opcode stream into one binary string: a single Ruby
    // allocation and one unpack("q*") on the Ruby side, instead of one
    // Integer object per event.
    let bytes: Vec<u8> = events.iter().flat_map(|e| e.to_le_bytes()).collect();
    let events_str = ruby.str_from_slice(&bytes);
    let strings_ary = ruby.ary_new_capa(strings.len());
    for s in strings {
        strings_ary.push(s.as_str())?;
    }
    let pair = ruby.ary_new_capa(2);
    pair.push(events_str)?;
    pair.push(strings_ary)?;
    Ok(pair.as_value())
}

/// Parse with a registered grammar handle WITHOUT requiring full-input
/// consumption (the Ruby engine's `prefix: true` mode). Returns
/// [value, end_pos]; unmatched trailing input is simply left over.
pub fn parse_handle_prefix(handle: u64, input: RString) -> Result<Value, Error> {
    let ruby = Ruby::get().unwrap();
    let entry = get_handle_map().lock().unwrap().get(&handle).cloned();
    let Some(entry) = entry else {
        return Err(Error::new(
            ruby.exception_arg_error(),
            format!("unknown grammar handle: {}", handle),
        ));
    };

    // SAFETY: same borrow discipline as parse_handle.
    let input_str: &str = unsafe { input.as_str()? };

    let program = entry
        .program
        .as_deref()
        .filter(|_| !entry.vm_disabled.load(Ordering::Relaxed));
    if let Some(program) = program {
        if input_str.len() >= VM_INPUT_THRESHOLD {
            let mut arena = AstArena::for_input(input_str.len());
            arena.set_input(input_str.to_string());
            let outcome = parse_with_vm_capped(
                program,
                input_str,
                &mut arena,
                vm_backtrack_budget(input_str.len()),
            );
            if let VmCappedOutcome::Parsed {
                result,
                diagnostics,
            } = outcome
            {
                let result = match result {
                    Ok(result) => result,
                    Err(e) => {
                        return Err(Error::new(
                            ruby.exception_runtime_error(),
                            native_failure_message(&e, diagnostics),
                        ));
                    }
                };
                let collapsed = crate::ffi::shared::collapse_ast(&result.value, &mut arena);
                let value = transform_ast(&collapsed, &arena, input_str, &ruby)?;
                let pair = ruby.ary_new_capa(2);
                pair.push(value)?;
                pair.push(result.end_pos as i64)?;
                return Ok(pair.as_value());
            }
            entry.vm_disabled.store(true, Ordering::Relaxed);
        }
    }

    let mut arena = AstArena::for_input(input_str.len());
    arena.set_input(input_str.to_string());
    let mut parser = PortableParser::new(&entry.grammar, input_str, &mut arena);
    let result = match parser.parse_with_end_pos() {
        Ok(result) => result,
        Err(e) => {
            let diagnostics = parser.failure_diagnostics();
            return Err(Error::new(
                ruby.exception_runtime_error(),
                native_failure_message(&e, diagnostics),
            ));
        }
    };

    let collapsed = crate::ffi::shared::collapse_ast(&result.value, &mut arena);
    let value = transform_ast(&collapsed, &arena, input_str, &ruby)?;
    let pair = ruby.ary_new_capa(2);
    pair.push(value)?;
    pair.push(result.end_pos as i64)?;
    Ok(pair.as_value())
}

/// Format a native parse failure for the Ruby tier: a parslet-style
/// expected-set message plus a machine-readable position marker the
/// Ruby side strips before raising Parsanol::ParseFailed.
fn native_failure_message(e: &ParseError, diagnostics: Option<(usize, Vec<String>)>) -> String {
    if let ParseError::Failed { position } = e {
        let (position, expected) = match diagnostics {
            Some((pos, labels)) => (pos, labels),
            None => (*position, Vec::new()),
        };
        let what = if expected.is_empty() {
            "no further input".to_string()
        } else {
            format!("one of [{}]", expected.join(", "))
        };
        // Position suffix comes from Cause#to_s on the Ruby side; the
        // marker line carries the byte offset for cause construction.
        return format!(
            "Failed to match: expected {}\n@@parsanol_pos:{}",
            what, position
        );
    }
    e.to_string()
}

fn parse_with_grammar(
    ruby: &Ruby,
    grammar: &Grammar,
    program: Option<&Program>,
    input: &str,
    vm_disabled: &std::sync::atomic::AtomicBool,
) -> Result<Value, Error> {
    // Large inputs run on the precompiled bytecode VM (TODO.max-perf/4
    // phase 2): byte-identical trees per the differential gate, 2.76x
    // on 64 KB inputs.
    if let Some(program) = program {
        if input.len() >= VM_INPUT_THRESHOLD {
            let mut arena = AstArena::for_input(input.len());
            arena.set_input(input.to_string());
            let outcome =
                parse_with_vm_capped(program, input, &mut arena, vm_backtrack_budget(input.len()));
            match outcome {
                VmCappedOutcome::BudgetExceeded => {
                    if std::env::var("PARSANOL_VM_DEBUG").is_ok() {
                        eprintln!(
                            "parsanol: VM backtrack budget tripped ({} bytes); sticking handle to packrat",
                            input.len()
                        );
                    }
                    vm_disabled.store(true, Ordering::Relaxed);
                    // fall through to the walker with a FRESH arena
                }
                VmCappedOutcome::Parsed {
                    result,
                    diagnostics,
                } => {
                    let result = match result {
                        Ok(result) => result,
                        Err(e) => {
                            return Err(Error::new(
                                ruby.exception_runtime_error(),
                                native_failure_message(&e, diagnostics),
                            ));
                        }
                    };
                    let collapsed = crate::ffi::shared::collapse_ast(&result.value, &mut arena);
                    return transform_ast(&collapsed, &arena, input, ruby);
                }
            }
        }
    }

    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(grammar, input, &mut arena);

    let ast = match parser.parse() {
        Ok(ast) => ast,
        Err(e) => {
            let diagnostics = parser.failure_diagnostics();
            return Err(Error::new(
                ruby.exception_runtime_error(),
                native_failure_message(&e, diagnostics),
            ));
        }
    };

    // Collapse adjacent input refs in the arena first: the join happens with
    // zero Ruby object churn, so the flat encoding stays small.
    let collapsed = crate::ffi::shared::collapse_ast(&ast, &mut arena);

    // Build Ruby objects directly. A batch-encoding detour (u64 cells
    // boxed into Integers, then re-walked by the Ruby decoder) measured
    // 1.7x SLOWER than parslet on a real ~90-rule compat grammar
    // (issue parsanol-ruby#25) versus ~3x faster with this direct
    // build; the batch path stays in the C-ABI tier where it is the
    // only option. The transformer heuristics on both paths are kept
    // in agreement by the shared differential corpus.
    transform_ast(&collapsed, &arena, input, ruby)
}

// ============================================================================
// INCREMENTAL SESSIONS (TODO.perf/4)
// ============================================================================

struct IncrementalSession {
    parser: crate::portable::incremental::IncrementalParser<'static>,
}

fn get_sessions() -> &'static std::sync::Mutex<std::collections::HashMap<u64, IncrementalSession>> {
    static SESSIONS: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<u64, IncrementalSession>>,
    > = std::sync::OnceLock::new();
    SESSIONS.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

static NEXT_SESSION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Create an incremental parsing session for a grammar. Returns the
/// session handle used by `incremental_parse` / `incremental_release`.
/// The first `incremental_parse` call is a full parse; subsequent
/// calls pass the edit span (offset, old_length, new_length) and
/// re-parse only what the edit invalidated.
pub fn incremental_session(grammar_json: String) -> Result<u64, Error> {
    let ruby = Ruby::get().unwrap();
    let grammar: Grammar = load_grammar(&grammar_json)
        .map_err(|e| Error::new(ruby.exception_arg_error(), e.to_string()))?;
    let handle = NEXT_SESSION.fetch_add(1, Ordering::Relaxed);
    get_sessions().lock().unwrap().insert(
        handle,
        IncrementalSession {
            parser: crate::portable::incremental::IncrementalParser::owned(grammar),
        },
    );
    Ok(handle)
}

/// Release an incremental session.
pub fn incremental_release(handle: u64) -> bool {
    get_sessions().lock().unwrap().remove(&handle).is_some()
}

/// Parse within an incremental session. A negative `edit_offset` runs
/// a full (re)parse; otherwise (offset, old_length, new_length)
/// describes the edit that produced this input since the previous
/// call. Returns the parslet-shaped tree, identical to a full parse
/// per the differential gate. Dynamic grammars are eligible: the
/// walker's dynamic-dependent memo filter keeps retention sound, and
/// the input is copied per call so host re-entry is safe.
pub fn incremental_parse(
    handle: u64,
    input: RString,
    edit_offset: i64,
    old_length: i64,
    new_length: i64,
) -> Result<Value, Error> {
    let ruby = Ruby::get().unwrap();
    let session = get_sessions().lock().unwrap().remove(&handle);
    let Some(mut session) = session else {
        return Err(Error::new(
            ruby.exception_arg_error(),
            format!("unknown incremental session: {}", handle),
        ));
    };

    // Own the input: dynamic grammars ARE eligible for sessions
    // (parsanol-ruby#80) and their blocks re-enter Ruby, so the
    // borrow-zero-copy discipline of parse_handle does not apply
    // here. One copy per call is the price of that eligibility.
    let input_str: String = input.to_string()?;
    let input_str: &str = &input_str;

    let mut arena = AstArena::for_input(input_str.len());
    let outcome = if edit_offset < 0 {
        session.parser.parse(input_str, &mut arena)
    } else {
        let edit = crate::portable::incremental::Edit::replace(
            edit_offset.max(0) as usize,
            old_length.max(0) as usize,
            new_length.max(0) as usize,
        );
        session
            .parser
            .parse_with_edit(input_str, &mut arena, edit)
            .map(|r| r.ast)
    };
    // Return the session (its cache) regardless of the parse outcome.
    get_sessions().lock().unwrap().insert(
        handle,
        IncrementalSession {
            parser: session.parser,
        },
    );

    let ast = outcome.map_err(|e| Error::new(ruby.exception_runtime_error(), format!("{}", e)))?;
    let collapsed = crate::ffi::shared::collapse_ast(&ast, &mut arena);
    transform_ast(&collapsed, &arena, input_str, &ruby)
}

/// Cache statistics of an incremental session: [hits, misses].
pub fn incremental_stats(handle: u64) -> Result<(i64, i64), Error> {
    let sessions = get_sessions().lock().unwrap();
    let Some(session) = sessions.get(&handle) else {
        let ruby = Ruby::get().unwrap();
        return Err(Error::new(
            ruby.exception_arg_error(),
            format!("unknown incremental session: {}", handle),
        ));
    };
    let (hits, misses, _) = session.parser.cache_stats();
    Ok((hits as i64, misses as i64))
}

// ============================================================================
// HIGH-LEVEL API
// ============================================================================

/// Parse input and return transformed AST with lazy line/column support
///
/// This is the MAIN parsing method that all users should use.
/// It returns a clean AST matching Ruby parser output:
/// - Symbol keys instead of string keys
/// - Merged sequences (unnamed strings discarded when named captures present)
/// - Proper repetition handling (arrays of named captures, joined strings)
/// - Slice objects with lazy line/column computation
///
/// # Performance
///
/// Provides up to 26x speedup over pure Ruby parsing.
/// Line/column is computed lazily only when Slice#line_and_column is called.
///
/// # Arguments
///
/// * `grammar_json` - JSON string containing the grammar definition
/// * `input` - Input string to parse
///
/// # Returns
///
/// Ruby Hash/Array with transformed AST structure. String values are
/// Parsanol::Slice objects that support lazy line/column computation.
///
/// # Example
///
/// ```ruby
/// grammar_json = Parsanol::Native.serialize_grammar(my_atom)
/// result = Parsanol::Native.parse(grammar_json, "hello")
/// # => {name: "hello"@0}
///
/// # Line/column computed lazily on demand
/// slice = result[:name]
/// slice.line_and_column  # => [1, 1]
/// ```
/// Parse with a grammar JSON string (LRU-cached by content hash).
pub fn parse(grammar_json: String, input: String) -> Result<Value, Error> {
    let ruby = Ruby::get().unwrap();

    // Get or compile grammar (thread-safe with LRU caching)
    let hash = hash_string(&grammar_json);
    let grammar = {
        let cache = get_grammar_cache();
        let mut guard = cache.lock().unwrap();
        if let Some(cached) = guard.get(&hash) {
            cached.clone()
        } else {
            drop(guard);
            let grammar: Grammar = load_grammar(&grammar_json)
                .map_err(|e| Error::new(ruby.exception_arg_error(), e.to_string()))?;
            let mut guard = get_grammar_cache().lock().unwrap();
            // Re-check in case another thread added it while we were parsing
            if let Some(cached) = guard.get(&hash) {
                cached.clone()
            } else {
                guard.insert(hash, grammar.clone());
                grammar
            }
        }
    };

    // One-shot parses have no registered program; the tree-walker path
    // serves them directly.
    static NO_PROGRAM_OFF: std::sync::atomic::AtomicBool =
        std::sync::atomic::AtomicBool::new(false);
    parse_with_grammar(&ruby, &grammar, None, &input, &NO_PROGRAM_OFF)
}

/// Parse without packrat caching for memory-bounded operation
///
/// This creates a fresh arena and an empty cache (no memoization).
/// Memory usage is bounded by AST size rather than input × atoms.
/// Use for large files where memory is more important than speed.
///
/// Returns the same format as `parse()`.
pub fn parse_fresh(grammar_json: String, input: String) -> Result<Value, Error> {
    let ruby = Ruby::get().unwrap();

    // Get or compile grammar (thread-safe with LRU caching)
    let hash = hash_string(&grammar_json);
    let grammar = {
        let cache = get_grammar_cache();
        let mut guard = cache.lock().unwrap();
        if let Some(cached) = guard.get(&hash) {
            cached.clone()
        } else {
            drop(guard);
            let grammar: Grammar = load_grammar(&grammar_json)
                .map_err(|e| Error::new(ruby.exception_arg_error(), e.to_string()))?;
            let mut guard = get_grammar_cache().lock().unwrap();
            if let Some(cached) = guard.get(&hash) {
                cached.clone()
            } else {
                guard.insert(hash, grammar.clone());
                grammar
            }
        }
    };

    // The VM carries no memo table, so it is inherently the
    // memory-bounded engine; prefer it whenever the grammar compiled
    // and the input clears the size threshold.
    if input.len() >= VM_INPUT_THRESHOLD && !vm_sticky_off(hash) {
        if let Some(program) = cached_program(hash, &grammar) {
            let mut arena = AstArena::for_input(input.len());
            arena.set_input(input.clone());
            let outcome = parse_with_vm_capped(
                &program,
                &input,
                &mut arena,
                vm_backtrack_budget(input.len()),
            );
            if let VmCappedOutcome::Parsed {
                result,
                diagnostics,
            } = outcome
            {
                let result = match result {
                    Ok(result) => result,
                    Err(e) => {
                        return Err(Error::new(
                            ruby.exception_runtime_error(),
                            native_failure_message(&e, diagnostics),
                        ));
                    }
                };
                let collapsed = crate::ffi::shared::collapse_ast(&result.value, &mut arena);
                return transform_ast(&collapsed, &arena, &input, &ruby);
            }
            mark_vm_sticky_off(hash);
        }
    }

    // Create fresh arena (no reuse, no memory accumulation)
    let mut arena = AstArena::for_input(input.len());
    // Create empty cache — every lookup misses, no memoization
    let cache = DenseCache::new(0);
    let mut parser = PortableParser::new_with_cache(&grammar, &input, &mut arena, cache);

    let ast = parser
        .parse()
        .map_err(|e| Error::new(ruby.exception_runtime_error(), e.to_string()))?;

    // Transform AST to Ruby format
    transform_ast(&ast, &arena, &input, &ruby)
}

// ============================================================================
// LOW-LEVEL API - For advanced users / debugging
// ============================================================================

/// Parse with cache statistics - returns [ast, cache_hits, cache_misses, hit_rate]
///
/// This is a low-level function for performance debugging.
/// Most users should use `parse()` instead.
pub fn parse_with_stats(
    grammar_json: String,
    input: String,
) -> Result<(Value, u64, u64, f64), Error> {
    let ruby = Ruby::get().unwrap();

    // Get or compile grammar (thread-safe with LRU caching)
    let hash = hash_string(&grammar_json);
    let grammar = {
        let cache = get_grammar_cache();
        let mut guard = cache.lock().unwrap();
        if let Some(cached) = guard.get(&hash) {
            cached.clone()
        } else {
            drop(guard);
            let grammar: Grammar = load_grammar(&grammar_json)
                .map_err(|e| Error::new(ruby.exception_arg_error(), e.to_string()))?;
            let mut guard = get_grammar_cache().lock().unwrap();
            // Re-check in case another thread added it while we were parsing
            if let Some(cached) = guard.get(&hash) {
                cached.clone()
            } else {
                guard.insert(hash, grammar.clone());
                grammar
            }
        }
    };

    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, &input, &mut arena);

    let ast = parser
        .parse()
        .map_err(|e| Error::new(ruby.exception_runtime_error(), e.to_string()))?;

    // Get cache statistics before parser is consumed
    let cache = parser.into_cache();
    let (hits, misses, hit_rate) = cache.stats();

    // Transform AST to Ruby format
    let result = transform_ast(&ast, &arena, &input, &ruby)?;

    Ok((result, hits, misses, hit_rate))
}

/// Parse using batch FFI - returns flat array WITH transformation
///
/// This is the RECOMMENDED batch parsing function. It transforms the AST
/// to Parslet-compatible format BEFORE flattening, so Ruby can decode
/// directly without additional transformation.
///
/// Returns a flat u64 array where the AST is already transformed:
/// - Sequences merged (unnamed discarded when named captures present)
/// - Repetitions properly handled (arrays of named captures)
/// - Consecutive slices joined
pub fn parse_batch(grammar_json: String, input: String) -> Result<Vec<u64>, Error> {
    let ruby = Ruby::get().unwrap();

    // Get or compile grammar (thread-safe with LRU caching)
    let hash = hash_string(&grammar_json);
    let grammar = {
        let cache = get_grammar_cache();
        let mut guard = cache.lock().unwrap();
        if let Some(cached) = guard.get(&hash) {
            cached.clone()
        } else {
            drop(guard);
            let grammar: Grammar = load_grammar(&grammar_json)
                .map_err(|e| Error::new(ruby.exception_arg_error(), e.to_string()))?;
            let mut guard = get_grammar_cache().lock().unwrap();
            // Re-check in case another thread added it while we were parsing
            if let Some(cached) = guard.get(&hash) {
                cached.clone()
            } else {
                guard.insert(hash, grammar.clone());
                grammar
            }
        }
    };

    let mut arena = AstArena::for_input(input.len());
    arena.set_input(input.clone());
    let mut parser = PortableParser::new(&grammar, &input, &mut arena);

    // 1. Parse
    let ast = parser
        .parse()
        .map_err(|e| Error::new(ruby.exception_runtime_error(), e.to_string()))?;

    // 2. Transform to Parslet-compatible format
    // This is REQUIRED for Expressir use case - without it, the AST is in
    // raw intermediate format that Builder cannot process.
    let transformed = to_parslet_compatible(&ast, &mut arena, &input);

    // 3. Flatten transformed AST to u64 array
    let mut result = Vec::new();
    flatten_ast_to_u64(&transformed, &arena, &input, &mut result);
    Ok(result)
}

/// Parse with a Ruby builder callback
///
/// This is an advanced function for streaming parsing.
/// Most users should use `parse()` instead.
pub fn parse_with_builder(
    grammar_json: String,
    input: String,
    builder: Value,
) -> Result<Value, Error> {
    let ruby = Ruby::get().unwrap();

    // Get or compile grammar (thread-safe with LRU caching)
    let hash = hash_string(&grammar_json);
    let grammar = {
        let cache = get_grammar_cache();
        let mut guard = cache.lock().unwrap();
        if let Some(cached) = guard.get(&hash) {
            cached.clone()
        } else {
            drop(guard);
            let grammar: Grammar = load_grammar(&grammar_json)
                .map_err(|e| Error::new(ruby.exception_arg_error(), e.to_string()))?;
            let mut guard = get_grammar_cache().lock().unwrap();
            // Re-check in case another thread added it while we were parsing
            if let Some(cached) = guard.get(&hash) {
                cached.clone()
            } else {
                guard.insert(hash, grammar.clone());
                grammar
            }
        }
    };

    // Create Ruby builder wrapper
    let mut ruby_builder = RubyBuilder::new(builder);

    // Parse with builder
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, &input, &mut arena);

    parser
        .parse_with_builder(&mut ruby_builder)
        .map_err(|e| Error::new(ruby.exception_runtime_error(), e.to_string()))
}

/// Return optimized atom count for a grammar JSON (diagnostic)
pub fn optimized_atom_count(grammar_json: String) -> Result<usize, Error> {
    let ruby = Ruby::get().unwrap();
    let grammar: Grammar = load_grammar(&grammar_json)
        .map_err(|e| Error::new(ruby.exception_arg_error(), e.to_string()))?;
    Ok(grammar.atom_count())
}

/// Return count of atoms that need packrat caching (diagnostic)
pub fn cacheable_atom_count(grammar_json: String) -> Result<usize, Error> {
    let ruby = Ruby::get().unwrap();
    let grammar: Grammar = load_grammar(&grammar_json)
        .map_err(|e| Error::new(ruby.exception_arg_error(), e.to_string()))?;
    Ok(grammar.cacheable_atom_count())
}
