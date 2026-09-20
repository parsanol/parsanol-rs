//! Bytecode VM execution engine
//!
//! This module implements the virtual machine that executes compiled bytecode.
//! The VM uses a stack-based backtracking approach inspired by LPeg.

use super::capture::{CaptureFrame, CaptureProcessor};
use super::error::{instruction_to_expected, ErrorTracker};
use super::instruction::Instruction;
use super::program::Program;
use crate::portable::arena::AstArena;
use crate::portable::ast::{AstNode, ParseError, ParseResult};
use crate::portable::capture_state::CaptureState;
use crate::portable::char_class::utf8_char_len;
use crate::portable::regex_cache;
use std::time::Instant;

/// Maximum capture stack depth
const MAX_CAPTURE_DEPTH: usize = 100_000;

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

    /// Capture state for generational scopes
    capture_state: CaptureState,

    /// Tree-walker value model: produced values (one per completed atom)
    value_stack: Vec<AstNode>,

    /// Repetition registers: iteration counts anchoring BuildRep
    rep_registers: Vec<u32>,

    /// Recorded captures: (name table index, start, length)
    recorded_captures: Vec<(u32, u32, u32)>,

    /// Backtrack count (diagnostics/budgeting)
    backtracks: u64,

    /// Rule-call memoization: (rule body pc, position) -> outcome.
    /// PEG parse results are context-free at a call boundary (Dynamic
    /// atoms are gated out of VM programs), so successes and failures
    /// alike are memoizable.
    memo: std::collections::HashMap<(usize, usize), MemoEntry>,

    /// Budget for parse_with_vm_capped; None = unlimited.
    backtrack_budget: Option<u64>,

    /// Set when the budget tripped.
    budget_exceeded: bool,
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
            capture_state: CaptureState::new(),
            value_stack: Vec::with_capacity(64),
            rep_registers: Vec::with_capacity(16),
            recorded_captures: Vec::new(),
            backtracks: 0,
            backtrack_budget: None,
            budget_exceeded: false,
            memo: std::collections::HashMap::new(),
        }
    }

    /// Number of backtracks performed during the run.
    pub fn backtrack_count(&self) -> u64 {
        self.backtracks
    }

    /// Whether any terminal failure was tracked during the run.
    pub fn error_tracker_has_failures(&self) -> bool {
        self.error_tracker.has_failures()
    }

    /// Deepest terminal failure: the furthest position and its expected
    /// labels (see `parse_with_vm_diag`).
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
        loop {
            // Check timeout
            if self.config.timeout_ms > 0 {
                let elapsed = self.start_time.elapsed().as_millis() as u64;
                if elapsed > self.config.timeout_ms {
                    return Err(ParseError::TimeoutExceeded {
                        elapsed_ms: elapsed,
                        timeout_ms: self.config.timeout_ms,
                    });
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
            let instr = match self.program.get_instruction(self.ip) {
                Some(i) => i.clone(),
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
            match self.execute_instruction(&instr)? {
                ExecutionResult::Continue => {
                    self.ip += 1;
                }
                ExecutionResult::Jump(offset) => {
                    self.ip = (self.ip as isize + 1 + offset as isize) as usize;
                }
                ExecutionResult::Success(value) => {
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
    #[inline]
    fn memo_store(&mut self, rule_pc: usize, pos: usize, entry: MemoEntry) {
        const MEMO_MAX: usize = 2_000_000;
        if self.memo.len() < MEMO_MAX {
            self.memo.insert((rule_pc, pos), entry);
        }
    }

    fn execute_instruction(&mut self, instr: &Instruction) -> Result<ExecutionResult, ParseError> {
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

                // Use the regex cache
                let re =
                    regex_cache::get_or_compile(pattern).ok_or_else(|| ParseError::Internal {
                        message: format!("Failed to compile regex: {}", pattern),
                    })?;

                // The tree-walker rejects regexes at end-of-input (even
                // zero-width-matchable ones) — parity requires the same.
                if self.position >= self.input.len() {
                    self.track_failure_with_context();
                    return Ok(ExecutionResult::Fail);
                }

                // Match at current position
                if let Some(m) = re.find_at(self.input_str, self.position) {
                    if m.start() == self.position {
                        let start = self.position;
                        self.position = m.end();
                        self.push_span_value(start);
                        return Ok(ExecutionResult::Continue);
                    }
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

                // Packrat memoization: rule outcomes are context-free at
                // a call boundary (Dynamic atoms are gated out of VM
                // programs), so successes and failures alike reuse.
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
                let set =
                    self.program
                        .get_char_set(*set_idx)
                        .ok_or_else(|| ParseError::Internal {
                            message: format!("Invalid charset index: {}", set_idx),
                        })?;

                // Match zero or more characters from the set
                let start = self.position;
                while self.position < self.input.len() && set.contains(self.input[self.position]) {
                    let char_len = utf8_char_len(self.input[self.position]);
                    self.position += char_len;
                }

                self.push_span_value(start);
                Ok(ExecutionResult::Continue)
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
                self.capture_state.push_scope();
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

                let _guard = match enter_dynamic() {
                    Some(g) => g,
                    None => {
                        self.track_failure();
                        return Ok(ExecutionResult::Fail);
                    }
                };

                // Create context for callback
                let ctx =
                    DynamicContext::new(self.input_str, self.position, self.capture_state.clone());

                // Invoke callback: a fragment grammar (self-consistent
                // atom indices, the shape host bridges produce) wins
                // over a bare atom, which is appended to a fresh
                // grammar. Mirrors the packrat engine's parse_dynamic.
                let (temp_grammar, _temp_atom_id) =
                    match with_dynamic_callback(*callback_id, |cb| {
                        if let Some((fragment, root)) = cb.resolve_fragment(&ctx) {
                            return Some((fragment, root));
                        }
                        cb.resolve(&ctx).map(|atom| {
                            let mut g = Grammar::new();
                            let id = g.add_atom(atom);
                            g.root = id;
                            (g, id)
                        })
                    }) {
                        Some(r) => r,
                        None => {
                            self.track_failure();
                            return Ok(ExecutionResult::Fail);
                        }
                    };

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
                        drop(_guard);
                        Ok(ExecutionResult::Continue)
                    }
                    Err(_) => {
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
                let items = self.value_stack.split_off(self.value_stack.len() - n);
                let (pool_idx, len) = self.arena.store_tagged_array(":sequence", &items);
                self.value_stack.push(AstNode::Array {
                    pool_index: pool_idx,
                    length: len,
                });
                Ok(ExecutionResult::Continue)
            }

            Instruction::RepOpen => {
                self.rep_registers.push(0);
                Ok(ExecutionResult::Continue)
            }

            Instruction::RepCount => {
                if let Some(count) = self.rep_registers.last_mut() {
                    *count += 1;
                }
                Ok(ExecutionResult::Continue)
            }

            Instruction::BuildRep { maybe } => {
                let n = self.rep_registers.pop().unwrap_or(0) as usize;
                if self.value_stack.len() < n {
                    return Err(ParseError::Internal {
                        message: format!("BuildRep underflow: need {n} values"),
                    });
                }
                let mut items = self.value_stack.split_off(self.value_stack.len() - n);
                if *maybe && items.len() == 1 {
                    // A present optional flattens to its value in every
                    // context; only the absent case keeps the tag.
                    let value = items.pop().unwrap_or(AstNode::Nil);
                    self.value_stack.push(value);
                } else {
                    let tag = if *maybe { ":maybe" } else { ":repetition" };
                    let (pool_idx, len) = self.arena.store_tagged_array(tag, &items);
                    self.value_stack.push(AstNode::Array {
                        pool_index: pool_idx,
                        length: len,
                    });
                }
                Ok(ExecutionResult::Continue)
            }

            Instruction::BuildHash { name_idx } => {
                let name =
                    self.program
                        .get_string(*name_idx)
                        .ok_or_else(|| ParseError::Internal {
                            message: format!("Invalid string index: {name_idx}"),
                        })?;
                let value = self.value_stack.pop().ok_or_else(|| ParseError::Internal {
                    message: "BuildHash underflow".to_string(),
                })?;
                let (pool_idx, len) = self.arena.store_hash(&[(name, value)]);
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
                    if !self.budget_exceeded {
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
        if self.position >= self.furthest_failure {
            // Get the current instruction for context
            if let Some(instr) = self.program.get_instruction(self.ip) {
                let expected = instruction_to_expected(instr, self.program);
                self.error_tracker
                    .record_failure(self.position, expected, self.ip);
            }
        }
        if self.position > self.furthest_failure {
            self.furthest_failure = self.position;
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
        Some(vm.failure_diagnostics())
    } else {
        None
    };
    (result, diagnostics)
}

/// Outcome of a budget-capped VM parse.
#[derive(Debug)]
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
