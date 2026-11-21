//! Bytecode VM execution engine
//!
//! This module implements the virtual machine that executes compiled bytecode.
//! The VM uses a stack-based backtracking approach inspired by LPeg.

use super::capture::{CaptureFrame, CaptureProcessor};
use super::error::{instruction_to_expected, ErrorTracker};
use super::instruction::Instruction;
use super::program::{NegTerm, Program};
use crate::portable::arena::AstArena;
use crate::portable::ast::{AstNode, ParseError, ParseResult};
use crate::portable::capture_state::CaptureState;
use crate::portable::char_class::utf8_char_len;
use crate::portable::regex_cache;
use std::time::Instant;

/// Maximum capture stack depth
const MAX_CAPTURE_DEPTH: usize = 100_000;

/// An open repetition: its iteration count, where the last counted
/// iteration ended, and whether that iteration consumed nothing.
#[derive(Debug, Clone, Copy)]
struct RepRegister {
    count: u32,
    last_end: usize,
    stalled: bool,
}

/// VM configuration options
#[derive(Debug, Clone)]
pub struct VMConfig {
    /// Maximum input size in bytes
    pub max_input_size: usize,

    /// Maximum recursion depth (backtrack stack depth)
    pub max_recursion_depth: usize,

    /// Timeout in milliseconds (0 = no timeout)
    pub timeout_ms: u64,

    /// Maximum memory usage in bytes
    pub max_memory: usize,

    /// Enable debug tracing
    pub debug: bool,
}

impl Default for VMConfig {
    fn default() -> Self {
        Self {
            max_input_size: 100_000_000, // 100MB
            max_recursion_depth: 10_000,
            timeout_ms: 30_000,      // 30 seconds
            max_memory: 500_000_000, // 500MB
            debug: false,
        }
    }
}

/// Result of VM execution
#[derive(Debug, Clone)]
pub struct VMResult {
    /// The parsed AST node
    pub value: AstNode,
    /// Final position in the input
    pub end_pos: usize,
}

/// Packrat memo entry for a rule call: (rule pc, position) -> outcome.
#[derive(Debug, Clone)]
struct MemoEntry {
    success: bool,
    end_pos: usize,
    value: Option<AstNode>,
}

/// The memo entry-count cap (see `memo_store`).
const MEMO_MAX: usize = 2_000_000;

/// Multiply-xor hasher for the memo's `(rule pc, position)` keys. The
/// packrat writes and reads on every rule call, and the default sip
/// hasher showed as the hot loop's largest block (hash per op plus the
/// un-pre-sized table's rehash churn); only key identity matters here,
/// never avalanche quality or iteration order.
#[derive(Default)]
struct MemoHasher(u64);

impl MemoHasher {
    #[inline]
    fn add(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(0x51_7c_c1_b7_27_22_0a_96);
    }
}

impl std::hash::Hasher for MemoHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut buf = [0u8; 8];
            buf[..chunk.len()].copy_from_slice(chunk);
            self.add(u64::from_le_bytes(buf));
        }
    }

    #[inline]
    fn write_u64(&mut self, word: u64) {
        self.add(word);
    }

    #[inline]
    fn write_usize(&mut self, word: usize) {
        self.add(word as u64);
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }
}

#[derive(Clone, Default)]
struct MemoBuild;

impl std::hash::BuildHasher for MemoBuild {
    type Hasher = MemoHasher;
    fn build_hasher(&self) -> MemoHasher {
        MemoHasher(0)
    }
}

type MemoMap = std::collections::HashMap<(usize, usize), MemoEntry, MemoBuild>;

/// What a backtrack frame represents when failure unwinds to it
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameKind {
    /// Ordered-choice point: jump to the alternative address
    Choice,
    /// Subroutine return address: skip during unwinding
    Return,
    /// Lookahead predicate boundary: skip during unwinding
    Predicate,
    /// Region marker (capture start / scope): skip during unwinding,
    /// truncating the marked region's side effects
    Mark,
}

#[derive(Debug, Clone)]
struct BacktrackFrame {
    /// Return instruction pointer (choice alternative or return address)
    return_ip: usize,
    /// Position to restore
    position: usize,
    /// Capture stack height to restore
    capture_height: usize,
    /// Value stack height to restore
    value_height: usize,
    /// Repetition register height to restore
    reg_height: usize,
    /// Recorded capture height to restore
    caps_height: usize,
    /// What this frame is
    kind: FrameKind,
    /// For Return frames: the memo key's rule pc, recorded on return
    /// (success) or on unwinding past the frame (failure).
    memo_rule: Option<usize>,
}

/// A `Regex` instruction's once-per-parse resolution: validity plus the
/// decoded single-class fast path when the pattern is one.
#[derive(Clone)]
struct ResolvedRegex {
    class: Option<std::sync::Arc<crate::portable::char_class::CharRanges>>,
}

/// The bytecode virtual machine
/// Decode a UTF-8 codepoint from its known-length byte slice.
#[inline]
fn decode_utf8_codepoint(bytes: &[u8]) -> u32 {
    let b0 = bytes[0];
    if b0 < 0x80 {
        b0 as u32
    } else if b0 < 0xE0 {
        ((u32::from(b0) & 0x1F) << 6) | (u32::from(bytes[1]) & 0x3F)
    } else if b0 < 0xF0 {
        ((u32::from(b0) & 0x0F) << 12)
            | ((u32::from(bytes[1]) & 0x3F) << 6)
            | (u32::from(bytes[2]) & 0x3F)
    } else {
        ((u32::from(b0) & 0x07) << 18)
            | ((u32::from(bytes[1]) & 0x3F) << 12)
            | ((u32::from(bytes[2]) & 0x3F) << 6)
            | (u32::from(bytes[3]) & 0x3F)
    }
}

/// The bytecode virtual machine
pub struct BytecodeVM<'a> {
    /// The program to execute
    program: &'a Program,

    /// The input bytes
    input: &'a [u8],

    /// The input as a string (for UTF-8 validation)
    input_str: &'a str,

    /// Current position in the input (byte index)
    position: usize,

    /// Instruction pointer
    ip: usize,

    /// Backtrack stack
    backtrack_stack: Vec<BacktrackFrame>,

    /// Capture stack
    capture_stack: Vec<CaptureFrame>,

    /// AST arena for result allocation
    arena: &'a mut AstArena,

    /// Configuration
    config: VMConfig,

    /// Start time (for timeout)
    start_time: Instant,

    /// Furthest failure position (for error reporting)
    furthest_failure: usize,

    /// Error tracker for detailed error messages
    error_tracker: ErrorTracker,

    /// Per-parse resolution of `Regex` instructions: the TLS caches
    /// (compile + HIR class decode) hash the pattern STRING per lookup,
    /// which on class-dense grammars ran per character. Resolved once
    /// per distinct regex_idx per parse instead.
    regex_state: Vec<Option<ResolvedRegex>>,

    /// Deferred failure events: (position, ip) pairs recorded on the hot
    /// path WITHOUT materializing their `Expected` labels — the label
    /// construction (format!/char-set scans) only ever matters for the
    /// furthest failure, so it happens once at parse end (see
    /// `materialize_deferred_failures`). Kept here, not on ErrorTracker,
    /// so the tracker's public surface is unchanged.
    deferred_failures: Vec<(usize, usize)>,

    /// The deferred buffer's own furthest position (independent of
    /// `furthest_failure`, which the contextless `track_failure()` also
    /// advances).
    deferred_failure_furthest: usize,

    /// Capture state for generational scopes
    capture_state: CaptureState,

    /// Tree-walker value model: produced values (one per completed atom)
    value_stack: Vec<AstNode>,

    /// Repetition registers: iteration counts anchoring BuildRep
    rep_registers: Vec<RepRegister>,

    /// Recorded captures: (name table index, start, length)
    recorded_captures: Vec<(u32, u32, u32)>,

    /// Backtrack count (diagnostics/budgeting)
    backtracks: u64,

    /// Rule-call memoization: (rule body pc, position) -> outcome.
    /// PEG parse results are context-free at a call boundary (Dynamic
    /// atoms are gated out of VM programs), so successes and failures
    /// alike are memoizable.
    memo: MemoMap,

    /// Budget for parse_with_vm_capped; None = unlimited.
    backtrack_budget: Option<u64>,

    /// Set when the budget tripped.
    budget_exceeded: bool,

    /// Rule-call memoization is unsound when the program invokes host
    /// callbacks (TODO.perf/5): their outcomes depend on capture state,
    /// which the memo key ignores.
    memo_enabled: bool,

    /// Per-opcode execution counts (parsanol-rs#100 item 2): None
    /// until `enable_profiling`.
    opcode_counts: Option<Box<[u64]>>,

    /// Interned pool indexes for the shape tags and the program's
    /// BuildHash names — per-program constants whose re-intern probe
    /// showed in the hot profile on every node build. Lazily filled;
    /// `None` = not yet interned this parse.
    tag_seq: Option<u32>,
    tag_rep: Option<u32>,
    tag_maybe: Option<u32>,
    name_pool: Vec<Option<u32>>,
}

fn vm_success_traced() -> bool {
    static TRACED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *TRACED.get_or_init(|| std::env::var("PARSANOL_VM_SUCCESS_TRACE").is_ok())
}

fn vm_choice_traced() -> bool {
    static TRACED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *TRACED.get_or_init(|| std::env::var("PARSANOL_VM_CHOICE_TRACE").is_ok())
}

