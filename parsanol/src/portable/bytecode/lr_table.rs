//! The LR fusion (rs#574 Phase 2, option B): the recursive-spine table.
//!
//! ONE LR(1) automaton drives a rule graph the fixpoint prover
//! accepts — the machinery the byte-driven DFA backend architecturally
//! lacks (recursion). The wire contract: the machine emits THE SAME
//! trees as the classic compile (the differential battery pins it).
//!
//! Model: states × 256 byte columns (the ACTION row; classes come
//! free — every byte in a class gets the same action), productions =
//! the atom tree flattened (sequences chain, alternatives split,
//! Named/Entity = nonterminal refs, Str = byte terminals, Re =
//! class terminals, repetitions = right-recursive auxiliary rules),
//! conflicts refuse (the classic keeps the rule).
//!
//! Slice 1: the table type + the LR(1) item-set builder. The
//! `LrRun` executor = slice 2.

use crate::portable::char_class::CharRanges;
use crate::portable::grammar::{Atom, Grammar, RepetitionTag};

/// One LR parse action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LrAction {
    /// Consume the byte, enter the state.
    Shift(u32),
    /// Reduce by production `p` (produce its value, pop its symbols,
    /// goto on the production's nonterminal).
    Reduce(u32),
    /// The entry rule's production completed: accept.
    Accept,
}

/// The LR(1) parse table for one prover-accepted rule graph.
#[derive(Debug, Clone, Default)]
pub struct LrTable {
    /// `state * 256 + byte` → the index into [`LrTable::actions`]
    /// (or `u32::MAX` when the byte is an error at that state).
    pub action_index: Vec<u32>,
    /// The deduplicated actions.
    pub actions: Vec<LrAction>,
    /// `goto[state * nonterminal_count + nonterminal]` → next state
    /// (or `u32::MAX`).
    pub goto: Vec<u32>,
    /// The nonterminal count (Named/Entity rules + the repetition
    /// auxiliaries).
    pub nonterminal_count: u32,
    /// The productions: (nonterminal, the symbol list). Symbols:
    /// terminal bytes/classes inlined in the item sets; a production's
    /// rhs = the atom-tree lowering's symbol ids.
    pub productions: Vec<LrProduction>,
    /// The Named production kinds intern their rule names here
    /// (`ProductionKind::Named` indexes this table).
    pub names: Vec<String>,
    /// The start state.
    /// The start state.
    pub start: u32,
    /// The item-set (state) count — the ACTION/GOTO rows.
    pub state_count: u32,
}

/// One production: the nonterminal it reduces to and its rhs symbol
/// ids. Terminal symbols inlined; nonterminal symbols = `NonTerminal`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LrProduction {
    /// The reducing nonterminal.
    pub lhs: u32,
    /// The rhs symbols (a `Byte`/`Class` match consumes bytes in the
    /// ACTION tables; a `NonTerminal` takes the GOTO).
    pub rhs: Vec<Symbol>,
    /// The value shape at reduce: the v2 named law's per-production
    /// kind (Named wraps, anonymous sequences, pure spans).
    pub kind: ProductionKind,
}

/// One grammar symbol in a production's rhs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Symbol {
    /// One exact byte.
    Byte(u8),
    /// Any byte in the ASCII class bitmap (members above 0x7F ride
    /// the later multi-byte slice — the stage-1 law).
    Class(Box<[bool; 256]>),
    /// A nonterminal reference (the goto).
    NonTerminal(u32),
}

/// The value shape a reduce produces (the v2 named law's kinds).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProductionKind {
    /// `{name: inner}` — the v2 named law.
    Named(u32),
    /// The children's values as one `:sequence` node.
    Sequence,
    /// The single child's value passes through.
    Pass,
    /// A pure-terminal production: the consumed byte span.
    Span,
    /// The tagged empty (an empty production).
    Empty,
    /// The repetition envelope fold: the aux chain's `body aux` /
    /// empty productions fold into ONE flat `:repetition` node (the
    /// classic's envelope), not a right-nested chain.
    Repetition,
}

/// The item: production `p`, the dot at `dot`, the lookahead byte
/// class bitmap (canonical LR(1); classes keep the set compact).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Item {
    p: u32,
    dot: usize,
    look: Box<[bool; 256]>,
}

/// The builder's error: the grammar shape the LR construction refuses
/// (conflicts, unsupported atoms) — the classic keeps the rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LrBuildError(pub String);

