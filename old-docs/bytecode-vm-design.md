# Bytecode VM Design for Parsanol

This document analyzes the bytecode VM approach used by JLpeg.jl (based on Roberto Ierusalimschy's LPeg) and how it could be implemented in parsanol-rs.

## Current Architecture vs Bytecode VM

### Current: Packrat Memoization

```
┌─────────────────────────────────────────────────────────────┐
│                    PACKRAT APPROACH                          │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  Grammar ──► Parser ──► Parse ──► Memo Cache ──► AST        │
│                           │                                 │
│                           ▼                                 │
│              ┌─────────────────────────┐                    │
│              │  Dense Cache (AHash)    │                    │
│              │  key: (rule, position)  │                    │
│              │  val: ParseResult       │                    │
│              └─────────────────────────┘                    │
│                                                             │
│  Time:  O(n) guaranteed                                     │
│  Space: O(n × rules) - can be large                         │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### Proposed: Bytecode VM

```
┌─────────────────────────────────────────────────────────────┐
│                    BYTECODE VM APPROACH                      │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  Grammar ──► Compiler ──► Instructions ──► VM ──► Result    │
│                              │              │               │
│                              ▼              ▼               │
│                    ┌──────────────┐  ┌──────────────┐       │
│                    │ IChar 'a'    │  │ Stack        │       │
│                    │ ICall rule_1 │  │ - backtrack  │       │
│                    │ IChoice +10  │  │ - calls      │       │
│                    │ ICommit +5   │  │ - captures   │       │
│                    │ IReturn      │  └──────────────┘       │
│                    └──────────────┘                         │
│                                                             │
│  Time:  O(n) typical, exponential worst case                │
│  Space: O(depth) - only backtrack stack                     │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

## Instruction Set Design

### Core Opcodes

```rust
/// VM instruction opcodes
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opcode {
    // === Matching ===
    /// Match any character, fail at end
    Any,
    /// Match specific character
    Char,
    /// Match character in set (BitSet inline)
    Set,
    /// Match string literal (length-prefixed)
    String,
    /// Match regex pattern (index into pattern table)
    Regex,

    // === Testing (no consume on fail) ===
    /// Test character, jump if no match
    TestChar,
    /// Test set, jump if no match
    TestSet,
    /// Test any char, jump if at end
    TestAny,

    // === Control Flow ===
    /// Unconditional jump
    Jump,
    /// Call rule (push return address)
    Call,
    /// Return from rule
    Return,
    /// End of program (success)
    End,

    // === Backtracking ===
    /// Push choice point (backtrack address)
    Choice,
    /// Pop choice point (commit to this branch)
    Commit,
    /// Update choice position (for loops)
    PartialCommit,
    /// Backtrack but continue (for predicates)
    BackCommit,
    /// Fail - backtrack to last choice
    Fail,
    /// Pop choice then fail
    FailTwice,

    // === Predicates ===
    /// Push predicate choice (different fail behavior)
    PredChoice,

    // === Captures ===
    /// Open capture
    OpenCapture,
    /// Close capture
    CloseCapture,
    /// Full capture (fixed length, no open/close)
    FullCapture,

    // === Error Recovery ===
    /// Throw labeled error
    Throw,
    /// Throw with recovery rule
    ThrowRec,

    // === Advanced ===
    /// Lookbehind (walk back n chars)
    Behind,
    /// No-op (placeholder for optimization)
    NoOp,
}
```

### Instruction Formats

```rust
/// Instruction variants for different operand types
#[derive(Clone, Copy, Debug)]
pub enum Instruction {
    /// Simple instruction (no operands)
    Simple { op: Opcode },

    /// Instruction with jump offset
    Jump { op: Opcode, offset: i32 },

    /// Match any n characters
    Any { n: u32 },

    /// Match single character (UTF-8 encoded inline)
    Char { bytes: [u8; 4], len: u8 },

    /// Character set (indices into set table)
    Set { set_idx: u32 },

    /// String literal (indices into string table)
    String { str_idx: u32, len: u16 },

    /// Choice with backtrack offset
    Choice { offset: i32 },

    /// Capture with kind and optional key
    Capture { kind: CaptureKind, key_idx: u16 },

    /// Throw with label index
    Throw { label_idx: u16 },
}
```

## VM State

```rust
/// VM state for parsing
pub struct VMState<'a> {
    /// The input being parsed
    input: &'a str,

    /// Byte length of input
    input_len: usize,

    /// Current position in input (byte index)
    pos: usize,

    /// Instruction pointer
    ip: usize,

    /// The program (compiled instructions)
    program: &'a [Instruction],

    /// Backtrack stack
    stack: Vec<StackFrame>,

    /// Capture stack
    captures: Vec<CaptureFrame>,

    /// Furthest failure position (for error reporting)
    fail_pos: usize,

    /// Current failure label (for error recovery)
    fail_label: Option<u16>,

    /// Are we inside a predicate?
    in_predicate: bool,
}

/// Stack frame for backtrack/call
#[derive(Clone, Copy)]
pub struct StackFrame {
    /// Return/jump address
    ip: usize,
    /// Position to restore on backtrack
    pos: usize,
    /// Capture stack height
    cap_height: usize,
    /// Predicate flag
    in_predicate: bool,
}

/// Capture frame
#[derive(Clone)]
pub struct CaptureFrame {
    /// Start position
    start: usize,
    /// Capture kind
    kind: CaptureKind,
    /// Optional key index
    key_idx: Option<u16>,
}
```

## Core VM Loop

```rust
impl<'a> VMState<'a> {
    /// Run the VM
    pub fn run(&mut self) -> Result<usize, ParseError> {
        loop {
            let inst = &self.program[self.ip];

            match inst {
                Instruction::Simple { op } => match op {
                    Opcode::End => {
                        return Ok(self.pos);
                    }
                    Opcode::Return => {
                        let frame = self.stack.pop().unwrap();
                        self.ip = frame.ip;
                    }
                    Opcode::Fail => {
                        if !self.backtrack() {
                            return Err(self.make_error());
                        }
                    }
                    Opcode::FailTwice => {
                        self.stack.pop();
                        if !self.backtrack() {
                            return Err(self.make_error());
                        }
                    }
                    _ => unreachable!(),
                },

                Instruction::Any { n } => {
                    if self.pos + *n as usize <= self.input_len {
                        self.pos += *n as usize;
                        self.ip += 1;
                    } else {
                        self.update_fail_pos();
                        if !self.backtrack() {
                            return Err(self.make_error());
                        }
                    }
                }

                Instruction::Char { bytes, len } => {
                    let char_bytes = &bytes[..*len as usize];
                    if self.input.as_bytes()[self.pos..].starts_with(char_bytes) {
                        self.pos += *len as usize;
                        self.ip += 1;
                    } else {
                        self.update_fail_pos();
                        if !self.backtrack() {
                            return Err(self.make_error());
                        }
                    }
                }

                Instruction::Jump { offset, .. } => {
                    self.ip = (self.ip as i32 + offset) as usize;
                }

                Instruction::Call { offset } => {
                    self.stack.push(StackFrame {
                        ip: self.ip + 1,
                        pos: 0, // marker for call frame
                        cap_height: self.captures.len(),
                        in_predicate: self.in_predicate,
                    });
                    self.ip = (self.ip as i32 + offset) as usize;
                }

                Instruction::Choice { offset } => {
                    self.stack.push(StackFrame {
                        ip: (self.ip as i32 + offset) as usize,
                        pos: self.pos,
                        cap_height: self.captures.len(),
                        in_predicate: self.in_predicate,
                    });
                    self.ip += 1;
                }

                Instruction::Commit { offset } => {
                    self.stack.pop();
                    self.ip = (self.ip as i32 + offset) as usize;
                }

                Instruction::PartialCommit { offset } => {
                    // Update top frame position, don't pop
                    let frame = self.stack.last_mut().unwrap();
                    frame.pos = self.pos;
                    frame.cap_height = self.captures.len();
                    self.ip = (self.ip as i32 + offset) as usize;
                }

                // ... more instructions
            }
        }
    }

    /// Backtrack to last choice point
    fn backtrack(&mut self) -> bool {
        loop {
            // Pop until we find a choice point (pos != 0)
            while !self.stack.is_empty() && self.stack.last().unwrap().pos == 0 {
                self.stack.pop();
            }

            if self.stack.is_empty() {
                return false;
            }

            let frame = self.stack.pop().unwrap();
            self.ip = frame.ip;
            self.pos = frame.pos;
            self.captures.truncate(frame.cap_height);
            self.in_predicate = frame.in_predicate;
            return true;
        }
    }
}
```

## Compiler

```rust
/// Compile a grammar to bytecode
pub struct Compiler {
    instructions: Vec<Instruction>,
    strings: Vec<String>,
    sets: Vec<BitSet>,
    labels: HashMap<String, usize>,
}

impl Compiler {
    /// Compile a pattern
    pub fn compile(&mut self, pattern: &Pattern) {
        match pattern {
            Pattern::Char(c) => {
                let bytes = encode_char(*c);
                self.instructions.push(Instruction::Char {
                    bytes,
                    len: bytes.len() as u8,
                });
            }

            Pattern::Seq(patterns) => {
                for p in patterns {
                    self.compile(p);
                }
            }

            Pattern::Choice(alternatives) => {
                if alternatives.len() == 1 {
                    self.compile(&alternatives[0]);
                    return;
                }

                // For each alternative except the last:
                // Choice <offset>
                // <alternative code>
                // Commit <end>

                let choice_offsets: Vec<usize> = alternatives.iter()
                    .take(alternatives.len() - 1)
                    .map(|_| {
                        let ip = self.instructions.len();
                        self.instructions.push(Instruction::Choice { offset: 0 }); // placeholder
                        ip
                    })
                    .collect();

                let commit_offsets: Vec<usize> = alternatives.iter()
                    .take(alternatives.len() - 1)
                    .map(|alt| {
                        self.compile(alt);
                        let ip = self.instructions.len();
                        self.instructions.push(Instruction::Commit { offset: 0 }); // placeholder
                        ip
                    })
                    .collect();

                // Last alternative (no choice/commit needed)
                self.compile(&alternatives[alternatives.len() - 1]);

                let end_ip = self.instructions.len();

                // Patch offsets
                for (i, &choice_ip) in choice_offsets.iter().enumerate() {
                    let alt_start = commit_offsets[i] + 1; // after Commit
                    if let Instruction::Choice { offset } = &mut self.instructions[choice_ip] {
                        *offset = (alt_start as i32) - (choice_ip as i32);
                    }
                }

                for &commit_ip in &commit_offsets {
                    if let Instruction::Commit { offset } = &mut self.instructions[commit_ip] {
                        *offset = (end_ip as i32) - (commit_ip as i32);
                    }
                }
            }

            Pattern::Repeat { pattern, min, max } => {
                // Compile min required repetitions
                for _ in 0..*min {
                    self.compile(pattern);
                }

                match max {
                    None => {
                        // At least min: Choice, pattern, PartialCommit
                        let start_ip = self.instructions.len();
                        self.instructions.push(Instruction::Choice { offset: 0 });
                        self.compile(pattern);
                        let after_pattern = self.instructions.len();

                        // Patch choice
                        if let Instruction::Choice { offset } = &mut self.instructions[start_ip] {
                            *offset = (after_pattern as i32 + 1) - (start_ip as i32);
                        }

                        // PartialCommit loops back
                        let pattern_len = after_pattern - start_ip - 1;
                        self.instructions.push(Instruction::PartialCommit {
                            offset: -(pattern_len as i32 + 1),
                        });
                    }
                    Some(max) => {
                        // Between min and max: optional repetitions
                        for _ in *min..*max {
                            let start_ip = self.instructions.len();
                            self.instructions.push(Instruction::Choice { offset: 0 });
                            self.compile(pattern);
                            let after = self.instructions.len();

                            if let Instruction::Choice { offset } = &mut self.instructions[start_ip] {
                                *offset = (after as i32 + 1) - (start_ip as i32);
                            }
                            self.instructions.push(Instruction::Commit { offset: 1 });
                        }
                    }
                }
            }

            Pattern::Not(pattern) => {
                // PredChoice, pattern, FailTwice
                let start_ip = self.instructions.len();
                self.instructions.push(Instruction::Simple { op: Opcode::PredChoice });
                // Actually need offset...
            }

            // ... more patterns
        }
    }
}
```

## Key Optimizations

### 1. PartialCommit for Loops

Instead of `Choice` + `Commit` for each iteration (pushing new frames):

```
; Naive loop
loop_start:
    Choice loop_end      ; push frame
    <pattern>
    Commit loop_start    ; pop, jump back
loop_end:

; Optimized loop (constant stack space)
    Choice loop_end
loop_start:
    <pattern>
    PartialCommit loop_start  ; update frame, jump back
loop_end:
```

### 2. Tail Call Optimization

```rust
// Before: Call + Return
ICall rule_x
IReturn

// After: Jump (tail call)
IJump rule_x
```

### 3. Peephole Optimization

```rust
/// Peephole optimizer
pub fn peephole(instructions: &mut [Instruction]) {
    let mut i = 0;
    while i < instructions.len() {
        match &instructions[i] {
            // Jump to Jump -> Jump to final target
            Instruction::Jump { offset } => {
                let target = (i as i32 + offset) as usize;
                if let Instruction::Jump { offset: offset2 } = &instructions[target] {
                    let final_target = (target as i32 + offset2) as usize;
                    instructions[i] = Instruction::Jump {
                        offset: (final_target as i32) - (i as i32),
                    };
                }
            }

            // Jump to Return -> Return
            Instruction::Jump { offset } => {
                let target = (i as i32 + offset) as usize;
                if matches!(instructions[target], Instruction::Simple { op: Opcode::Return }) {
                    instructions[i] = Instruction::Simple { op: Opcode::Return };
                }
            }

            // Jump to Fail -> Fail
            Instruction::Jump { offset } => {
                let target = (i as i32 + offset) as usize;
                if matches!(instructions[target], Instruction::Simple { op: Opcode::Fail }) {
                    instructions[i] = Instruction::Simple { op: Opcode::Fail };
                }
            }

            _ => {}
        }
        i += 1;
    }
}
```

### 4. Fixed-Length Capture Optimization

```rust
// If pattern has fixed length, use FullCapture instead of Open/Close
if let Some(len) = fixed_length(pattern) {
    self.compile(pattern);
    self.instructions.push(Instruction::Capture {
        kind: CaptureKind::Full,
        length: len,
    });
} else {
    self.instructions.push(Instruction::Capture { kind: CaptureKind::Open, .. });
    self.compile(pattern);
    self.instructions.push(Instruction::Capture { kind: CaptureKind::Close, .. });
}
```

## Error Recovery with Labeled Throws

```rust
// Grammar with error labels
let grammar = grammar! {
    :expr <- :term (("+" / "-") :term)* %expected_operator
    :term <- :factor (("*" / "/") :factor)*
    :factor <- "(" :expr ")" / number %expected_number
};

// On failure, throw label and optionally call recovery rule
// IThrow label_idx
// or
// IThrowRec label_idx, recovery_rule_offset
```

## Comparison: Packrat vs Bytecode VM

| Aspect | Packrat (Current) | Bytecode VM |
|--------|------------------|-------------|
| Time Complexity | O(n) guaranteed | O(n) typical, O(2^n) worst |
| Space Complexity | O(n × rules) | O(depth) |
| Memory Pressure | High (cache) | Low (stack only) |
| Cache Locality | Moderate | Excellent |
| Error Recovery | Complex | Built-in (throws) |
| Optimization | Limited | Many (peephole, tail-call, etc.) |
| Compilation | N/A | Required |
| Debugging | Hard | Easy (trace instructions) |

## Hybrid Approach (Recommended)

We could implement both and let users choose:

```rust
pub enum ParserBackend {
    /// Packrat memoization (current approach)
    Packrat,
    /// Bytecode VM (new approach)
    Bytecode,
    /// Auto-select based on grammar analysis
    Auto,
}

impl Grammar {
    pub fn parse(&self, input: &str, backend: ParserBackend) -> Result<Ast, Error> {
        match backend {
            ParserBackend::Packrat => self.parse_packrat(input),
            ParserBackend::Bytecode => self.parse_bytecode(input),
            ParserBackend::Auto => {
                // Hard rule: nested repetitions → Packrat (prevents O(2^n))
                // Otherwise → Bytecode (lower memory)
                if self.has_nested_repetition() {
                    self.parse_packrat(input)
                } else {
                    self.parse_bytecode(input)
                }
            }
        }
    }
}
```

## Implementation Plan

1. **Phase 1: Core VM**
   - Define instruction set
   - Implement VM state and execution loop
   - Basic matching instructions

2. **Phase 2: Compiler**
   - Compile patterns to instructions
   - Handle choices and sequences
   - Implement loops with PartialCommit

3. **Phase 3: Optimizations**
   - Peephole optimizer
   - Tail call elimination
   - Fixed-length captures

4. **Phase 4: Advanced Features**
   - Captures
   - Error recovery with throws
   - Predicates (lookahead)

5. **Phase 5: Integration**
   - Hybrid mode with packrat
   - Benchmarking
   - Documentation

## References

- [LPeg: A Parsing Expression Grammar for Lua](https://www.inf.puc-rio.br/~roberto/docs/peg.pdf) - Roberto Ierusalimschy
- [JLpeg.jl](https://github.com/mnemnion/JLpeg.jl) - Julia implementation
- [lpeglabel](https://github.com/sqmedeiros/lpeglabel) - LPeg with labeled failures