impl<'a> BytecodeVM<'a> {
    /// Create a new VM instance
    #[inline]
    pub fn new(
        program: &'a Program,
        input: &'a str,
        arena: &'a mut AstArena,
        config: VMConfig,
    ) -> Self {
        if !crate::portable::dynamic::in_dynamic_dispatch() {
            crate::portable::dynamic::begin_parse(input);
        }
        Self {
            program,
            input: input.as_bytes(),
            input_str: input,
            position: 0,
            ip: program.entry_point(),
            backtrack_stack: Vec::with_capacity(256),
            capture_stack: Vec::with_capacity(64),
            arena,
            config,
            start_time: Instant::now(),
            furthest_failure: 0,
            error_tracker: ErrorTracker::new(),
            deferred_failures: Vec::new(),
            deferred_failure_furthest: 0,
            regex_state: Vec::new(),
            capture_state: CaptureState::new(),
            value_stack: Vec::with_capacity(64),
            rep_registers: Vec::with_capacity(16),
            recorded_captures: Vec::new(),
            backtracks: 0,
            backtrack_budget: None,
            budget_exceeded: false,
            memo: MemoMap::with_capacity_and_hasher(
                // Pre-size against the store cap: the raw table otherwise
                // grows through repeated rehash cycles on any real corpus
                // (the profiled malloc/madvise churn). Rule calls land at
                // roughly one to three positions per input byte.
                (input.len().saturating_mul(2)).clamp(64, MEMO_MAX),
                MemoBuild,
            ),
            memo_enabled: !program.has_invoke_dynamic(),
            opcode_counts: None,
            tag_seq: None,
            tag_rep: None,
            tag_maybe: None,
            name_pool: vec![None; program.string_count()],
        }
    }

    /// Number of backtracks performed during the run.
    pub fn backtrack_count(&self) -> u64 {
        self.backtracks
    }

    /// Start counting executed instructions per opcode discriminant
    /// (parsanol-rs#100): one counter per opcode value. Off by
    /// default; a single counted branch on the dispatch loop.
    pub fn enable_profiling(&mut self) {
        self.opcode_counts = Some(vec![0u64; 256].into_boxed_slice());
    }

    /// Per-opcode execution counts, indexed by the instruction's
    /// opcode discriminant (see `Instruction::opcode`).
    pub fn opcode_counts(&self) -> Option<&[u64]> {
        self.opcode_counts.as_deref()
    }

    /// Whether any terminal failure was tracked during the run.
    pub fn error_tracker_has_failures(&self) -> bool {
        !self.deferred_failures.is_empty() || self.error_tracker.has_failures()
    }

    /// Deepest terminal failure: the furthest position and its expected
    /// labels (see `parse_with_vm_diag`). Callers that track via the hot
    /// path must call `materialize_deferred_failures` first.
    pub fn failure_diagnostics(&self) -> (usize, Vec<String>) {
        (
            self.error_tracker.furthest_position(),
            self.error_tracker.expected_labels_at_furthest(),
        )
    }

    /// Run the VM and return the result
    pub fn run(&mut self) -> Result<VMResult, ParseError> {
        // Check input size
        if self.input.len() > self.config.max_input_size {
            return Err(ParseError::InputTooLarge {
                input_size: self.input.len(),
                max_size: self.config.max_input_size,
            });
        }

        // Main execution loop
        let mut timeout_tick: u32 = 0;
        loop {
            // Check timeout — periodically, not per iteration: a
            // clock_gettime per instruction dominated the hot loop
            // (28% of the rs#533 profile). A 30 s guard sampled every
            // 4096 instructions still catches pathological inputs at
            // sub-millisecond granularity.
            timeout_tick = timeout_tick.wrapping_add(1);
            if timeout_tick >= 4096 {
                timeout_tick = 0;
                if self.config.timeout_ms > 0 {
                    let elapsed = self.start_time.elapsed().as_millis() as u64;
                    if elapsed > self.config.timeout_ms {
                        return Err(ParseError::TimeoutExceeded {
                            elapsed_ms: elapsed,
                            timeout_ms: self.config.timeout_ms,
                        });
                    }
                }
            }

            // Check recursion depth
            if self.backtrack_stack.len() > self.config.max_recursion_depth {
                return Err(ParseError::RecursionLimitExceeded {
                    depth: self.backtrack_stack.len(),
                    max_depth: self.config.max_recursion_depth,
                });
            }

            // Fetch instruction
            // Fetch instruction. `program` is a shared `&'a` reference, so
            // the fetch borrows the program's lifetime, not `&mut self` —
            // the per-dispatch clone this replaces memcpy'd the enum
            // (~24B) on every executed instruction (rs#507's VM perf).
            let instr = match self.program.get_instruction(self.ip) {
                Some(i) => i,
                None => {
                    return Err(ParseError::Internal {
                        message: format!("Invalid instruction pointer: {}", self.ip),
                    })
                }
            };

            if self.config.debug {
                eprintln!(
                    "IP={:04} POS={} STK={} CAP={} {:?}",
                    self.ip,
                    self.position,
                    self.backtrack_stack.len(),
                    self.capture_stack.len(),
                    instr
                );
            }

            // Dispatch instruction
            // The scan instructions dispatch OUT of
            // execute_instruction: calls inside it degrade its hot
            // codegen (the 1.5.6 +12% receipt).
            match instr {
                Instruction::ScanClass { ranges_idx } => {
                    self.exec_scan_class(*ranges_idx)?;
                    // Continue: advance past this instruction, exactly
                    // like execute_instruction's Continue arm.
                    self.ip += 1;
                    continue;
                }
                Instruction::ScanUntil { spec_idx, max } => {
                    self.exec_scan_until(*spec_idx, *max)?;
                    self.ip += 1;
                    continue;
                }
                Instruction::DfaRun { table_idx } => {
                    // Unlike the scans (which always Continue), a DFA
                    // run legitimately fails — route that through the
                    // same backtrack handling as execute_instruction's
                    // Fail arm.
                    if matches!(self.exec_dfa_run(*table_idx)?, ExecutionResult::Continue) {
                        self.ip += 1;
                    } else if !self.backtrack()? {
                        return Err(ParseError::Failed {
                            position: self.furthest_failure,
                        });
                    }
                    continue;
                }
                _ => {}
            }

            match self.execute_instruction(instr)? {
                ExecutionResult::Continue => {
                    self.ip += 1;
                }
                ExecutionResult::Jump(offset) => {
                    self.ip = (self.ip as isize + 1 + offset as isize) as usize;
                }
                ExecutionResult::Success(value) => {
                    if vm_success_traced() {
                        eprintln!(
                            "parsanol-vm: Success at ip={} position={} value_stack={}",
                            self.ip,
                            self.position,
                            self.value_stack.len()
                        );
                    }
                    return Ok(VMResult {
                        value,
                        end_pos: self.position,
                    });
                }
                ExecutionResult::Fail => {
                    if !self.backtrack()? {
                        return Err(ParseError::Failed {
                            position: self.furthest_failure,
                        });
                    }
                }
            }
        }
    }

    /// Push a value for the current atom's match span
    #[inline]
    fn push_span_value(&mut self, start: usize) {
        let len = self.position - start;
        self.value_stack.push(self.arena.input_ref(start, len));
    }

    /// Push one routed SEQUENCE iteration's value: the
    /// `:sequence`-tagged array of the per-element values, each cut
    /// from the iteration's element-accept crossings and wrapped per
    /// its descriptor (bare span, or the `{name: span}` hash).
    #[inline]
    fn push_assembled_iteration(
        &mut self,
        iter_start: usize,
        cuts: &[usize],
        tag_seq: u32,
        element_keys: &[Option<u32>],
        element_inline: &[bool],
    ) {
        let mut vals: Vec<AstNode> = Vec::with_capacity(element_keys.len());
        // Consecutive InlineBare elements merge into ONE span (the
        // classic BuildSeq fold over inline spans); ref-produced and
        // hash-wrapped values stay separate.
        let mut run_start: Option<usize> = None;
        let mut run_end = 0usize;
        for i in 0..element_keys.len() {
            let elem_start = if i == 0 { iter_start } else { cuts[i - 1] };
            let elem_end = cuts[i];
            if element_inline[i] {
                if run_start.is_none() {
                    run_start = Some(elem_start);
                }
                run_end = elem_end;
            } else {
                if let Some(rs) = run_start.take() {
                    vals.push(self.arena.input_ref(rs, run_end - rs));
                }
                let span = self.arena.input_ref(elem_start, elem_end - elem_start);
                let wrapped = match element_keys[i] {
                    None => span,
                    Some(key_idx) => {
                        let (pool_idx, length) = self.arena.store_hash_indexed_key(key_idx, span);
                        AstNode::Hash {
                            pool_index: pool_idx,
                            length,
                        }
                    }
                };
                vals.push(wrapped);
            }
        }
        if let Some(rs) = run_start.take() {
            vals.push(self.arena.input_ref(rs, run_end - rs));
        }
        let (pool_idx, length) = self.arena.store_tagged_array_indexed(tag_seq, &vals);
        self.value_stack.push(AstNode::Array {
            pool_index: pool_idx,
            length,
        });
    }

    /// Push one routed-loop iteration's value: the bare span, the
    /// `:sequence`-tagged array wrapping the merged iteration span
    /// (a Sequence body's one-BuildSeq-per-iteration fold shape), and
    /// optionally Named-wrapped as `{name: inner}` (a Named body's
    /// BuildHash shape).
    #[inline]
    fn push_iteration_value(
        &mut self,
        iter_start: usize,
        boundary: usize,
        seq_wrapped: bool,
        named_idx: Option<u32>,
    ) {
        let len = boundary - iter_start;
        let span = self.arena.input_ref(iter_start, len);
        let inner = if seq_wrapped {
            let tag = match self.tag_seq {
                Some(idx) => idx,
                None => {
                    let idx = self.arena.intern_string_index(":sequence");
                    self.tag_seq = Some(idx);
                    idx
                }
            };
            let (pool_idx, length) = self.arena.store_tagged_array_indexed(tag, &[span]);
            AstNode::Array {
                pool_index: pool_idx,
                length,
            }
        } else {
            span
        };
        match named_idx {
            None => self.value_stack.push(inner),
            Some(key) => {
                let (pool_idx, length) = self.arena.store_hash_indexed_key(key, inner);
                self.value_stack.push(AstNode::Hash {
                    pool_index: pool_idx,
                    length,
                });
            }
        }
    }