/// Lower one rule graph (the entry rule's atom tree, the other
/// rules reachable through Named/Entity refs) into productions and
/// build the canonical LR(1) table. Refuses anything the byte-level
/// model can't express — the classic keeps the rule.
pub fn build_lr_table(grammar: &Grammar, entry: usize) -> Result<LrTable, LrBuildError> {
    let mut lower = Lowering::default();
    let entry_nt = lower.lower_rule(grammar, entry, 0)?;
    let productions = lower.productions;
    let names = lower.names;
    let nt_count = lower.nonterminal_count as u32;
    let mut table = lr1_construct(&productions, nt_count, entry_nt)?;
    table.names = names;
    Ok(table)
}

/// The atom-tree lowering: every reachable rule becomes a nonterminal
/// with productions; the shared atoms dedupe by id.
#[derive(Default)]
struct Lowering {
    productions: Vec<LrProduction>,
    /// The rule (or auxiliary) atom id → the nonterminal id.
    rule_nt: std::collections::HashMap<usize, u32>,
    nonterminal_count: usize,
    /// The Named rule names, internd in production order.
    names: Vec<String>,
    /// The pending rhs symbol lists per in-progress rule (recursion).
    _pending: std::collections::HashSet<usize>,
}

impl Lowering {
    fn new_nt(&mut self) -> u32 {
        let nt = self.nonterminal_count as u32;
        self.nonterminal_count += 1;
        nt
    }

    /// Lower the rule `atom_idx` to a nonterminal id (deduped).
    fn lower_rule(
        &mut self,
        grammar: &Grammar,
        atom_idx: usize,
        depth: u8,
    ) -> Result<u32, LrBuildError> {
        if depth > 64 {
            return Err(LrBuildError("depth".into()));
        }
        if let Some(&nt) = self.rule_nt.get(&atom_idx) {
            return Ok(nt);
        }
        let nt = self.new_nt();
        self.rule_nt.insert(atom_idx, nt);
        let prods = self.lower_body(grammar, atom_idx, depth + 1)?;
        for (rhs, kind) in prods {
            self.productions.push(LrProduction { lhs: nt, rhs, kind });
        }
        Ok(nt)
    }

    /// Lower one body atom to a production list for its parent
    /// nonterminal.
    fn lower_body(
        &mut self,
        grammar: &Grammar,
        idx: usize,
        depth: u8,
    ) -> Result<Vec<(Vec<Symbol>, ProductionKind)>, LrBuildError> {
        if depth > 64 {
            return Err(LrBuildError("depth".into()));
        }
        match grammar.get_atom(idx) {
            Some(Atom::Named { atom, name, .. }) => {
                // The named rule: its own nonterminal; the productions'
                // kind = Named (the name interns at reduce).
                let name_id = self.intern_name(name);
                let inner = self.lower_rule(grammar, *atom, depth + 1)?;
                Ok(vec![(
                    vec![Symbol::NonTerminal(inner)],
                    ProductionKind::Named(name_id),
                )])
            }
            Some(Atom::Entity { atom, .. }) => {
                let inner = self.lower_rule(grammar, *atom, depth + 1)?;
                Ok(vec![(
                    vec![Symbol::NonTerminal(inner)],
                    ProductionKind::Pass,
                )])
            }
            Some(Atom::Sequence { atoms }) if !atoms.is_empty() => {
                let mut rhs = Vec::with_capacity(atoms.len());
                for &child in atoms {
                    rhs.extend(self.lower_symbol(grammar, child, depth + 1)?);
                }
                Ok(vec![(rhs, ProductionKind::Sequence)])
            }
            Some(Atom::Alternative { atoms }) if !atoms.is_empty() => {
                let mut prods = Vec::with_capacity(atoms.len());
                for &child in atoms {
                    prods.extend(self.lower_body(grammar, child, depth + 1)?);
                }
                Ok(prods)
            }
            Some(Atom::Repetition {
                atom,
                min,
                max,
                tag,
            }) => {
                // Repetitions lower to a right-recursive auxiliary:
                // X → body X | εtail. min copies unroll; max=None
                // recurses; tag Maybe = the optional form.
                if tag != &RepetitionTag::Repetition && tag != &RepetitionTag::Maybe {
                    return Err(LrBuildError("tag".into()));
                }
                if max.is_some_and(|m| m.saturating_sub(*min) > 8) {
                    return Err(LrBuildError("unroll".into()));
                }
                // The class-run law (the classic's merged Span run): a
                // `*`-form repetition over a SINGLE terminal body
                // lowers to the left-recursive merge — the whole run
                // collapses into ONE span value, byte-identical to
                // the classic's class-run shape.
                if *min == 0 && max.is_none() && matches!(tag, RepetitionTag::Repetition) {
                    if let Ok(mut syms) = self.lower_symbol(grammar, *atom, depth + 1) {
                        if syms.len() == 1 {
                            let sym = syms.pop().expect("one symbol");
                            let run_nt = self.new_nt();
                            self.rule_nt.insert(idx, run_nt);
                            self.productions.push(LrProduction {
                                lhs: run_nt,
                                rhs: vec![Symbol::NonTerminal(run_nt), sym],
                                kind: ProductionKind::Span,
                            });
                            self.productions.push(LrProduction {
                                lhs: run_nt,
                                rhs: Vec::new(),
                                kind: ProductionKind::Repetition,
                            });
                            return Ok(vec![(
                                vec![Symbol::NonTerminal(run_nt)],
                                ProductionKind::Pass,
                            )]);
                        }
                    }
                }
                let body = self.lower_fragment(grammar, *atom, depth + 1)?;
                let mut prods = Vec::new();
                for _ in 0..*min {
                    prods.push((body.clone(), ProductionKind::Pass));
                }
                match (max, tag) {
                    (None, _) => {
                        let aux = self.new_nt();
                        self.rule_nt.insert(idx, aux);
                        let mut rhs = body.clone();
                        rhs.push(Symbol::NonTerminal(aux));
                        self.productions.push(LrProduction {
                            lhs: aux,
                            rhs,
                            kind: ProductionKind::Repetition,
                        });
                        self.productions.push(LrProduction {
                            lhs: aux,
                            rhs: Vec::new(),
                            kind: ProductionKind::Repetition,
                        });
                        if *min == 0 {
                            prods.push((vec![Symbol::NonTerminal(aux)], ProductionKind::Pass));
                        }
                    }
                    (Some(m), _) => {
                        for _ in *min..*m {
                            prods.push((body.clone(), ProductionKind::Pass));
                        }
                    }
                }
                Ok(prods)
            }
            _ => self.lower_terminal_prods(grammar, idx),
        }
    }

    /// The terminal productions for a bare leaf (the Span kind).
    fn lower_terminal_prods(
        &mut self,
        grammar: &Grammar,
        idx: usize,
    ) -> Result<Vec<(Vec<Symbol>, ProductionKind)>, LrBuildError> {
        match grammar.get_atom(idx) {
            Some(Atom::Str { pattern }) if !pattern.is_empty() => {
                let rhs = pattern
                    .as_bytes()
                    .iter()
                    .map(|&b| Symbol::Byte(b))
                    .collect();
                Ok(vec![(rhs, ProductionKind::Span)])
            }
            Some(Atom::Re { pattern }) => {
                let ranges = CharRanges::from_pattern(pattern)
                    .ok_or_else(|| LrBuildError("class".into()))?;
                let mut bits = [false; 256];
                for (lo, hi) in ranges.codepoint_ranges() {
                    if lo > 0x7F {
                        return Err(LrBuildError("multibyte".into()));
                    }
                    for cp in lo..=hi.min(0x7F) {
                        bits[cp as usize] = true;
                    }
                }
                Ok(vec![(
                    vec![Symbol::Class(Box::new(bits))],
                    ProductionKind::Span,
                )])
            }
            _ => Err(LrBuildError("atom".into())),
        }
    }

    /// Lower an EMBEDDED atom (inside a sequence) to rhs symbols:
    /// nested fragments become auxiliary nonterminals so the
    /// item-set machinery stays production-shaped.
    fn lower_symbol(
        &mut self,
        grammar: &Grammar,
        idx: usize,
        depth: u8,
    ) -> Result<Vec<Symbol>, LrBuildError> {
        match grammar.get_atom(idx) {
            Some(Atom::Str { pattern }) if pattern.len() == 1 => {
                Ok(vec![Symbol::Byte(pattern.as_bytes()[0])])
            }
            Some(Atom::Str { pattern }) if !pattern.is_empty() => {
                let mut out = Vec::with_capacity(pattern.len());
                for &b in pattern.as_bytes() {
                    out.push(Symbol::Byte(b));
                }
                Ok(out)
            }
            Some(Atom::Re { pattern }) => {
                let ranges = CharRanges::from_pattern(pattern)
                    .ok_or_else(|| LrBuildError("class".into()))?;
                let mut bits = [false; 256];
                for (lo, hi) in ranges.codepoint_ranges() {
                    if lo > 0x7F {
                        return Err(LrBuildError("multibyte".into()));
                    }
                    for cp in lo..=hi.min(0x7F) {
                        bits[cp as usize] = true;
                    }
                }
                Ok(vec![Symbol::Class(Box::new(bits))])
            }
            Some(Atom::Named { .. }) | Some(Atom::Entity { .. }) => {
                let nt = self.lower_rule(grammar, idx, depth)?;
                Ok(vec![Symbol::NonTerminal(nt)])
            }
            _ => {
                // A nested fragment (a sequence inside a sequence, an
                // embedded repetition): its own auxiliary nonterminal.
                let nt = self.lower_rule(grammar, idx, depth)?;
                Ok(vec![Symbol::NonTerminal(nt)])
            }
        }
    }