    /// Restore all stacks to a frame's recorded heights
    #[inline]
    fn truncate_to(&mut self, frame: &BacktrackFrame) {
        self.capture_stack.truncate(frame.capture_height);
        self.value_stack.truncate(frame.value_height);
        self.rep_registers.truncate(frame.reg_height);
        self.recorded_captures.truncate(frame.caps_height);
    }

    /// Capture a snapshot frame of the current state
    #[inline]
    fn frame_snapshot(&self, return_ip: usize, kind: FrameKind) -> BacktrackFrame {
        BacktrackFrame {
            return_ip,
            position: self.position,
            capture_height: self.capture_stack.len(),
            value_height: self.value_stack.len(),
            reg_height: self.rep_registers.len(),
            caps_height: self.recorded_captures.len(),
            kind,
            memo_rule: None,
        }
    }

    fn return_frame(&self, return_ip: usize, memo_rule: usize) -> BacktrackFrame {
        BacktrackFrame {
            return_ip,
            position: self.position,
            capture_height: self.capture_stack.len(),
            value_height: self.value_stack.len(),
            reg_height: self.rep_registers.len(),
            caps_height: self.recorded_captures.len(),
            kind: FrameKind::Return,
            memo_rule: Some(memo_rule),
        }
    }

    /// Record a memo outcome unless the entry pool is saturated.
    /// Callers only reach this with `memo_rule` set, which happens
    /// only for eligible rules (all rules in dynamic-free programs,
    /// dynamic-free subtrees otherwise).
    #[inline]
    fn memo_store(&mut self, rule_pc: usize, pos: usize, entry: MemoEntry) {
        if self.memo.len() < MEMO_MAX {
            self.memo.insert((rule_pc, pos), entry);
        }
    }