    /// Lower a repetition BODY as one fragment (a symbol list) for
    /// the auxiliary chain.
    fn lower_fragment(
        &mut self,
        grammar: &Grammar,
        idx: usize,
        depth: u8,
    ) -> Result<Vec<Symbol>, LrBuildError> {
        self.lower_symbol(grammar, idx, depth)
    }

    fn intern_name(&mut self, name: &str) -> u32 {
        let id = self.names.len() as u32;
        self.names.push(name.to_string());
        id
    }
}

/// The canonical LR(1) construction over byte terminals: the item
/// sets, the ACTION/GOTO tables, the conflict refusal.
fn lr1_construct(
    productions: &[LrProduction],
    nonterminal_count: u32,
    entry_nt: u32,
) -> Result<LrTable, LrBuildError> {
    // The augmented start production: S' → entry, EOF-lookahead.
    let mut prods = productions.to_vec();
    let accept_prod = prods.len();
    prods.push(LrProduction {
        lhs: nonterminal_count,
        rhs: vec![Symbol::NonTerminal(entry_nt)],
        kind: ProductionKind::Pass,
    });

    // FIRST sets over the nonterminals (nullable-aware, byte bitmaps).
    let mut first: Vec<[bool; 256]> = vec![[false; 256]; nonterminal_count as usize + 1];
    let mut nullable: Vec<bool> = vec![false; nonterminal_count as usize + 1];
    let mut changed = true;
    let mut rounds = 0;
    while changed {
        changed = false;
        rounds += 1;
        if rounds > 4096 {
            return Err(LrBuildError("first-fixpoint".into()));
        }
        for p in &prods {
            let lhs = p.lhs as usize;
            let mut all_nullable = true;
            for sym in &p.rhs {
                match sym {
                    Symbol::Byte(b) => {
                        if !first[lhs][*b as usize] {
                            first[lhs][*b as usize] = true;
                            changed = true;
                        }
                        all_nullable = false;
                    }
                    Symbol::Class(bits) => {
                        for (b, &on) in bits.iter().enumerate() {
                            if on && !first[lhs][b] {
                                first[lhs][b] = true;
                                changed = true;
                            }
                        }
                        all_nullable = false;
                    }
                    Symbol::NonTerminal(nt) => {
                        let s = *nt as usize;
                        let row = first[s];
                        for (b, &on) in row.iter().enumerate() {
                            if on && !first[lhs][b] {
                                first[lhs][b] = true;
                                changed = true;
                            }
                        }
                        if !nullable[s] {
                            all_nullable = false;
                        }
                    }
                }
                if !all_nullable {
                    break;
                }
            }
            if all_nullable && !nullable[lhs] {
                nullable[lhs] = true;
                changed = true;
            }
        }
    }

    // The item sets: canonical LR(1) with byte-class lookaheads.
    // States = deduplicated closure sets; the edges = per symbol.
    let mut start_look = [false; 256];
    start_look[0] = true; // the EOF-ish sentinel byte 0 (never in the input's members)
    let start_state_items = vec![Item {
        p: accept_prod as u32,
        dot: 0,
        look: Box::new(start_look),
    }];

    let mut states: Vec<Vec<Item>> = Vec::new();
    let mut action_index: Vec<u32> = Vec::new();
    let mut actions: Vec<LrAction> = Vec::new();
    let mut goto: Vec<u32> = Vec::new();

    let closure = |items: &[Item],
                   prods: &[LrProduction],
                   first: &[[bool; 256]],
                   nullable: &[bool]|
     -> Vec<Item> {
        let mut set: Vec<Item> = items.to_vec();
        let mut i = 0;
        while i < set.len() {
            let item = &set[i];
            let p = &prods[item.p as usize];
            let Some(sym_nt) = p.rhs.get(item.dot) else {
                i += 1;
                continue;
            };
            let Symbol::NonTerminal(nt) = sym_nt else {
                i += 1;
                continue;
            };
            // The lookahead: beta FIRST (the after-dot rest), or
            // the item's own look when beta is nullable/empty.
            let mut la = [false; 256];
            let mut beta_nullable = true;
            for sym in &p.rhs[item.dot + 1..] {
                match sym {
                    Symbol::Byte(b) => {
                        la[*b as usize] = true;
                        beta_nullable = false;
                    }
                    Symbol::Class(bits) => {
                        for (b, &on) in bits.iter().enumerate() {
                            if on {
                                la[b] = true;
                            }
                        }
                        beta_nullable = false;
                    }
                    Symbol::NonTerminal(t) => {
                        let ts = *t as usize;
                        for b in 0..256 {
                            if first[ts][b] {
                                la[b] = true;
                            }
                        }
                        if !nullable[ts] {
                            beta_nullable = false;
                        }
                    }
                }
                if !beta_nullable {
                    break;
                }
            }
            if beta_nullable {
                for (b, &on) in item.look.iter().enumerate() {
                    if on {
                        la[b] = true;
                    }
                }
            }
            for (pi, prod) in prods.iter().enumerate() {
                // Empty productions included: their dot-0 item is
                // already complete, and the ε reduce needs it in the
                // state (the standard construction).
                if prod.lhs == *nt {
                    let cand = Item {
                        p: pi as u32,
                        dot: 0,
                        look: Box::new(la),
                    };
                    if !set
                        .iter()
                        .any(|it| it.p == cand.p && it.dot == 0 && it.look == cand.look)
                    {
                        set.push(cand);
                    }
                }
            }
            i += 1;
        }
        set
    };

    // Symbol interning: distinct Byte/Class symbols get distinct
    // edge ids (the nonterminals keep their own ids). Two states'
    // identical symbols share the id, so the state dedupe works.
    let mut sym_ids: std::collections::HashMap<Vec<u8>, usize> = std::collections::HashMap::new();
    let mut symbols: Vec<Symbol> = Vec::new();
    let symbol_id = |sym: &Symbol,
                     symbols: &mut Vec<Symbol>,
                     ids: &mut std::collections::HashMap<Vec<u8>, usize>,
                     _nt_count: u32|
     -> usize {
        // EVERY symbol kind interns uniformly (the edge walk reads
        // symbols[id] back); the kind byte keeps Byte/Class/NonTerminal
        // keys disjoint.
        let key: Vec<u8> = match sym {
            Symbol::NonTerminal(nt) => {
                let mut k = vec![2u8];
                k.extend(nt.to_le_bytes());
                k
            }
            Symbol::Byte(b) => vec![0u8, *b],
            Symbol::Class(bits) => {
                let mut k = vec![1u8];
                k.extend(bits.iter().map(|&b| b as u8));
                k
            }
        };
        if let Some(&id) = ids.get(&key) {
            return id;
        }
        let id = symbols.len();
        symbols.push(sym.clone());
        ids.insert(key, id);
        id
    };

    // Seed state 0.
    let seed = closure(&start_state_items, &prods, &first, &nullable);
    states.push(seed);
    action_index.extend(std::iter::repeat_n(u32::MAX, 256));
    goto.extend(std::iter::repeat_n(
        u32::MAX,
        nonterminal_count as usize + 1,
    ));
    let mut worklist = 0usize;
    while worklist < states.len() {
        let set = states[worklist].clone();
        worklist += 1;
        // The edge map: the after-dot symbol id → the advanced items.
        let mut edges: std::collections::BTreeMap<usize, Vec<Item>> =
            std::collections::BTreeMap::new();
        for item in &set {
            let p = &prods[item.p as usize];
            if let Some(sym) = p.rhs.get(item.dot) {
                let id = symbol_id(sym, &mut symbols, &mut sym_ids, nonterminal_count);
                edges.entry(id).or_default().push(Item {
                    p: item.p,
                    dot: item.dot + 1,
                    look: item.look.clone(),
                });
            }
        }
        for (id, moved) in edges {
            let closed = closure(&moved, &prods, &first, &nullable);
            let next = match states.iter().position(|st| st == &closed) {
                Some(pos) => pos as u32,
                None => {
                    let nid = states.len() as u32;
                    states.push(closed);
                    action_index.extend(std::iter::repeat_n(u32::MAX, 256));
                    goto.extend(std::iter::repeat_n(
                        u32::MAX,
                        nonterminal_count as usize + 1,
                    ));
                    nid
                }
            };
            let from = worklist - 1;
            match &symbols[id] {
                Symbol::NonTerminal(nt) => {
                    goto[from * (nonterminal_count as usize + 1) + *nt as usize] = next;
                }
                Symbol::Byte(b) => {
                    let col = from * 256 + *b as usize;
                    let slot = &mut action_index[col];
                    let act = LrAction::Shift(next);
                    if *slot == u32::MAX {
                        actions.push(act);
                        *slot = (actions.len() - 1) as u32;
                    } else if actions[*slot as usize] != act {
                        return Err(LrBuildError(format!(
                            "shift/reduce conflict at state {from} byte {b}"
                        )));
                    }
                }
                Symbol::Class(bits) => {
                    for (b, &on) in bits.iter().enumerate() {
                        if !on {
                            continue;
                        }
                        let col = from * 256 + b;
                        let slot = &mut action_index[col];
                        let act = LrAction::Shift(next);
                        if *slot == u32::MAX {
                            actions.push(act);
                            *slot = (actions.len() - 1) as u32;
                        } else if actions[*slot as usize] != act {
                            return Err(LrBuildError(format!(
                                "shift/reduce conflict at state {from} byte {b}"
                            )));
                        }
                    }
                }
            }
        }
        // The reductions: the completed items (the dot at the end).
        for item in &set {
            let p = &prods[item.p as usize];
            if item.dot >= p.rhs.len() {
                let act = if item.p == accept_prod as u32 {
                    LrAction::Accept
                } else {
                    LrAction::Reduce(item.p)
                };
                for b in 0..256 {
                    if !item.look[b] {
                        continue;
                    }
                    let col = (worklist - 1) * 256 + b;
                    let slot = &mut action_index[col];
                    if *slot == u32::MAX {
                        actions.push(act.clone());
                        *slot = (actions.len() - 1) as u32;
                    } else if actions[*slot as usize] != act {
                        return Err(LrBuildError(format!(
                            "shift/reduce conflict at state {} byte {b}",
                            worklist - 1
                        )));
                    }
                }
            }
        }
    }

    Ok(LrTable {
        action_index,
        actions,
        goto,
        nonterminal_count,
        productions: prods,
        names: Vec::new(),
        start: 0,
        state_count: states.len() as u32,
    })
}