    /// The DFA run (rs#574 slice B). Two modes:
    ///
    /// - **The plain recognizer** (no iteration-accept states in the
    ///   table): walk bytes, track the last accept position, succeed at
    ///   the longest accept (exact for the provable family), fail when
    ///   none.
    /// - **The envelope mode** (the routed repetition's loop table):
    ///   each iteration-accept crossing pushes the iteration's span;
    ///   on stuck the walk rewinds to the last boundary (a partial
    ///   iteration never counts — the PEG's no-partial rule), the
    ///   crossing count checks against `min`, and the pushed spans
    ///   build the same tagged envelope BuildRep constructs — the
    ///   tree is identical to the per-iteration Choice loop's.
    #[inline(never)]
    fn exec_dfa_run(&mut self, table_idx: u32) -> Result<ExecutionResult, ParseError> {
        let table = self
            .program
            .get_dfa_table(table_idx)
            .ok_or_else(|| ParseError::Internal {
                message: format!("Invalid DFA table index: {table_idx}"),
            })?;
        let iteration_accepts = self.program.get_dfa_iteration_accepts(table_idx);
        let seq_wrapped = self.program.get_dfa_loop_sequence_wrapped(table_idx);
        let named_idx: Option<u32> = self
            .program
            .get_dfa_loop_named_key(table_idx)
            .map(|name| self.arena.intern_string_index(name));
        let elements_plan = self.program.get_dfa_loop_elements(table_idx);
        let element_keys: Vec<Option<u32>> = elements_plan
            .map(|plan| {
                plan.names
                    .iter()
                    .map(|d| match d {
                        super::dfa_table::ElementDesc::Hash(name) => {
                            Some(self.arena.intern_string_index(name))
                        }
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let element_inline: Vec<bool> = elements_plan
            .map(|plan| {
                plan.names
                    .iter()
                    .map(|d| matches!(d, super::dfa_table::ElementDesc::InlineBare))
                    .collect()
            })
            .unwrap_or_default();
        let element_accepts: &[Option<u8>] = elements_plan
            .map(|plan| plan.accepts_per_state.as_slice())
            .unwrap_or(&[]);
        let element_count = elements_plan.map(|plan| plan.names.len()).unwrap_or(0);
        let mut cuts: Vec<usize> = vec![0; element_count];
        let tag_seq_idx: Option<u32> = if element_count > 0 {
            Some(match self.tag_seq {
                Some(idx) => idx,
                None => {
                    let idx = self.arena.intern_string_index(":sequence");
                    self.tag_seq = Some(idx);
                    idx
                }
            })
        } else {
            None
        };
        // The routed loop's min rides the registry next to the
        // iteration-accept states; rule tables carry none.
        let min = self.program.get_dfa_loop_min(table_idx).unwrap_or(0);
        let input = self.input;
        let start = self.position;
        let envelope_mode = iteration_accepts.is_some_and(|a| a.iter().any(|&b| b));
        let mut state = table.start;
        let mut last_accept: Option<usize> = if table.is_accept(state) {
            Some(start)
        } else {
            None
        };
        // The envelope mode's bookkeeping.
        let entry_len = self.value_stack.len();
        let mut iterations = 0usize;
        let mut boundary = start;
        let mut iteration_start = start;
        while self.position < input.len() {
            match table.step(state, input[self.position]) {
                Some(next) => {
                    state = next;
                    self.position += 1;
                    if table.is_accept(state) {
                        last_accept = Some(self.position);
                    }
                    // Tranche 4: an element accept records the cut;
                    // a chain state carries at most one element accept.
                    if let Some(&Some(i)) = element_accepts.get(state as usize) {
                        cuts[i as usize] = self.position;
                    }
                    if envelope_mode && iteration_accepts.is_some_and(|a| a[state as usize]) {
                        iterations += 1;
                        boundary = self.position;
                        let iter_start = iteration_start;
                        iteration_start = self.position;
                        if element_count > 0 {
                            self.push_assembled_iteration(
                                iter_start,
                                &cuts,
                                tag_seq_idx.unwrap_or_default(),
                                &element_keys,
                                &element_inline,
                            );
                        } else {
                            self.push_iteration_value(iter_start, boundary, seq_wrapped, named_idx);
                        }
                    }
                }
                None => break,
            }
        }
        if envelope_mode {
            // The PEG's no-partial rule: the walk ending mid-body
            // rewinds to the last iteration boundary, dropping the
            // partial iteration's span.
            if self.position != boundary {
                self.value_stack.truncate(entry_len + iterations);
                self.position = boundary;
            }
            if (iterations as u64) < u64::from(min) {
                self.value_stack.truncate(entry_len);
                self.position = start;
                self.track_failure();
                return Ok(ExecutionResult::Fail);
            }
            // BuildRep(maybe = false) with the pushed spans: the same
            // tagged envelope, byte-identical to the Choice loop's.
            let tag = match self.tag_rep {
                Some(idx) => idx,
                None => {
                    let idx = self.arena.intern_string_index(":repetition");
                    self.tag_rep = Some(idx);
                    idx
                }
            };
            let (pool_idx, len) = self
                .arena
                .store_tagged_array_indexed(tag, &self.value_stack[entry_len..]);
            self.value_stack.truncate(entry_len);
            self.value_stack.push(AstNode::Array {
                pool_index: pool_idx,
                length: len,
            });
            return Ok(ExecutionResult::Continue);
        }
        match last_accept {
            Some(end) => {
                self.position = end;
                self.push_span_value(start);
                Ok(ExecutionResult::Continue)
            }
            None => {
                self.position = start;
                self.track_failure();
                Ok(ExecutionResult::Fail)
            }
        }
    }

    /// The bulk class-run: exact per-char decode + range check, per-char
    /// span values (tree-identical to the per-char choice loop this
    /// replaces). Dispatched from `run`, never inside
    /// `execute_instruction`: calls there degrade the hot match's
    /// codegen (the 1.5.6 +12% receipt).
    #[inline(never)]
    fn exec_scan_class(&mut self, ranges_idx: u32) -> Result<ExecutionResult, ParseError> {
        let program = self.program;
        let ranges: &[(u32, u32)] =
            program
                .get_char_ranges(ranges_idx)
                .ok_or_else(|| ParseError::Internal {
                    message: format!("Invalid char-range index: {ranges_idx}"),
                })?;
        let input = self.input;
        while self.position < input.len() {
            let b = input[self.position];
            let char_len = utf8_char_len(b);
            if self.position + char_len > input.len() {
                break;
            }
            let cp = decode_utf8_codepoint(&input[self.position..self.position + char_len]);
            let member = ranges.iter().any(|&(lo, hi)| cp >= lo && cp <= hi);
            if !member {
                break;
            }
            let start = self.position;
            self.position += char_len;
            // The span closes at the NEW position: values measure
            // position - start.
            self.push_span_value(start);
            if let Some(reg) = self.rep_registers.last_mut() {
                reg.count += 1;
                reg.last_end = self.position;
                reg.stalled = false;
            }
        }
        Ok(ExecutionResult::Continue)
    }

    /// The quoted-string idiom's fused run (see exec_scan_class).
    #[inline(never)]
    fn exec_scan_until(&mut self, spec_idx: u32, max: u32) -> Result<ExecutionResult, ParseError> {
        let program = self.program;
        let spec = program
            .get_neg_spec(spec_idx)
            .ok_or_else(|| ParseError::Internal {
                message: format!("Invalid neg-scan spec index: {spec_idx}"),
            })?;
        let delims = &spec.delims;
        let term = &spec.term;
        let input_bytes = self.input;
        let input_str = self.input_str;
        let max_count = max as usize;
        let mut count = 0usize;
        while count < max_count && self.position < input_bytes.len() {
            if delims
                .iter()
                .any(|d| input_str[self.position..].starts_with(d.as_str()))
            {
                break;
            }
            let len = match term {
                NegTerm::Lit(s) => {
                    if input_str[self.position..].starts_with(s.as_str()) {
                        s.len()
                    } else {
                        break;
                    }
                }
                NegTerm::Ranges(r) => match r.match_at(input_bytes, self.position) {
                    Some(l) => l,
                    None => break,
                },
                NegTerm::CharPat(p) => {
                    let b = input_bytes[self.position];
                    if p.matches(b) {
                        match p {
                            crate::portable::char_class::CharacterPattern::Any
                            | crate::portable::char_class::CharacterPattern::NonDigit
                            | crate::portable::char_class::CharacterPattern::NonSpace
                            | crate::portable::char_class::CharacterPattern::NonWord => {
                                utf8_char_len(b)
                            }
                            _ => 1,
                        }
                    } else {
                        break;
                    }
                }
            };
            if self.position + len > input_bytes.len() {
                break;
            }
            let t = self.arena.input_ref(self.position, len);
            let tag = match self.tag_seq {
                Some(idx) => idx,
                None => {
                    let idx = self.arena.intern_string_index(":sequence");
                    self.tag_seq = Some(idx);
                    idx
                }
            };
            let (pool_idx, slen) = self
                .arena
                .store_tagged_array_indexed(tag, &[AstNode::Nil, t]);
            self.value_stack.push(AstNode::Array {
                pool_index: pool_idx,
                length: slen,
            });
            self.position += len;
            if let Some(reg) = self.rep_registers.last_mut() {
                reg.count += 1;
                reg.last_end = self.position;
                reg.stalled = false;
            }
            count += 1;
        }
        Ok(ExecutionResult::Continue)
    }

    fn execute_instruction(&mut self, instr: &Instruction) -> Result<ExecutionResult, ParseError> {
        if let Some(counts) = &mut self.opcode_counts {
            counts[instr.opcode() as usize] += 1;
        }
        match instr {
            // ========================================================================
            // Matching Instructions
            // ========================================================================
            Instruction::Any { n } => {
                let n = *n as usize;
                let start = self.position;
                if self.position + n <= self.input.len() {
                    // Advance position by n characters (not bytes)
                    let mut chars = 0usize;
                    let mut pos = self.position;
                    while chars < n && pos < self.input.len() {
                        let char_len = utf8_char_len(self.input[pos]);
                        pos += char_len;
                        chars += 1;
                    }
                    if chars == n {
                        self.position = pos;
                        self.push_span_value(start);
                        Ok(ExecutionResult::Continue)
                    } else {
                        self.track_failure_with_context();
                        Ok(ExecutionResult::Fail)
                    }
                } else {
                    self.track_failure_with_context();
                    Ok(ExecutionResult::Fail)
                }
            }

            Instruction::Char { byte } => {
                if self.position < self.input.len() && self.input[self.position] == *byte {
                    let start = self.position;
                    self.position += 1;
                    self.push_span_value(start);
                    Ok(ExecutionResult::Continue)
                } else {
                    self.track_failure_with_context();
                    Ok(ExecutionResult::Fail)
                }
            }

            Instruction::CharSet { set_idx } => {
                let set =
                    self.program
                        .get_char_set(*set_idx)
                        .ok_or_else(|| ParseError::Internal {
                            message: format!("Invalid charset index: {}", set_idx),
                        })?;

                if self.position < self.input.len() && set.contains(self.input[self.position]) {
                    let start = self.position;
                    let char_len = utf8_char_len(self.input[self.position]);
                    self.position += char_len;
                    self.push_span_value(start);
                    Ok(ExecutionResult::Continue)
                } else {
                    self.track_failure_with_context();
                    Ok(ExecutionResult::Fail)
                }
            }

            Instruction::String { str_idx, len } => {
                let s = self
                    .program
                    .get_string(*str_idx)
                    .ok_or_else(|| ParseError::Internal {
                        message: format!("Invalid string index: {}", str_idx),
                    })?;

                let s_bytes = s.as_bytes();
                let end = self.position + s_bytes.len();

                if end <= self.input.len()
                    && &self.input[self.position..end] == s_bytes
                    && *len as usize == s_bytes.len()
                {
                    let start = self.position;
                    self.position = end;
                    self.push_span_value(start);
                    Ok(ExecutionResult::Continue)
                } else {
                    self.track_failure_with_context();
                    Ok(ExecutionResult::Fail)
                }
            }

            Instruction::Regex { regex_idx } => {
                let pattern =
                    self.program
                        .get_regex(*regex_idx)
                        .ok_or_else(|| ParseError::Internal {
                            message: format!("Invalid regex index: {}", regex_idx),
                        })?;

                // Resolve once per distinct regex_idx per parse: the
                // TLS caches hash the pattern string per lookup, which
                // on class-dense grammars ran per character.
                let idx = *regex_idx as usize;
                if idx >= self.regex_state.len() {
                    self.regex_state.resize_with(idx + 1, || None);
                }
                if self.regex_state[idx].is_none() {
                    match regex_cache::get_or_compile(pattern) {
                        None => {
                            return Err(ParseError::Internal {
                                message: format!("Failed to compile regex: {}", pattern),
                            });
                        }
                        Some(_) => {
                            let class = regex_cache::get_or_decode_class(pattern);
                            self.regex_state[idx] = Some(ResolvedRegex { class });
                        }
                    }
                }
                let class = self.regex_state[idx].as_ref().and_then(|r| r.class.clone());

                // The tree-walker rejects regexes at end-of-input (even
                // zero-width-matchable ones) — parity requires the same.
                if self.position >= self.input.len() {
                    self.track_failure_with_context();
                    return Ok(ExecutionResult::Fail);
                }

                // Pure-class fast path (rs#507: the VM-parity of the
                // interpreter's): a pattern whose HIR is a single Unicode
                // class matches by direct membership — the regex engine's
                // lazy-DFA determinization is catastrophic per character,
                // and the keyword spellings lower to per-char class
                // patterns, so this is the VM's dominant corpus cost.
                if let Some(class) = class {
                    if let Some(char_len) = class.match_at(self.input, self.position) {
                        let start = self.position;
                        self.position += char_len;
                        self.push_span_value(start);
                        return Ok(ExecutionResult::Continue);
                    }
                    self.track_failure();
                    return Ok(ExecutionResult::Fail);
                }

                // Match anchored at the current position: an unanchored
                // search scans to the end of the input on every failed
                // attempt (quadratic under repetition).
                if let Some(end) = regex_cache::match_end_at(pattern, self.input_str, self.position)
                {
                    let start = self.position;
                    self.position = end;
                    self.push_span_value(start);
                    return Ok(ExecutionResult::Continue);
                }

                self.track_failure();
                Ok(ExecutionResult::Fail)
            }

            // ========================================================================
            // Test Instructions
            // ========================================================================
            Instruction::TestChar { byte, offset } => {
                if self.position < self.input.len() && self.input[self.position] == *byte {
                    let start = self.position;
                    self.position += 1;
                    self.push_span_value(start);
                    Ok(ExecutionResult::Continue)
                } else {
                    self.track_failure();
                    Ok(ExecutionResult::Jump(*offset))
                }
            }

            Instruction::TestSet { set_idx, offset } => {
                let set =
                    self.program
                        .get_char_set(*set_idx)
                        .ok_or_else(|| ParseError::Internal {
                            message: format!("Invalid charset index: {}", set_idx),
                        })?;

                if self.position < self.input.len() && set.contains(self.input[self.position]) {
                    let start = self.position;
                    let char_len = utf8_char_len(self.input[self.position]);
                    self.position += char_len;
                    self.push_span_value(start);
                    Ok(ExecutionResult::Continue)
                } else {
                    self.track_failure();
                    Ok(ExecutionResult::Jump(*offset))
                }
            }

            Instruction::TestAny { n, offset } => {
                let n = *n as usize;
                let start = self.position;
                if self.position + n <= self.input.len() {
                    // Check for n characters
                    let mut chars = 0usize;
                    let mut pos = self.position;
                    while chars < n && pos < self.input.len() {
                        let char_len = utf8_char_len(self.input[pos]);
                        pos += char_len;
                        chars += 1;
                    }
                    if chars == n {
                        self.position = pos;
                        self.push_span_value(start);
                        return Ok(ExecutionResult::Continue);
                    }
                }
                self.track_failure();
                Ok(ExecutionResult::Jump(*offset))
            }

            // ========================================================================
            // Control Flow
            // ========================================================================
            Instruction::Jump { offset } => Ok(ExecutionResult::Jump(*offset)),

            Instruction::Call { offset } => {
                let target = (self.ip as i32 + 1 + offset) as usize;
                let pos = self.position;

                // Selective memoization (#100.1/#90.3): rule outcomes
                // are context-free at a call boundary for dynamic-free
                // rules. Programs with host calls memoize only the
                // dynamic-free transitive rule subtrees — the memo
                // key ignores capture state, and replaying a tainted
                // rule would skip host side effects.
                let can_memo = self.memo_enabled || self.program.is_rule_memoizable(target);
                if can_memo {
                    if let Some(entry) = self.memo.get(&(target, pos)) {
                        if entry.success {
                            let value = entry.value.clone().unwrap_or(AstNode::Nil);
                            self.position = entry.end_pos;
                            self.value_stack.push(value);
                            Ok(ExecutionResult::Continue)
                        } else {
                            self.track_failure();
                            Ok(ExecutionResult::Fail)
                        }
                    } else {
                        self.backtrack_stack
                            .push(self.return_frame(self.ip + 1, target));
                        Ok(ExecutionResult::Jump(*offset))
                    }
                } else {
                    self.backtrack_stack
                        .push(self.frame_snapshot(self.ip + 1, FrameKind::Return));
                    Ok(ExecutionResult::Jump(*offset))
                }
            }

            Instruction::Return => {
                // Pop return frame
                if let Some(frame) = self.backtrack_stack.pop() {
                    if frame.kind == FrameKind::Return {
                        if let Some(rule_pc) = frame.memo_rule {
                            let value = self.value_stack.last().cloned();
                            self.memo_store(
                                rule_pc,
                                frame.position,
                                MemoEntry {
                                    success: true,
                                    end_pos: self.position,
                                    value,
                                },
                            );
                        }
                        self.ip = frame.return_ip;
                        return Ok(ExecutionResult::Jump(-1)); // -1 because we'll add 1 in the main loop
                    }
                }

                // No frame to return to, success
                let value = self.build_result()?;
                Ok(ExecutionResult::Success(value))
            }

            Instruction::End => {
                let value = if self.value_stack.len() == 1 {
                    self.value_stack.pop().unwrap_or(AstNode::Nil)
                } else {
                    self.build_result()?
                };
                Ok(ExecutionResult::Success(value))
            }

            // ========================================================================
            // Backtracking
            // ========================================================================
            Instruction::Choice { offset } => {
                let alt = (self.ip as i32 + 1 + offset) as usize;
                self.backtrack_stack
                    .push(self.frame_snapshot(alt, FrameKind::Choice));
                Ok(ExecutionResult::Continue)
            }

            Instruction::Commit { offset } => {
                // Pop the choice point
                self.backtrack_stack.pop();
                Ok(ExecutionResult::Jump(*offset))
            }

            Instruction::PartialCommit { offset } => {
                // An iteration that consumed nothing would repeat forever:
                // like the tree-walker, count it once (its RepCount ran)
                // and leave the loop, which ends at this instruction.
                if self
                    .backtrack_stack
                    .last()
                    .is_some_and(|frame| frame.position == self.position)
                {
                    self.backtrack_stack.pop();
                    return Ok(ExecutionResult::Continue);
                }
                // Update the choice point with current position and the
                // completed iterations' side effects, so a later failure
                // keeps them and only discards the failing iteration.
                if let Some(frame) = self.backtrack_stack.last_mut() {
                    frame.position = self.position;
                    frame.capture_height = self.capture_stack.len();
                    frame.value_height = self.value_stack.len();
                    frame.reg_height = self.rep_registers.len();
                    frame.caps_height = self.recorded_captures.len();
                }
                Ok(ExecutionResult::Jump(*offset))
            }

            Instruction::BackCommit { offset } => {
                // Pop frame, restore position, then jump
                if let Some(frame) = self.backtrack_stack.pop() {
                    self.position = frame.position;
                    self.truncate_to(&frame);
                }
                Ok(ExecutionResult::Jump(*offset))
            }

            Instruction::Fail => {
                self.track_failure();
                Ok(ExecutionResult::Fail)
            }

            Instruction::FailTwice => {
                self.track_failure();
                // Pop one frame first, discarding its side effects
                if let Some(frame) = self.backtrack_stack.pop() {
                    self.truncate_to(&frame);
                }
                Ok(ExecutionResult::Fail)
            }

            // ========================================================================
            // Captures
            // ========================================================================
            Instruction::OpenCapture { kind, key_idx } => {
                if self.capture_stack.len() >= MAX_CAPTURE_DEPTH {
                    return Err(ParseError::RecursionLimitExceeded {
                        depth: self.capture_stack.len(),
                        max_depth: MAX_CAPTURE_DEPTH,
                    });
                }

                self.capture_stack
                    .push(CaptureFrame::open(self.position, *kind, *key_idx));
                Ok(ExecutionResult::Continue)
            }

            Instruction::CloseCapture { kind, key_idx } => {
                // Close the most recent matching open capture
                // Search backwards for a matching open capture
                for frame in self.capture_stack.iter_mut().rev() {
                    if !frame.is_closed() && frame.kind == *kind && frame.key_idx == *key_idx {
                        frame.close(self.position);
                        break;
                    }
                }
                Ok(ExecutionResult::Continue)
            }

            Instruction::FullCapture { kind, key_idx } => {
                // Full capture in one instruction (for fixed-length patterns)
                // Create an already-closed capture at current position
                let mut frame = CaptureFrame::open(self.position, *kind, *key_idx);
                frame.close(self.position);
                self.capture_stack.push(frame);
                Ok(ExecutionResult::Continue)
            }

            // ========================================================================
            // Predicates
            // ========================================================================
            Instruction::PredChoice { offset } => {
                let alt = (self.ip as i32 + 1 + offset) as usize;
                self.backtrack_stack
                    .push(self.frame_snapshot(alt, FrameKind::Predicate));
                Ok(ExecutionResult::Continue)
            }

            // ========================================================================
            // Advanced
            // ========================================================================
            Instruction::Behind { n } => {
                // Move position backward by n characters
                let n = *n as usize;
                let mut chars = 0usize;
                while chars < n && self.position > 0 {
                    // Find the start of the previous character
                    let mut back = 1usize;
                    while back < 4 && self.position > back {
                        let byte = self.input[self.position - back];
                        if (byte & 0xC0) != 0x80 {
                            break; // Found start of character
                        }
                        back += 1;
                    }
                    self.position -= back;
                    chars += 1;
                }

                if chars == n {
                    Ok(ExecutionResult::Continue)
                } else {
                    self.track_failure();
                    Ok(ExecutionResult::Fail)
                }
            }

            Instruction::DfaRun { .. } => {
                // Unreachable through `run` (the pre-dispatch catches
                // DfaRun before execute_instruction); the belt for any
                // future direct caller.
                Err(ParseError::Internal {
                    message: "DfaRun dispatched inside execute_instruction".to_string(),
                })
            }
            Instruction::NoOp => Ok(ExecutionResult::Continue),

            Instruction::Throw { label_idx } => {
                let label = self.program.get_label(*label_idx).unwrap_or("unknown");
                Err(ParseError::Internal {
                    message: format!("Parse error: {}", label),
                })
            }

            Instruction::ThrowRec {
                label_idx,
                recovery_offset,
            } => {
                let label = self.program.get_label(*label_idx).unwrap_or("unknown");
                // For now, just fail - recovery would be implemented later
                let _ = label;
                let _ = recovery_offset;
                self.track_failure();
                Ok(ExecutionResult::Fail)
            }

            Instruction::Span { set_idx } => {
                // Scan the member-run via the derived plan (TODO.perf/2):
                // comparison-shaped plans vectorize; the fallback keeps
                // the lead-byte-stepping semantics for sets with
                // non-ASCII members.
                let start = self.position;
                let program = self.program;
                match program.scan_plan(*set_idx) {
                    Some(plan) => self.position = plan.scan_run(self.input, self.position),
                    None => {
                        let set =
                            program
                                .get_char_set(*set_idx)
                                .ok_or_else(|| ParseError::Internal {
                                    message: format!("Invalid charset index: {}", set_idx),
                                })?;
                        while self.position < self.input.len()
                            && set.contains(self.input[self.position])
                        {
                            let char_len = utf8_char_len(self.input[self.position]);
                            self.position += char_len;
                        }
                    }
                }

                self.push_span_value(start);
                Ok(ExecutionResult::Continue)
            }

            Instruction::ScanClass { .. } | Instruction::ScanUntil { .. } => {
                unreachable!("scan instructions are dispatched in run")
            }

            Instruction::Custom { id } => {
                // Call the custom atom at runtime
                let result =
                    crate::portable::custom::parse_custom_atom(*id, self.input_str, self.position);

                match result {
                    Some(custom_result) => {
                        // Custom atom matched, update position
                        let start_pos = self.position;
                        self.position = custom_result.end_pos;

                        // The matched span is the value, mirroring every
                        // other terminal in the tree-walker value model.
                        if let Some(value) = custom_result.value {
                            self.value_stack.push(value);
                        } else {
                            self.push_span_value(start_pos);
                        }

                        Ok(ExecutionResult::Continue)
                    }
                    None => {
                        // Custom atom did not match
                        self.track_failure();
                        Ok(ExecutionResult::Fail)
                    }
                }
            }

            // ========================================================================
            // Capture Scope Instructions
            // ========================================================================
            Instruction::PushScope => {
                // Push a new capture scope
                self.capture_state.try_push_scope()?;
                Ok(ExecutionResult::Continue)
            }

            Instruction::PopScope => {
                // Pop the capture scope (discards inner captures)
                self.capture_state.pop_scope();
                Ok(ExecutionResult::Continue)
            }

            // ========================================================================
            // Dynamic Atom Instructions
            // ========================================================================
            Instruction::InvokeDynamic { callback_id } => {
                // Dynamic atoms use Packrat fallback, under the shared
                // recursion/budget guards (GH-76): divergent dispatch
                // fails loudly instead of hanging or ballooning.
                use crate::portable::arena::AstArena;
                use crate::portable::dynamic::{
                    enter_dynamic, with_dynamic_callback, DynamicContext,
                };
                use crate::portable::grammar::Grammar;
                use crate::portable::parser::PortableParser;

                let start_pos = self.position;
                let _guard = match enter_dynamic() {
                    Some(g) => g,
                    None => {
                        self.track_failure();
                        return Ok(ExecutionResult::Fail);
                    }
                };

                // Create context for callback. Capture subtrees ride
                // inside the capture state so the block reads the
                // parsed TREE (capture parity).
                let ctx = DynamicContext::new(
                    self.input_str,
                    self.position,
                    self.capture_state.clone_visible(),
                );

                // Dispatch cache (parsanol-ruby#80): a deterministic
                // block's fragment is a pure function of (input, pos,
                // captures) — the same policy as the packrat engine. A
                // hit skips the host round-trip and replays the
                // recorded capture writes.
                let temp_grammar: std::sync::Arc<Grammar> =
                    match crate::portable::dynamic::cached_fragment(
                        *callback_id,
                        self.position,
                        &self.capture_state,
                        self.input_str,
                    ) {
                        Some((g, _root, writes)) => {
                            for (name, value) in writes {
                                let cv = match value {
                                    crate::portable::dynamic::WriteValue::Text(t) => {
                                        crate::portable::capture_state::CaptureValue::text(t)
                                    }
                                    crate::portable::dynamic::WriteValue::Opaque(handle) => {
                                        crate::portable::capture_state::CaptureValue::opaque(handle)
                                    }
                                };
                                self.capture_state.store(&name, cv);
                            }
                            g
                        }
                        None => {
                            // Writes the block made to its context
                            // (parsanol-ruby#80) land before the
                            // fragment parses, mirroring the packrat
                            // engine.
                            crate::portable::dynamic::drain_capture_writes_into(
                                &mut self.capture_state,
                            );
                            // Invoke callback: a fragment grammar (self-consistent
                            // atom indices, the shape host bridges produce) wins
                            // over a bare atom, which is appended to a fresh
                            // grammar whose root is the new atom.
                            let resolved = match with_dynamic_callback(*callback_id, |cb| {
                                if let Some((fragment, root)) = cb.resolve_fragment(&ctx) {
                                    let mut fragment = fragment;
                                    fragment.root = root;
                                    return Some(fragment);
                                }
                                cb.resolve(&ctx).map(|atom| {
                                    let mut g = Grammar::new();
                                    let id = g.add_atom(atom);
                                    g.root = id;
                                    g
                                })
                            }) {
                                Some(g) => g,
                                None => {
                                    self.track_failure();
                                    return Ok(ExecutionResult::Fail);
                                }
                            };
                            let writes = crate::portable::dynamic::take_pending_writes();
                            for (name, value) in &writes {
                                let cv = match value {
                                    crate::portable::dynamic::WriteValue::Text(t) => {
                                        crate::portable::capture_state::CaptureValue::text(
                                            t.clone(),
                                        )
                                    }
                                    crate::portable::dynamic::WriteValue::Opaque(handle) => {
                                        crate::portable::capture_state::CaptureValue::opaque(
                                            *handle,
                                        )
                                    }
                                };
                                self.capture_state.store(name, cv);
                            }
                            let root = resolved.root;
                            crate::portable::dynamic::store_dispatch_fragment(
                                *callback_id,
                                self.position,
                                &self.capture_state,
                                self.input_str,
                                resolved.clone(),
                                root,
                                writes,
                            );
                            std::sync::Arc::new(resolved)
                        }
                    };

                // Outcome replay (rs#174): when this exact dispatch
                // was already parsed under this arena, reuse the
                // recorded end position and value instead of re-running
                // a fresh empty-memo fragment parse. Outer backtracking
                // re-invokes dynamics at the same positions
                // exponentially; the re-parse was the whole cost
                // (coradoc table: minutes / 16GB).
                let arena_tag = self.arena.id();
                eprintln!("DYN invoke cb={} pos={}", callback_id, self.position);
                if let Some(outcome) = crate::portable::dynamic::cached_dynamic_outcome(
                    *callback_id,
                    self.position,
                    &self.capture_state,
                    self.input_str,
                    arena_tag,
                ) {
                    if outcome.ok {
                        eprintln!("DYN outcome HIT ok end={}", outcome.end_pos);
                        self.position = outcome.end_pos;
                        self.value_stack.push(outcome.value);
                        return Ok(ExecutionResult::Continue);
                    }
                    self.track_failure();
                    return Ok(ExecutionResult::Fail);
                }

                // Create temporary arena
                let mut temp_arena = AstArena::for_input(self.input_str.len());

                // Use Packrat parser for dynamic atom, seeded with the
                // VM's captures so capture reads inside the fragment
                // observe state set before the dynamic atom (GH-76).
                let mut temp_parser =
                    PortableParser::new(&temp_grammar, self.input_str, &mut temp_arena);
                for name in self.capture_state.names() {
                    if let Some(value) = self.capture_state.get(name) {
                        temp_parser.capture_state_mut().store(name, value);
                    }
                }
                match temp_parser.parse_from_pos(self.position) {
                    Ok(result) => {
                        // Merge captures from temp parser
                        let temp_captures = temp_parser.capture_state();
                        for name in temp_captures.names() {
                            if let Some(value) = temp_captures.get(name) {
                                self.capture_state.store(name, value);
                            }
                        }
                        self.position = result.end_pos;
                        // The subtree was built in the fragment's
                        // temporary arena: adopt pool-backed nodes into
                        // the VM arena before the value joins the stack
                        // (GH-76 cross-arena fix).
                        let value = self.arena.adopt_node(&temp_arena, &result.value);
                        crate::portable::dynamic::store_dynamic_outcome(
                            *callback_id,
                            start_pos,
                            &self.capture_state,
                            self.input_str,
                            crate::portable::dynamic::DynamicOutcome {
                                end_pos: result.end_pos,
                                value: value.clone(),
                                arena_tag,
                                ok: true,
                            },
                        );
                        self.value_stack.push(value);
                        Ok(ExecutionResult::Continue)
                    }
                    Err(error @ ParseError::RecursionLimitExceeded { .. }) => Err(error),
                    Err(_) => {
                        crate::portable::dynamic::store_dynamic_outcome(
                            *callback_id,
                            start_pos,
                            &self.capture_state,
                            self.input_str,
                            crate::portable::dynamic::DynamicOutcome {
                                end_pos: self.position,
                                value: crate::portable::ast::AstNode::Nil,
                                arena_tag,
                                ok: false,
                            },
                        );
                        self.track_failure();
                        Ok(ExecutionResult::Fail)
                    }
                }
            }

            // ========================================================================
            // Tree-walker value model
            // ========================================================================
            Instruction::ToNil => {
                self.value_stack.pop();
                self.value_stack.push(AstNode::Nil);
                Ok(ExecutionResult::Continue)
            }

            Instruction::PushNil => {
                self.value_stack.push(AstNode::Nil);
                Ok(ExecutionResult::Continue)
            }

            Instruction::ByteDispatch { table_idx } => {
                let table = self.program.get_dispatch_table(*table_idx).ok_or_else(|| {
                    ParseError::Internal {
                        message: format!("Invalid dispatch table index: {table_idx}"),
                    }
                })?;
                if self.position >= self.input.len() {
                    self.track_failure_with_context();
                    return Ok(ExecutionResult::Fail);
                }
                let offset = table[self.input[self.position] as usize];
                if offset < 0 {
                    self.track_failure_with_context();
                    return Ok(ExecutionResult::Fail);
                }
                Ok(ExecutionResult::Jump(offset))
            }

            Instruction::BuildSeq { n } => {
                let n = *n as usize;
                if self.value_stack.len() < n {
                    return Err(ParseError::Internal {
                        message: format!("BuildSeq underflow: need {n} values"),
                    });
                }
                // Store straight from the stack tail — the split_off
                // allocated a fresh Vec per node build.
                let start = self.value_stack.len() - n;
                let tag = match self.tag_seq {
                    Some(idx) => idx,
                    None => {
                        let idx = self.arena.intern_string_index(":sequence");
                        self.tag_seq = Some(idx);
                        idx
                    }
                };
                let (pool_idx, len) = self
                    .arena
                    .store_tagged_array_indexed(tag, &self.value_stack[start..]);
                self.value_stack.truncate(start);
                self.value_stack.push(AstNode::Array {
                    pool_index: pool_idx,
                    length: len,
                });
                Ok(ExecutionResult::Continue)
            }

            Instruction::RepOpen => {
                self.rep_registers.push(RepRegister {
                    count: 0,
                    last_end: self.position,
                    stalled: false,
                });
                Ok(ExecutionResult::Continue)
            }

            Instruction::RepCount => {
                if let Some(reg) = self.rep_registers.last_mut() {
                    let progressed = self.position != reg.last_end;
                    if !progressed && reg.stalled {
                        // The tree-walker stops after the first iteration
                        // that consumes nothing; this one repeats it, so
                        // it is not counted and its value is dropped.
                        self.value_stack.pop();
                    } else {
                        reg.count += 1;
                        reg.last_end = self.position;
                        reg.stalled = !progressed;
                    }
                }
                Ok(ExecutionResult::Continue)
            }

            Instruction::BuildRep { maybe } => {
                let n = self.rep_registers.pop().map_or(0, |reg| reg.count) as usize;
                if self.value_stack.len() < n {
                    return Err(ParseError::Internal {
                        message: format!("BuildRep underflow: need {n} values"),
                    });
                }
                if *maybe && n == 1 {
                    // A present optional flattens to its value in every
                    // context; only the absent case keeps the tag.
                    let value = self.value_stack.pop().unwrap_or(AstNode::Nil);
                    self.value_stack.push(value);
                } else {
                    let cache = if *maybe {
                        &mut self.tag_maybe
                    } else {
                        &mut self.tag_rep
                    };
                    let tag = match *cache {
                        Some(idx) => idx,
                        None => {
                            let name = if *maybe { ":maybe" } else { ":repetition" };
                            let idx = self.arena.intern_string_index(name);
                            *cache = Some(idx);
                            idx
                        }
                    };
                    let start = self.value_stack.len() - n;
                    let (pool_idx, len) = self
                        .arena
                        .store_tagged_array_indexed(tag, &self.value_stack[start..]);
                    self.value_stack.truncate(start);
                    self.value_stack.push(AstNode::Array {
                        pool_index: pool_idx,
                        length: len,
                    });
                }
                Ok(ExecutionResult::Continue)
            }

            Instruction::BuildHash { name_idx } => {
                let idx = *name_idx as usize;
                if idx >= self.name_pool.len() {
                    return Err(ParseError::Internal {
                        message: format!("Invalid string index: {name_idx}"),
                    });
                }
                let value = self.value_stack.pop().ok_or_else(|| ParseError::Internal {
                    message: "BuildHash underflow".to_string(),
                })?;
                let key = match self.name_pool[idx] {
                    Some(key) => key,
                    None => {
                        let name = self.program.get_string(*name_idx).ok_or_else(|| {
                            ParseError::Internal {
                                message: format!("Invalid string index: {name_idx}"),
                            }
                        })?;
                        let key = self.arena.intern_string_index(name);
                        self.name_pool[idx] = Some(key);
                        key
                    }
                };
                let (pool_idx, len) = self.arena.store_hash_indexed_key(key, value);
                self.value_stack.push(AstNode::Hash {
                    pool_index: pool_idx,
                    length: len,
                });
                Ok(ExecutionResult::Continue)
            }

            Instruction::CapMark => {
                self.backtrack_stack
                    .push(self.frame_snapshot(0, FrameKind::Mark));
                Ok(ExecutionResult::Continue)
            }

            Instruction::RecordCapture { name_idx } => {
                if let Some(frame) = self.backtrack_stack.pop() {
                    if frame.kind != FrameKind::Mark {
                        return Err(ParseError::Internal {
                            message: "RecordCapture without CapMark".to_string(),
                        });
                    }
                    let len = self.position.saturating_sub(frame.position) as u32;
                    self.recorded_captures
                        .push((*name_idx, frame.position as u32, len));
                }
                Ok(ExecutionResult::Continue)
            }

            Instruction::ScopeEnd => {
                if let Some(frame) = self.backtrack_stack.pop() {
                    if frame.kind != FrameKind::Mark {
                        return Err(ParseError::Internal {
                            message: "ScopeEnd without CapMark".to_string(),
                        });
                    }
                    // Captures recorded inside the scope are discarded;
                    // the body's value passes through.
                    self.recorded_captures.truncate(frame.caps_height);
                }
                Ok(ExecutionResult::Continue)
            }
        }
    }

    /// Backtrack to the previous choice point
    fn backtrack(&mut self) -> Result<bool, ParseError> {
        self.backtracks += 1;
        if let Some(budget) = self.backtrack_budget {
            if self.backtracks > budget {
                self.budget_exceeded = true;
                return Ok(false);
            }
        }
        loop {
            if self.backtrack_stack.is_empty() {
                return Ok(false);
            }

            if self.backtrack_stack.len() > self.config.max_recursion_depth {
                return Err(ParseError::RecursionLimitExceeded {
                    depth: self.backtrack_stack.len(),
                    max_depth: self.config.max_recursion_depth,
                });
            }

            let frame = self.backtrack_stack.pop().unwrap();

            // Restore state
            self.position = frame.position;
            self.truncate_to(&frame);

            // A failed memoized rule body records its failure — unless
            // the budget tripped, in which case the unwind is an abort,
            // not a parse outcome.
            if frame.kind == FrameKind::Return {
                if let Some(rule_pc) = frame.memo_rule {
                    if !self.budget_exceeded && std::env::var("PARSANOL_VM_NO_FAIL_MEMO").is_err() {
                        self.memo_store(
                            rule_pc,
                            frame.position,
                            MemoEntry {
                                success: false,
                                end_pos: frame.position,
                                value: None,
                            },
                        );
                    }
                }
                continue;
            }

            match frame.kind {
                // These frames have no alternative: continue backtracking.
                // Predicate/mark frames only bound a region whose side
                // effects are already truncated; Return frames had their
                // failure memoized above.
                FrameKind::Return | FrameKind::Predicate | FrameKind::Mark => continue,
                FrameKind::Choice => {
                    if vm_choice_traced() {
                        eprintln!(
                            "parsanol-vm: Choice resume -> ip={} position={} (frame pos {}) depth {}",
                            frame.return_ip,
                            frame.position,
                            self.position,
                            self.backtrack_stack.len()
                        );
                    }
                    self.ip = frame.return_ip;
                    return Ok(true);
                }
            }
        }
    }

    /// Track the furthest failure position for error reporting
    /// Also records what was expected at this position
    #[inline]
    fn track_failure(&mut self) {
        if self.position > self.furthest_failure {
            self.furthest_failure = self.position;
        }
    }

    /// Track a failure with context about what was expected
    #[inline]
    fn track_failure_with_context(&mut self) {
        // Hot path: record (position, ip) only. The Expected label
        // construction (format!/char-set scans) is deferred to
        // `materialize_deferred_failures` — on a successful parse it
        // never runs, and on failure only the furthest event's labels
        // are ever read. The furthest bookkeeping below mirrors
        // `ErrorTracker::record_failure` exactly (including dropping a
        // first failure AT position 0), so the drained events replay
        // through the eager `record_failure` byte-identically.
        // The buffer's furthest is tracked SEPARATELY from
        // `self.furthest_failure`: the contextless `track_failure()` also
        // advances the latter, and the first context event AT that
        // position must still be recorded (this is `record_failure_ip`'s
        // exact bookkeeping, verbatim).
        if self.position >= self.furthest_failure {
            if self.position > self.deferred_failure_furthest {
                self.deferred_failure_furthest = self.position;
                self.deferred_failures.clear();
                self.deferred_failures.push((self.position, self.ip));
            } else if self.position == self.deferred_failure_furthest
                && !self.deferred_failures.is_empty()
            {
                self.deferred_failures.push((self.position, self.ip));
            }
        }
        if self.position > self.furthest_failure {
            self.furthest_failure = self.position;
        }
    }

    /// Materialize the deferred (position, ip) events into the error
    /// tracker through the eager `record_failure` — once per parse, and
    /// only when a consumer actually reads the diagnostics.
    fn materialize_deferred_failures(&mut self) {
        if self.deferred_failures.is_empty() {
            return;
        }
        let events = std::mem::take(&mut self.deferred_failures);
        for (position, ip) in events {
            if let Some(instr) = self.program.get_instruction(ip) {
                let expected = instruction_to_expected(instr, self.program);
                self.error_tracker.record_failure(position, expected, ip);
            }
        }
    }

    /// Build the final result from captures
    fn build_result(&mut self) -> Result<AstNode, ParseError> {
        // Use CaptureProcessor to build proper AST nodes
        let captures = std::mem::take(&mut self.capture_stack);
        let mut processor = CaptureProcessor::new(self.program, self.arena);
        let result = processor.process_flat(&captures, self.position);

        // Restore captures in case they're needed later
        self.capture_stack = captures;

        Ok(result)
    }
}

/// Result of executing an instruction
#[derive(Debug)]
enum ExecutionResult {
    /// Continue to next instruction
    Continue,
    /// Jump to offset
    Jump(i32),
    /// Parse succeeded
    Success(AstNode),
    /// Parse failed, backtrack
    Fail,
}

/// Convenience function to parse with the bytecode VM
pub fn parse_with_vm(
    program: &Program,
    input: &str,
    arena: &mut AstArena,
) -> Result<ParseResult, ParseError> {
    parse_with_vm_diag(program, input, arena).0
}

/// Result plus out-of-band deepest-failure diagnostics.
pub type VmParseWithDiagnostics = (
    Result<ParseResult, ParseError>,
    Option<(usize, Vec<String>)>,
);

/// Parse with a precompiled program, also returning the deepest-failure
/// diagnostics: `(position, expected labels)` at the furthest failure,
/// carried out-of-band like `PortableParser::failure_diagnostics`.
pub fn parse_with_vm_diag(
    program: &Program,
    input: &str,
    arena: &mut AstArena,
) -> VmParseWithDiagnostics {
    let config = VMConfig::default();
    let mut vm = BytecodeVM::new(program, input, arena, config);
    let result = vm.run().map(|result| ParseResult {
        value: result.value,
        end_pos: result.end_pos,
        capture_state: None,
    });
    let diagnostics = if vm.error_tracker_has_failures() {
        vm.materialize_deferred_failures();
        Some(vm.failure_diagnostics())
    } else {
        None
    };
    (result, diagnostics)
}

/// Outcome of a budget-capped VM parse.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)] // Parsed carries the parse result by value on a short-lived path
pub enum VmCappedOutcome {
    /// The parse ran to completion (or a real failure) within budget.
    Parsed {
        /// The parse result.
        result: Result<ParseResult, ParseError>,
        /// Deepest-failure diagnostics, when any failure was tracked.
        diagnostics: Option<(usize, Vec<String>)>,
    },
    /// The backtrack budget tripped: the grammar is backtracking-heavy
    /// and the memoized packrat engine will serve it better.
    BudgetExceeded,
}

/// Parse with a backtrack budget. Grammars whose choice structure makes
/// the (memo-less) VM thrash trip the budget early and fall back to the
/// packrat engine; linear grammars never come close.
pub fn parse_with_vm_capped(
    program: &Program,
    input: &str,
    arena: &mut AstArena,
    max_backtracks: u64,
) -> VmCappedOutcome {
    let config = VMConfig::default();
    let mut vm = BytecodeVM::new(program, input, arena, config);
    vm.backtrack_budget = Some(max_backtracks);
    let result = vm.run().map(|result| ParseResult {
        value: result.value,
        end_pos: result.end_pos,
        capture_state: None,
    });
    if vm.budget_exceeded {
        return VmCappedOutcome::BudgetExceeded;
    }
    let diagnostics = if vm.error_tracker_has_failures() {
        vm.materialize_deferred_failures();
        Some(vm.failure_diagnostics())
    } else {
        None
    };
    if std::env::var("PARSANOL_VM_DEBUG").is_ok() {
        eprintln!(
            "parsanol-vm: input {} bytes, {} backtracks",
            input.len(),
            vm.backtrack_count()
        );
    }
    VmCappedOutcome::Parsed {
        result,
        diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::bytecode::instruction::Instruction;
    use crate::portable::bytecode::program::CharSet;

    fn make_test_program() -> Program {
        Program::new()
    }

    #[test]
    fn test_vm_any_instruction() {
        let mut program = make_test_program();
        program.add_instruction(Instruction::any(1)); // Match 1 char
        program.add_instruction(Instruction::end());
        program.set_entry_point(0);

        let mut arena = AstArena::new();
        let config = VMConfig::default();
        let mut vm = BytecodeVM::new(&program, "hello", &mut arena, config);

        let result = vm.run().unwrap();
        assert_eq!(result.end_pos, 1); // One character consumed
    }

    #[test]
    fn test_vm_char_instruction() {
        let mut program = make_test_program();
        program.add_instruction(Instruction::char(b'h'));
        program.add_instruction(Instruction::end());
        program.set_entry_point(0);

        let mut arena = AstArena::new();
        let config = VMConfig::default();

        // Should succeed
        let mut vm = BytecodeVM::new(&program, "hello", &mut arena, config.clone());
        let result = vm.run().unwrap();
        assert_eq!(result.end_pos, 1);

        // Should fail
        let mut vm = BytecodeVM::new(&program, "world", &mut arena, config);
        assert!(vm.run().is_err());
    }

    #[test]
    fn test_vm_string_instruction() {
        let mut program = make_test_program();
        let str_idx = program.add_string("hello");
        program.add_instruction(Instruction::string(str_idx, 5));
        program.add_instruction(Instruction::end());
        program.set_entry_point(0);

        let mut arena = AstArena::new();
        let config = VMConfig::default();

        // Should succeed
        let mut vm = BytecodeVM::new(&program, "hello world", &mut arena, config.clone());
        let result = vm.run().unwrap();
        assert_eq!(result.end_pos, 5);

        // Should fail
        let mut vm = BytecodeVM::new(&program, "world", &mut arena, config);
        assert!(vm.run().is_err());
    }

    #[test]
    fn test_vm_choice_and_backtrack() {
        let mut program = make_test_program();
        // Try 'a', if fail try 'b'
        program.add_instruction(Instruction::choice(2)); // Choice: skip to ip+3 on failure
        program.add_instruction(Instruction::char(b'a')); // Try 'a'
        program.add_instruction(Instruction::jump(1)); // Success: skip 'b'
        program.add_instruction(Instruction::char(b'b')); // Try 'b'
        program.add_instruction(Instruction::end());
        program.set_entry_point(0);

        let mut arena = AstArena::new();
        let config = VMConfig::default();

        // Should match 'b' via backtracking
        let mut vm = BytecodeVM::new(&program, "b", &mut arena, config);
        let result = vm.run().unwrap();
        assert_eq!(result.end_pos, 1);
    }

    #[test]
    fn test_deferred_diag_recorded_after_contextless_advance() {
        // Regression (the 2026-11-16 deferred-diagnostics round): the
        // contextless `track_failure()` in the Regex-class path advances
        // `furthest_failure`; the FIRST context event AT that furthest
        // must still be recorded even though the deferred buffer is
        // empty. The buggy first rewrite dropped it, so a failing parse
        // reported `failure_diagnostics() == None` and consumers
        // expecting `(position, labels)` panicked (caught by the rs#533
        // corrupted corpus, doc2.insert-20 — not by the crate suite).
        let mut program = make_test_program();
        // input "1z": branch A matches '1' then fails the class at pos 1
        // (contextless advance to furthest 1); the backtrack re-enters at
        // branch B, matches '1', then fails Char '2' AT pos 1 with
        // context — recorded against an empty buffer.
        let regex_idx = program.add_regex("[ab]");
        program.add_instruction(Instruction::Choice { offset: 3 }); // ip 0 -> alt 4
        program.add_instruction(Instruction::char(b'1')); // ip 1
        program.add_instruction(Instruction::regex(regex_idx)); // ip 2
        program.add_instruction(Instruction::Fail); // ip 3 (unreached)
        program.add_instruction(Instruction::char(b'1')); // ip 4
        program.add_instruction(Instruction::char(b'2')); // ip 5
        program.add_instruction(Instruction::end()); // ip 6
        program.set_entry_point(0);

        let mut arena = AstArena::new();
        let mut vm = BytecodeVM::new(&program, "1z", &mut arena, VMConfig::default());
        assert!(vm.run().is_err(), "the parse must fail");
        vm.materialize_deferred_failures();
        let (pos, labels) = vm.failure_diagnostics();
        assert_eq!(pos, 1, "the deepest failure is at the Char '2' position");
        assert_eq!(labels, vec!["2".to_string()]);
    }

    #[test]
    fn test_vm_sequence() {
        let mut program = make_test_program();
        program.add_instruction(Instruction::char(b'a'));
        program.add_instruction(Instruction::char(b'b'));
        program.add_instruction(Instruction::char(b'c'));
        program.add_instruction(Instruction::end());
        program.set_entry_point(0);

        let mut arena = AstArena::new();
        let config = VMConfig::default();

        let mut vm = BytecodeVM::new(&program, "abc", &mut arena, config);
        let result = vm.run().unwrap();
        assert_eq!(result.end_pos, 3);
    }

    #[test]
    fn test_vm_charset_instruction() {
        let mut program = make_test_program();
        let set = CharSet::from_range(b'a', b'z');
        let set_idx = program.add_char_set(set);
        program.add_instruction(Instruction::charset(set_idx));
        program.add_instruction(Instruction::end());
        program.set_entry_point(0);

        let mut arena = AstArena::new();
        let config = VMConfig::default();

        // Should match lowercase letter
        let mut vm = BytecodeVM::new(&program, "hello", &mut arena, config.clone());
        let result = vm.run().unwrap();
        assert_eq!(result.end_pos, 1);

        // Should fail on uppercase
        let mut vm = BytecodeVM::new(&program, "HELLO", &mut arena, config);
        assert!(vm.run().is_err());
    }

    #[test]
    fn test_vm_span_instruction() {
        let mut program = make_test_program();
        let set = CharSet::from_range(b'a', b'z');
        let set_idx = program.add_char_set(set);
        program.add_instruction(Instruction::span(set_idx));
        program.add_instruction(Instruction::end());
        program.set_entry_point(0);

        let mut arena = AstArena::new();
        let config = VMConfig::default();

        let mut vm = BytecodeVM::new(&program, "hello world", &mut arena, config);
        let result = vm.run().unwrap();
        assert_eq!(result.end_pos, 5); // "hello" consumed, space stops
    }

    #[test]
    fn test_vm_jump_instruction() {
        let mut program = make_test_program();
        program.add_instruction(Instruction::jump(1)); // Skip next instruction (IP 0 -> IP 2)
        program.add_instruction(Instruction::char(b'x')); // Skipped
        program.add_instruction(Instruction::char(b'y')); // Execute this
        program.add_instruction(Instruction::end());
        program.set_entry_point(0);

        let mut arena = AstArena::new();
        let config = VMConfig::default();

        // Should match 'y', not 'x'
        let mut vm = BytecodeVM::new(&program, "y", &mut arena, config);
        let result = vm.run().unwrap();
        assert_eq!(result.end_pos, 1);
    }

    #[test]
    fn test_vm_fail_on_invalid_input() {
        let mut program = make_test_program();
        program.add_instruction(Instruction::char(b'a'));
        program.add_instruction(Instruction::end());
        program.set_entry_point(0);

        let mut arena = AstArena::new();
        let config = VMConfig::default();

        let mut vm = BytecodeVM::new(&program, "b", &mut arena, config);
        let result = vm.run();
        assert!(result.is_err());
    }

    #[test]
    fn test_vm_custom_instruction() {
        use crate::portable::custom::{
            register_custom_atom, unregister_custom_atom, CustomAtom, CustomResult,
        };

        // Register a test custom atom that matches "foo"
        struct FooMatcher;
        impl CustomAtom for FooMatcher {
            fn parse(&self, input: &str, pos: usize) -> Option<CustomResult> {
                if input[pos..].starts_with("foo") {
                    Some(CustomResult {
                        end_pos: pos + 3,
                        value: None,
                    })
                } else {
                    None
                }
            }

            fn description(&self) -> &str {
                "foo matcher"
            }
        }

        let id = register_custom_atom(9999, Box::new(FooMatcher));

        let mut program = make_test_program();
        program.add_instruction(Instruction::custom(id));
        program.add_instruction(Instruction::end());
        program.set_entry_point(0);

        let mut arena = AstArena::new();
        let config = VMConfig::default();

        // Should match "foo"
        let mut vm = BytecodeVM::new(&program, "foobar", &mut arena, config.clone());
        let result = vm.run().unwrap();
        assert_eq!(result.end_pos, 3);

        // Should not match "bar"
        let mut arena2 = AstArena::new();
        let mut vm = BytecodeVM::new(&program, "bar", &mut arena2, config);
        let result = vm.run();
        assert!(result.is_err());

        // Clean up
        unregister_custom_atom(id);
    }
}