/// The value an LR reduce produces (slice 2's structural model — the
/// same-trees law's shapes; the arena wiring rides slice 3's compiler
/// seam).
#[derive(Debug, Clone, PartialEq)]
pub enum LrValue {
    /// A consumed span: `[start, end)` over the input bytes (a
    /// terminal production's merged run, or one shifted byte).
    Span {
        /// The inclusive start offset.
        start: usize,
        /// The exclusive end offset.
        end: usize,
    },
    /// The children's values as one `:sequence` node.
    Seq(Vec<LrValue>),
    /// The iterations as one flat `:repetition` node.
    Rep(Vec<LrValue>),
    /// `{name: inner}` — the v2 named law.
    Named {
        /// The rule name.
        name: String,
        /// The wrapped value.
        inner: Box<LrValue>,
    },
    /// The tagged empty (an empty production).
    Nil,
}

/// The executor's failure: an ACTION-table error (the byte cannot
/// continue any viable production) or a truncated input — the same
/// acceptance contract as the classic (the rule fails at the deepest
/// offset).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LrRunError(pub String);

/// Run the LR(1) table over `input` (the ENTRY rule; consume-all).
///
/// The EOF convention is slice 1's sentinel: byte column 0 carries
/// the accept/reduce-at-EOF actions, and byte 0 never appears in
/// valid UTF-8 input.
pub fn lr_run(table: &LrTable, input: &[u8]) -> Result<LrValue, LrRunError> {
    // The parse stack: one state + one value + one span-start per
    // stack symbol.
    let mut states = vec![table.start];
    let mut values: Vec<LrValue> = Vec::new();
    let mut starts: Vec<usize> = Vec::new();
    let mut pos = 0usize;
    loop {
        let state = *states.last().expect("the LR stack never empties");
        let (byte, eof) = match input.get(pos) {
            Some(&b) if b != 0 => (b as usize, false),
            // End of input (or a NUL byte, which the sentinel owns —
            // valid UTF-8 never contains it, so this is EOF).
            _ => (0usize, true),
        };
        let slot = table.action_index[state as usize * 256 + byte];
        if slot == u32::MAX {
            return Err(LrRunError(match eof {
                true => format!("unexpected end of input at byte {pos}"),
                false => format!("unexpected byte 0x{byte:02x} at offset {pos}"),
            }));
        }
        match &table.actions[slot as usize] {
            LrAction::Shift(next) => {
                values.push(LrValue::Span {
                    start: pos,
                    end: pos + 1,
                });
                starts.push(pos);
                states.push(*next);
                pos += 1;
            }
            LrAction::Reduce(p) => {
                let prod = table
                    .productions
                    .get(*p as usize)
                    .ok_or_else(|| LrRunError(format!("bad production {}", *p)))?;
                let n = prod.rhs.len();
                let start = if n == 0 {
                    pos
                } else {
                    let idx = starts.len() - n;
                    starts[idx]
                };
                // Pop the rhs's values (in order).
                let popped: Vec<LrValue> = values.split_off(values.len() - n);
                starts.truncate(starts.len() - n);
                states.truncate(states.len() - n);
                let value = match &prod.kind {
                    ProductionKind::Named(id) => {
                        let name = table.names.get(*id as usize).cloned().unwrap_or_default();
                        let inner = popped
                            .into_iter()
                            .next()
                            .expect("Named productions carry one symbol");
                        LrValue::Named {
                            name,
                            inner: Box::new(inner),
                        }
                    }
                    ProductionKind::Sequence => LrValue::Seq(popped),
                    ProductionKind::Repetition => {
                        // The envelope fold: `aux → body aux`
                        // prepends the body's iteration to the rest's
                        // accumulated list; the empty production
                        // starts it.
                        let mut it = popped.into_iter();
                        let mut items = Vec::new();
                        if let Some(head) = it.next() {
                            items.push(head);
                            if let Some(LrValue::Rep(rest)) = it.next() {
                                items.extend(rest);
                            }
                        }
                        LrValue::Rep(items)
                    }
                    ProductionKind::Pass => popped
                        .into_iter()
                        .next()
                        .expect("Pass productions carry one symbol"),
                    ProductionKind::Span => LrValue::Span { start, end: pos },
                    ProductionKind::Empty => LrValue::Nil,
                };
                // The GOTO from the state now on top.
                let top = *states.last().expect("the LR stack never empties");
                let next = table.goto
                    [top as usize * (table.nonterminal_count as usize + 1) + prod.lhs as usize];
                if next == u32::MAX {
                    return Err(LrRunError(format!(
                        "missing goto for nt {} at state {top}",
                        prod.lhs
                    )));
                }
                values.push(value);
                starts.push(start);
                states.push(next);
            }
            LrAction::Accept => {
                return Ok(values.pop().expect("accept carries the entry value"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::parser_dsl;
    use crate::portable::parser_dsl::{choice, dynamic, re, ref_, seq, GrammarBuilder, ParsletExt};

    /// The mini recursive spine: value = object / string;
    /// object = "{" value "}"; string = the class run. LEFT-recursive
    /// free, RIGHT-recursive through value — the LR machine's home
    /// territory.
    // The earlier "lowering count desync" theory was wrong: the panic
    // was symbol_id returning nt_count+nt for NonTerminal symbols
    // WITHOUT interning them, so the GOTO edge's symbols[id] indexed
    // out of bounds. The interning is uniform now (the kind byte
    // disambiguates the keys).
    #[test]
    fn lr_builds_the_recursive_spine() {
        let value = choice(vec![
            dynamic(seq(vec![
                dynamic(parser_dsl::str("{")),
                dynamic(ref_("value")),
                dynamic(parser_dsl::str("}")),
            ])),
            dynamic(re("[a-z]").many()),
        ]);
        let g = GrammarBuilder::new()
            .rule("value", value)
            .build()
            .to_json()
            .unwrap();
        let grammar = crate::portable::grammar::Grammar::from_json(&g).unwrap();
        let table = build_lr_table(&grammar, grammar.root).expect("the spine builds");
        assert!(table.state_count > 1, "the item sets constructed");
        assert!(
            table.actions.iter().any(|a| matches!(a, LrAction::Accept)),
            "the accept action exists"
        );
        assert!(
            table
                .actions
                .iter()
                .any(|a| matches!(a, LrAction::Shift(_))),
            "the shift actions exist"
        );
        assert!(
            table
                .actions
                .iter()
                .any(|a| matches!(a, LrAction::Reduce(_))),
            "the reduce actions exist"
        );
    }

    /// The LEFT-recursive variant (X → "{" X "}"), also LR-expressible
    /// (LR's superpower over the DFA backend).
    #[test]
    fn lr_builds_left_recursion() {
        let mut g = crate::portable::grammar::Grammar::new();
        let open = g.add_atom(Atom::Str {
            pattern: "{".to_string(),
        });
        let close = g.add_atom(Atom::Str {
            pattern: "}".to_string(),
        });
        let letter = g.add_atom(Atom::Re {
            pattern: "[a-z]".to_string(),
        });
        let letters = g.add_atom(Atom::Repetition {
            atom: letter,
            min: 1,
            max: None,
            tag: crate::portable::grammar::RepetitionTag::Repetition,
        });
        let seq = g.add_atom(Atom::Sequence {
            atoms: vec![open, letters, close],
        });
        let inner = g.add_atom(Atom::Sequence {
            atoms: vec![open, seq, close],
        });
        let alt = g.add_atom(Atom::Alternative {
            atoms: vec![inner, letters],
        });
        g.root = alt;
        let text = g.to_json().unwrap();
        let grammar = crate::portable::grammar::Grammar::from_json(&text).unwrap();
        let table = build_lr_table(&grammar, g.root).expect("the left-recursive build");
        assert!(table.state_count > 1);
    }

    /// The t9-format mapper (the differential's common currency): the
    /// same shape strings the walker visitor prints (the tagged
    /// arrays, the hashes, the input refs).
    fn lr_value_sexpr(v: &LrValue, input: &str, out: &mut String) {
        match v {
            LrValue::Span { start, end } => {
                out.push_str(&format!("in:{:?}@{};", &input[*start..*end], start))
            }
            LrValue::Seq(children) => {
                out.push_str(&format!("[{}(", children.len() + 1));
                out.push_str("s:\":sequence\";");
                for c in children {
                    lr_value_sexpr(c, input, out);
                }
                out.push_str(")];");
            }
            LrValue::Rep(iterations) => {
                out.push_str(&format!("[{}(", iterations.len() + 1));
                out.push_str("s:\":repetition\";");
                for c in iterations {
                    lr_value_sexpr(c, input, out);
                }
                out.push_str(")];");
            }
            LrValue::Named { name, inner } => {
                out.push('{');
                out.push_str(&format!("{name:?}->"));
                lr_value_sexpr(inner, input, out);
                out.push_str("};");
            }
            LrValue::Nil => {}
        }
    }

    /// The walker's tree in the same t9 format (the t9 probe's M
    /// visitor).
    fn walker_sexpr(
        value: &crate::portable::ast::AstNode,
        arena: &crate::portable::arena::AstArena,
        input: &str,
    ) -> String {
        use crate::portable::visitor::{walk, Visitor};
        struct M(String);
        impl Visitor for M {
            fn visit_input_ref(&mut self, offset: u32, length: u32, input: &str) {
                self.0.push_str(&format!(
                    "in:{:?}@{};",
                    &input[offset as usize..offset as usize + length as usize],
                    offset
                ));
            }
            fn visit_string_ref(&mut self, pool: u32, arena: &crate::portable::arena::AstArena) {
                self.0
                    .push_str(&format!("s:{:?};", arena.get_string(pool as usize)));
            }
            fn visit_array_start(&mut self, _: u32, length: u32) {
                self.0.push_str(&format!("[{length}("));
            }
            fn visit_array_end(&mut self, _: u32, _: u32) {
                self.0.push_str(")];");
            }
            fn visit_hash_start(&mut self, _: u32, _: u32) {
                self.0.push('{');
            }
            fn visit_hash_key(&mut self, key: &str) {
                self.0.push_str(&format!("{key:?}->"));
            }
            fn visit_hash_end(&mut self, _: u32, _: u32) {
                self.0.push_str("};");
            }
        }
        walk(value, arena, input, M(String::new())).0
    }

    /// THE SAME-TREES LAW (slice 2's differential pin): the LR
    /// executor's value trees match the classic walker's byte for
    /// byte on the recursive spine — acceptance, the spans, the
    /// envelopes, the named wraps.
    #[test]
    fn lr_run_matches_the_walker_on_the_spine() {
        use crate::portable::arena::AstArena;
        use crate::portable::PortableParser;
        let value = choice(vec![
            dynamic(seq(vec![
                dynamic(parser_dsl::str("{")),
                dynamic(ref_("value")),
                dynamic(parser_dsl::str("}")),
            ])),
            dynamic(re("[a-z]").many()),
        ]);
        let g = GrammarBuilder::new()
            .rule("value", value)
            .build()
            .to_json()
            .unwrap();
        let grammar = crate::portable::grammar::Grammar::from_json(&g).unwrap();
        let table = build_lr_table(&grammar, grammar.root).expect("the spine builds");

        for input in ["abc", "{a}", "{{a}}", "{abc}{d}", "", "{xyz}{p}{qq}"] {
            let mut arena = AstArena::for_input(input.len());
            let grammar2 = crate::portable::grammar::Grammar::from_json(&g).unwrap();
            let mut parser = PortableParser::new(&grammar2, input, &mut arena);
            let walker = parser.parse();
            let lr = lr_run(&table, input.as_bytes());
            match (walker, lr) {
                (Ok(w), Ok(v)) => {
                    let mut s = String::new();
                    lr_value_sexpr(&v, input, &mut s);
                    let ws = walker_sexpr(&w, &arena, input);
                    assert_eq!(s, ws, "the LR tree diverged from the walker on {input:?}");
                }
                (Err(_), Err(_)) => {}
                (w, l) => panic!("acceptance diverged on {input:?}: walker={w:?} lr={l:?}"),
            }
        }
    }
}
