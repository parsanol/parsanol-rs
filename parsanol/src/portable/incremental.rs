//! Incremental Parsing Support
//!
//! This module provides incremental parsing capabilities, allowing efficient
//! re-parsing when only small portions of the input change.
//!
//! # Overview
//!
//! Traditional packrat parsing requires re-parsing the entire input when any
//! change is made. Incremental parsing tracks:
//! 1. **Dirty regions** - portions of input that have changed
//! 2. **Cache invalidation** - which cached results are affected
//! 3. **Reuse** - which results can be safely reused
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    Incremental Parser                            │
//! ├─────────────────────────────────────────────────────────────────┤
//! │  Input V1: "hello world"                                        │
//! │  Cache: {pos:0, atom:1} -> {end:5, ast:...}                     │
//! │         {pos:6, atom:1} -> {end:11, ast:...}                    │
//! └─────────────────────────────────────────────────────────────────┘
//!                          │
//!                          ▼ Edit: Change "world" -> "rust"
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    Dirty Region Tracker                          │
//! ├─────────────────────────────────────────────────────────────────┤
//! │  Change: offset=6, old_len=5, new_len=4                         │
//! │  Dirty range: [6, 11)                                           │
//! │  Affected cache entries: positions >= 6                         │
//! └─────────────────────────────────────────────────────────────────┘
//!                          │
//!                          ▼ Re-parse
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    Incremental Result                            │
//! ├─────────────────────────────────────────────────────────────────┤
//! │  Reused cache entries: 1 (position 0-5)                         │
//! │  Re-parsed entries: 1 (position 6+)                             │
//! │  Time saved: ~50%                                               │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! use parsanol::portable::{Grammar, AstArena, incremental::IncrementalParser};
//!
//! // Create incremental parser
//! let grammar = /* ... */;
//! let mut parser = IncrementalParser::new(&grammar);
//!
//! // Initial parse
//! let input = "hello world";
//! let result = parser.parse(input)?;
//!
//! // Edit and re-parse
//! let new_input = "hello rust";
//! let edit = Edit { offset: 6, old_length: 5, new_length: 4 };
//! let result = parser.parse_with_edit(new_input, edit)?;
//! ```

use super::{
    arena::AstArena,
    ast::{AstNode, ParseError},
    cache::{DenseCache, NodeTag},
    grammar::{Atom, Grammar, RepetitionTag},
};

/// Represents a change to the input
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edit {
    /// Byte offset where the change starts
    pub offset: usize,
    /// Length of the old text being replaced
    pub old_length: usize,
    /// Length of the new text
    pub new_length: usize,
}

impl Edit {
    /// Create a new edit
    #[inline]
    pub fn new(offset: usize, old_length: usize, new_length: usize) -> Self {
        Self {
            offset,
            old_length,
            new_length,
        }
    }

    /// Create an insertion edit
    #[inline]
    pub fn insert(offset: usize, length: usize) -> Self {
        Self {
            offset,
            old_length: 0,
            new_length: length,
        }
    }

    /// Create a deletion edit
    #[inline]
    pub fn delete(offset: usize, length: usize) -> Self {
        Self {
            offset,
            old_length: length,
            new_length: 0,
        }
    }

    /// Create a replacement edit
    #[inline]
    pub fn replace(offset: usize, old_length: usize, new_length: usize) -> Self {
        Self {
            offset,
            old_length,
            new_length,
        }
    }

    /// Calculate the delta (change in length)
    #[inline]
    pub fn delta(&self) -> isize {
        self.new_length as isize - self.old_length as isize
    }

    /// Get the range affected by this edit (in old coordinates)
    #[inline]
    #[allow(dead_code)]
    pub fn old_range(&self) -> std::ops::Range<usize> {
        self.offset..self.offset + self.old_length
    }

    /// Check if this edit affects a position (in old coordinates)
    #[inline]
    #[allow(dead_code)]
    pub fn affects_position(&self, pos: usize) -> bool {
        pos >= self.offset
    }

    /// Translate a position from old to new coordinates
    #[inline]
    pub fn translate_position(&self, pos: usize) -> usize {
        if pos <= self.offset {
            pos
        } else if pos <= self.offset + self.old_length {
            // Position inside deleted region -> map to start of edit
            self.offset + self.new_length
        } else {
            // Position after edit -> apply delta
            ((pos as isize) + self.delta()) as usize
        }
    }
}

/// Tracks dirty regions in the input
#[derive(Debug, Clone)]
pub struct DirtyRegionTracker {
    /// List of dirty regions (non-overlapping, sorted by start)
    regions: Vec<DirtyRegion>,
}

/// A dirty region in the input
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirtyRegion {
    /// Start byte offset (inclusive)
    pub start: usize,
    /// End byte offset (exclusive)
    pub end: usize,
}

impl DirtyRegion {
    /// Create a new dirty region
    #[inline]
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Check if a position is within this region
    #[inline]
    pub fn contains(&self, pos: usize) -> bool {
        pos >= self.start && pos < self.end
    }

    /// Check if this region overlaps with another
    #[inline]
    pub fn overlaps(&self, other: &DirtyRegion) -> bool {
        self.start < other.end && other.start < self.end
    }

    /// Merge this region with another (they must overlap or be adjacent)
    #[inline]
    pub fn merge(&self, other: &DirtyRegion) -> DirtyRegion {
        DirtyRegion {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

impl DirtyRegionTracker {
    /// Create a new dirty region tracker
    #[inline]
    pub fn new() -> Self {
        Self {
            regions: Vec::new(),
        }
    }

    /// Mark a region as dirty
    pub fn mark_dirty(&mut self, region: DirtyRegion) {
        // Find where to insert/merge
        let mut merged = region;
        let mut insert_at = None;
        let mut remove_count = 0;

        for (i, existing) in self.regions.iter().enumerate() {
            if existing.end < merged.start {
                // Existing region is before, continue
                continue;
            } else if existing.start > merged.end {
                // Existing region is after, insert here
                insert_at = Some(i);
                break;
            } else {
                // Overlapping or adjacent, merge
                merged = merged.merge(existing);
                if insert_at.is_none() {
                    insert_at = Some(i);
                }
                remove_count += 1;
            }
        }

        if remove_count > 0 {
            // Remove merged regions and insert the combined one
            let idx = insert_at.unwrap();
            self.regions.drain(idx..idx + remove_count);
            self.regions.insert(idx, merged);
        } else if let Some(idx) = insert_at {
            self.regions.insert(idx, merged);
        } else {
            self.regions.push(merged);
        }
    }

    /// Mark an edit as dirty
    #[inline]
    pub fn mark_edit(&mut self, edit: &Edit) {
        self.mark_dirty(DirtyRegion::new(edit.offset, edit.offset + edit.old_length));
    }

    /// Check if a position is dirty
    pub fn is_dirty(&self, pos: usize) -> bool {
        // Binary search for efficiency with many regions
        self.regions
            .binary_search_by(|r| {
                if r.end <= pos {
                    std::cmp::Ordering::Less
                } else if r.start > pos {
                    std::cmp::Ordering::Greater
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .is_ok()
    }

    /// Check if a range overlaps with any dirty region
    pub fn is_range_dirty(&self, start: usize, end: usize) -> bool {
        self.regions.iter().any(|r| r.start < end && start < r.end)
    }

    /// Get all dirty regions
    #[inline]
    pub fn regions(&self) -> &[DirtyRegion] {
        &self.regions
    }

    /// Clear all dirty regions
    #[inline]
    pub fn clear(&mut self) {
        self.regions.clear();
    }

    /// Check if there are any dirty regions
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.regions.is_empty()
    }
}

impl Default for DirtyRegionTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Incremental parser that efficiently re-parses after edits
pub struct IncrementalParser<'a> {
    /// The compiled grammar
    grammar: std::borrow::Cow<'a, Grammar>,

    /// Packrat cache (preserved across parses)
    cache: DenseCache,

    /// Stable arena backing snapshot-marked cache entries: adopted
    /// node data that must survive across parses (TODO.perf/4).
    snapshot_arena: AstArena,

    /// Session-owned persistent output arena for the `*_retained`
    /// parse family (TODO.perf/9): node data from every retained
    /// parse stays valid, so retention keeps full container entries
    /// with no adoption copy and the previous parse TREE persists
    /// alongside the memo window.
    output_arena: AstArena,

    /// Dirty region tracker
    dirty_tracker: DirtyRegionTracker,

    /// The previous retained parse's root node (splice graft source)
    /// and its repetition-body item boundaries (pos, end) in input
    /// coordinates, collected during the last retention pass.
    last_root: Option<AstNode>,
    body_chain: Vec<(u32, u32)>,
    /// The root repetition's iteration bounds from the most recent
    /// parse, staged here because the parser is consumed (into_cache)
    /// before the chain assignment below.
    last_spine_trace: Vec<(u32, u32)>,
    spliced_parses: usize,

    /// Previous input length (for position translation)
    prev_input_len: usize,

    /// The pending single edit's suffix shift: (edit_end in OLD input
    /// coordinates, length delta). Set by the single-edit re-parse
    /// entrypoints; the retention pass keys the surviving suffix
    /// entries at delta-shifted positions (rs#542). Multi-edit calls
    /// leave it None and keep the cutoff-only window.
    pending_shift: Option<(u32, i64)>,

    /// The edit end in NEW coordinates after the shift applied — the
    /// floor above which the parse's FRESH suffix entries stay
    /// retained post-parse (they are already in the new coordinates).
    /// usize::MAX when no shift applied.
    applied_suffix_floor: usize,

    /// The spine runs the last FULL parse recorded, keyed
    /// `(atom_id, run_start, depth, items)` — depth 1 is the top-level
    /// run (the path-splice chain), deeper runs are the nested lists
    /// the multi-level splice bridges at. Cleared whenever a splice
    /// moves coordinates (only a full parse re-derives them).
    spine_runs: Vec<SpineRun>,
    /// The spine bodies the parser watches (every unbounded run
    /// records its iteration bounds).
    spine_bodies: Vec<usize>,
    /// Body atom → the unbounded repetition atom for that body (the
    /// id spine runs are recorded under).
    body_reps: std::collections::HashMap<usize, usize>,
    /// Retained parses rebuilt via the deep (nested-run) surgery.
    spliced_deep: usize,
    /// The path-splice's persisted item bounds (the spine list's
    /// items, current-input coordinates). Seeded by one drain after a
    /// full parse; every later keystroke derives its bounds by shift
    /// (below unchanged, above += delta) plus ONE bridge probe for the
    /// edited item — no per-keystroke drain, no memo dependency (the
    /// body atom carries none).
    path_chain: Vec<(u32, u32)>,

    /// The applied single edit (cutoff = edit start, edit end in OLD
    /// coordinates, delta), stashed by the shift retention for the
    /// splice's chain derivation. None when no shift applied.
    last_edit: Option<(usize, u32, i64)>,
}

/// The selected nested-run candidate: where to bridge and what the
/// arm needs to patch.
struct DeepCandidate {
    item_start: usize,
    item_end_old: usize,
    span: usize,
    below: Vec<(usize, usize)>,
    above_shifted: Vec<(usize, usize)>,
    tail_steps: Vec<crate::portable::spine::SpineStep>,
    prev_deep_list: AstNode,
}

/// The nested-run surgery the arm executes after `into_cache` (the
/// deep bridge probe ran in the derivation; the patch reassembles the
/// containers from the deep list up to the level-0 item).
struct DeepPatch {
    /// The deep item's new tree (the bridge probe's value).
    bridge_node: AstNode,
    /// The previous tree's deep list node (its items split at
    /// `below_d_count`: below clones | the bridge | shifted above).
    prev_deep_list: AstNode,
    /// The tail steps from the level-0 body down to the deep rep,
    /// with the previous tree's container nodes — the surgery
    /// navigates these backward, replacing the list and shifting the
    /// siblings above the edit.
    prev_containers: Vec<(crate::portable::spine::SpineStep, AstNode)>,
    /// The deep list's item count below the edit.
    below_d_count: usize,
    /// The deep run's refreshed bounds (new coordinates).
    new_run_items: Vec<(u32, u32)>,
    /// The spine_runs entry the deep run occupies.
    run_idx: usize,
}

/// A successful path-splice: the level-0 item index, the item tree
/// for slot k (a bridge parse — or the deep surgery's patched item),
/// and the re-parsed sibling tails innermost-first.
struct SpliceOutcome {
    k: usize,
    bridge: AstNode,
    tails: Vec<Vec<AstNode>>,
    deep: Option<DeepPatch>,
}

/// One recorded spine run: `(body atom, run start, watched depth,
/// item bounds)`.
pub(crate) type SpineRun = (usize, u32, usize, Vec<(u32, u32)>);

/// A persistent output arena past this budget is reset wholesale
/// between retained parses (retention dropped, next parse runs cold)
/// instead of growing without bound across an edit session.
const OUTPUT_ARENA_BUDGET: usize = 64 * 1024 * 1024;

/// Deep-copy a tree into the arena with every InputRef offset shifted
/// by `delta`. The suffix bytes are byte-identical at shifted
/// positions, so the shifted copy of a suffix item is the correct
/// tree without re-parsing — the body atom carries no memo, and the
/// probe replay paid a full member parse per suffix item (the
/// incremental gate's remaining ceiling, now gone).
fn copy_shift_tree(node: &AstNode, delta: i64, arena: &mut AstArena) -> AstNode {
    match node {
        AstNode::InputRef { offset, length } => AstNode::InputRef {
            offset: ((*offset as i64) + delta).max(0) as u32,
            length: *length,
        },
        AstNode::Array { pool_index, length } => {
            let items = arena.get_array(*pool_index as usize, *length as usize);
            let copied: Vec<AstNode> = items
                .iter()
                .map(|item| copy_shift_tree(item, delta, arena))
                .collect();
            let (start, len) = arena.store_array(&copied);
            AstNode::Array {
                pool_index: start,
                length: len,
            }
        }
        AstNode::Hash { pool_index, length } => {
            let pairs = arena.get_hash_items(*pool_index as usize, *length as usize);
            let copied: Vec<(&str, AstNode)> = pairs
                .iter()
                .map(|(key, item)| (key.as_str(), copy_shift_tree(item, delta, arena)))
                .collect();
            let (start, len) = arena.store_hash(&copied);
            AstNode::Hash {
                pool_index: start,
                length: len,
            }
        }
        AstNode::StringRef { pool_index } => {
            let text = arena.get_string(*pool_index as usize).to_string();
            arena.intern_string(&text)
        }
        other => other.clone(),
    }
}

/// Copy a tree with only the leaves ABOVE the edit shifted by `delta`
/// (the tree-surgery building block: containers straddling the edit
/// keep their below-edit leaves untouched and shift the rest).
fn shift_if_above(
    node: &AstNode,
    edit_end_old: usize,
    delta: i64,
    arena: &mut AstArena,
) -> AstNode {
    match node {
        AstNode::InputRef { offset, length } => {
            if (*offset as usize) >= edit_end_old {
                AstNode::InputRef {
                    offset: ((*offset as i64) + delta).max(0) as u32,
                    length: *length,
                }
            } else {
                node.clone()
            }
        }
        AstNode::Array { pool_index, length } => {
            let items = arena.get_array(*pool_index as usize, *length as usize);
            let copied: Vec<AstNode> = items
                .iter()
                .map(|item| shift_if_above(item, edit_end_old, delta, arena))
                .collect();
            let (start, len) = arena.store_array(&copied);
            AstNode::Array {
                pool_index: start,
                length: len,
            }
        }
        AstNode::Hash { pool_index, length } => {
            let pairs = arena.get_hash_items(*pool_index as usize, *length as usize);
            let copied: Vec<(&str, AstNode)> = pairs
                .iter()
                .map(|(key, item)| {
                    (
                        key.as_str(),
                        shift_if_above(item, edit_end_old, delta, arena),
                    )
                })
                .collect();
            let (start, len) = arena.store_hash(&copied);
            AstNode::Hash {
                pool_index: start,
                length: len,
            }
        }
        other => other.clone(),
    }
}

/// Walk the previous tree through `steps`, collecting each container
/// node before descending — the surgery's backward map. Named checks
/// the hash key; Sequence takes slot `index + 1` (tagged arrays);
/// Alternative passes through (nothing to rebuild).
fn walk_containers(
    arena: &AstArena,
    root: &AstNode,
    steps: &[crate::portable::spine::SpineStep],
) -> Option<Vec<(crate::portable::spine::SpineStep, AstNode)>> {
    use crate::portable::spine::SpineStep;
    let mut containers: Vec<(SpineStep, AstNode)> = Vec::new();
    let mut node = root.clone();
    for step in steps {
        match step {
            SpineStep::Named { name } => {
                if let AstNode::Hash { pool_index, length } = &node {
                    let items = arena.get_hash_items(*pool_index as usize, *length as usize);
                    let (key, value) = items.first()?;
                    if key != name {
                        return None;
                    }
                    containers.push((step.clone(), node.clone()));
                    node = value.clone();
                } else {
                    return None;
                }
            }
            SpineStep::Sequence { index, .. } => {
                if let AstNode::Array { pool_index, length } = &node {
                    let children = arena.get_array(*pool_index as usize, *length as usize);
                    if *index + 1 >= children.len() {
                        return None;
                    }
                    let child = children[*index + 1].clone();
                    containers.push((step.clone(), node.clone()));
                    node = child;
                } else {
                    return None;
                }
            }
            SpineStep::Alternative { .. } => {}
        }
    }
    Some(containers)
}

/// The unbounded repetition atom per spine body — the id the
/// parser's spine runs are recorded under. Grammars share one rep
/// atom across every nesting of the same list shape.
fn body_reps_of(grammar: &Grammar) -> std::collections::HashMap<usize, usize> {
    let mut map = std::collections::HashMap::new();
    for (i, a) in grammar.atoms.iter().enumerate() {
        if let Atom::Repetition {
            atom,
            min: _,
            max: None,
            tag: crate::portable::grammar::RepetitionTag::Repetition,
        } = a
        {
            map.entry(*atom).or_insert(i);
        }
    }
    map
}

impl<'a> IncrementalParser<'a> {
    /// Create a new incremental parser
    #[inline]
    pub fn new(grammar: &'a Grammar) -> Self {
        let spine_bodies = crate::portable::spine::discover_spines(grammar)
            .iter()
            .map(|s| s.body)
            .collect();
        let body_reps = body_reps_of(grammar);
        Self {
            grammar: std::borrow::Cow::Borrowed(grammar),
            cache: DenseCache::new(4096),
            snapshot_arena: AstArena::new(),
            output_arena: AstArena::new(),
            dirty_tracker: DirtyRegionTracker::new(),
            last_root: None,
            body_chain: Vec::new(),
            spliced_deep: 0,
            spine_runs: Vec::new(),
            spine_bodies,
            body_reps,
            last_spine_trace: Vec::new(),
            spliced_parses: 0,
            prev_input_len: 0,
            pending_shift: None,
            applied_suffix_floor: usize::MAX,
            path_chain: Vec::new(),
            last_edit: None,
        }
    }

    /// Create a new incremental parser owning its grammar (FFI
    /// sessions hold the grammar and the parser together).
    #[inline]
    pub fn owned(grammar: Grammar) -> IncrementalParser<'static> {
        let spine_bodies = crate::portable::spine::discover_spines(&grammar)
            .iter()
            .map(|s| s.body)
            .collect();
        let body_reps = body_reps_of(&grammar);
        IncrementalParser {
            grammar: std::borrow::Cow::Owned(grammar),
            cache: DenseCache::new(4096),
            snapshot_arena: AstArena::new(),
            output_arena: AstArena::new(),
            dirty_tracker: DirtyRegionTracker::new(),
            last_root: None,
            body_chain: Vec::new(),
            spliced_deep: 0,
            spine_runs: Vec::new(),
            spine_bodies,
            body_reps,
            last_spine_trace: Vec::new(),
            spliced_parses: 0,
            prev_input_len: 0,
            pending_shift: None,
            applied_suffix_floor: usize::MAX,
            path_chain: Vec::new(),
            last_edit: None,
        }
    }

    /// Parse input for the first time
    pub fn parse(&mut self, input: &str, arena: &mut AstArena) -> Result<AstNode, ParseError> {
        // Clear previous state. The cache is sized for the document,
        // not the 4 KiB default: retention needs the EARLY memo window
        // to survive the whole parse, and the recycling cache discards
        // whatever exceeds capacity (a 4 KiB window holds only the
        // document's tail, so nothing before an edit would ever be
        // retained).
        self.cache = DenseCache::for_input(input.len(), self.grammar.atom_count());
        self.snapshot_arena = AstArena::new();
        self.dirty_tracker.clear();
        self.prev_input_len = 0;

        // Initial parse, then snapshot the whole successful cache so
        // the first incremental parse starts from valid cross-parse
        // entries.
        let (result, _, _) = self.parse_and_snapshot(input, arena);
        result
    }

    /// The single-edit suffix shift (rs#542): keep entries entirely
    /// ABOVE the edit region, keyed at delta-shifted positions — the
    /// suffix bytes are byte-identical between the old and new inputs,
    /// so a match lying wholly within the suffix is equally valid at
    /// shifted coordinates (tree-sitter's old-tree reuse, in memo-key
    /// space). Only arena-free values shift: pool-backed entries
    /// describe trees whose inner offsets cannot move coherently.
    /// Consumes `pending_shift`.
    fn retain_suffix_shift(&mut self, cutoff: usize, root_atom: u16, input_len_changed: bool) {
        let Some((edit_end_old, delta)) = self.pending_shift.take() else {
            return;
        };
        self.applied_suffix_floor = (edit_end_old as i64 + delta).max(0) as usize;
        self.last_edit = Some((cutoff, edit_end_old, delta));
        self.cache.retain(|entry| {
            let entry_end = entry.end_pos as usize;
            let is_root_at_start = entry.pos == 0 && entry.atom_id == root_atom;
            let below = entry_end < cutoff && !(input_len_changed && is_root_at_start);
            let shiftable = entry.pos as i64 >= edit_end_old as i64
                && matches!(entry.node_tag, NodeTag::Nil | NodeTag::InputRef);
            below || shiftable
        });
        for entry in self.cache.entries_mut() {
            if entry.pos as i64 >= edit_end_old as i64 {
                entry.pos = (entry.pos as i64 + delta) as u32;
                entry.end_pos = (entry.end_pos as i64 + delta) as u32;
                if entry.node_tag == NodeTag::InputRef {
                    entry.node_data_a = (entry.node_data_a as i64 + delta) as u32;
                }
            }
        }
        // The keys moved: rebuild the slot table.
        self.cache.retain(|_| true);
    }

    /// Parse against the retained cache, then convert the entries the
    /// dirty regions did not touch into snapshots. Returns the tree,
    /// the retained count at parse start, and the post-parse entry
    /// count for the efficiency stats.
    fn parse_and_snapshot(
        &mut self,
        input: &str,
        arena: &mut AstArena,
    ) -> (Result<AstNode, ParseError>, usize, usize) {
        let before = self.cache.len();
        let cutoff = earliest_dirty_offset(&self.dirty_tracker);
        let root_atom = self.grammar.root as u16;
        let input_len_changed = self.prev_input_len != input.len();
        self.prev_input_len = input.len();

        // Drop entries the edit invalidated BEFORE parsing: entries at
        // or after the edit offset describe the OLD input (shifted
        // positions), and replaying them mid-parse produces results
        // beyond the new input's end. STRICT inequality: an entry
        // ending exactly at the edit offset depended on the byte at
        // that offset as its match boundary (maximal runs stop there),
        // and the edit can make the run extend differently.
        //
        // The surviving window is position-stable, so it replays
        // correctly.
        if self.pending_shift.is_some() {
            self.retain_suffix_shift(cutoff, root_atom, input_len_changed);
        } else {
            self.applied_suffix_floor = usize::MAX;
            self.last_edit = None;
            self.cache.retain(|entry| {
                let entry_end = entry.end_pos as usize;
                let is_root_at_start = entry.pos == 0 && entry.atom_id == root_atom;
                entry_end < cutoff && !(input_len_changed && is_root_at_start)
            });
        }

        let mut parser = super::parser::PortableParser::new_with_cache_and_snap(
            &self.grammar,
            input,
            arena,
            std::mem::take(&mut self.cache),
            Some(&self.snapshot_arena),
        );
        let result = parser.parse();
        self.cache = parser.into_cache();
        let post_parse = self.cache.len();

        // Budget guard: a snapshot arena past the cap is dropped
        // wholesale; the next parse runs cold instead of growing
        // without bound across edit sessions.
        if self.snapshot_arena.memory_usage() > 32 * 1024 * 1024 {
            self.snapshot_arena = AstArena::new();
            self.cache.drop_snapshots();
        }

        // Retain only arena-free entries (terminal InputRefs, scalars,
        // failures): adopting pool-backed values into a snapshot arena
        // measured SLOWER than the full re-parse it was meant to avoid
        // (a deep copy of the whole tree per parse); terminals are the
        // expensive byte-matching work, and re-walking structure
        // against terminal hits is cheap. The snapshot MARK stays: it
        // exempts retained entries from the generation check.
        self.cache.retain_snapshot_adopt(
            |entry| {
                let entry_end = entry.end_pos as usize;
                let is_root_at_start = entry.pos == 0 && entry.atom_id == root_atom;
                // Failures are never retained: their validity depends
                // on bytes after the failure position, which an edit
                // can change (an unknowable lookahead distance).
                let arena_free_success = entry.success
                    && matches!(
                        entry.to_node(),
                        crate::portable::ast::AstNode::InputRef { .. }
                            | crate::portable::ast::AstNode::Nil
                    );
                arena_free_success && entry_end < cutoff && !(input_len_changed && is_root_at_start)
            },
            // Never reached: survivors are arena-free by the
            // predicate, and only pool-backed values adopt.
            |node| node.clone(),
        );

        // TODO.perf/9 round 2 (rs#464): the slot table stays sized for
        // the session-start input while only a few thousand entries
        // survive retention — the capacity-wide maintenance loops are
        // the late band's remaining floor. Shrink to the survivors
        // (the insert path's load-factor check grows it back naturally
        // if the set recovers).
        self.cache.shrink_to_entries();

        self.dirty_tracker.clear();

        (result, before, post_parse)
    }

    /// Re-parse after an edit
    pub fn parse_with_edit(
        &mut self,
        input: &str,
        arena: &mut AstArena,
        edit: Edit,
    ) -> Result<IncrementalResult, ParseError> {
        self.dirty_tracker.mark_edit(&edit);
        self.pending_shift = Some((
            edit.offset as u32 + edit.old_length as u32,
            edit.delta() as i64,
        ));
        self.finish_parse(input, arena)
    }

    /// Re-parse after multiple edits
    pub fn parse_with_edits(
        &mut self,
        input: &str,
        arena: &mut AstArena,
        edits: &[Edit],
    ) -> Result<IncrementalResult, ParseError> {
        for edit in edits {
            self.dirty_tracker.mark_edit(edit);
        }
        self.finish_parse(input, arena)
    }

    /// Parse with the current dirty regions, then retain the cache
    /// window that is provably unaffected by them as snapshots.
    ///
    /// Retention rule: an entry survives only when its result ended
    /// at or before the earliest edit offset (its substring is
    /// byte-identical in the new input), and - when the input length
    /// changed - the root entry at position 0 is dropped, because the
    /// root must consume the whole (new) input. Surviving pool-backed
    /// node data is adopted into the snapshot arena; a fresh parse's
    /// arena is a different one, so unadopted pool references would
    /// dangle (the pre-snapshot implementation silently produced
    /// wrong trees across parses).
    fn finish_parse(
        &mut self,
        input: &str,
        arena: &mut AstArena,
    ) -> Result<IncrementalResult, ParseError> {
        let (result, _retained_before, post_parse) = self.parse_and_snapshot(input, arena);
        Ok(IncrementalResult {
            ast: result?,
            reused_cache_entries: self.cache.len(),
            invalidated_cache_entries: post_parse - self.cache.len(),
        })
    }

    // ========================================================================
    // Retained-tree parse family (TODO.perf/9)
    //
    // The `*_retained` parses own their output arena: node data from
    // every retained parse stays valid until the NEXT retained parse
    // or `clear`, so retention keeps FULL container entries (Array /
    // Hash / StringRef) with an identity "adoption" — no cross-arena
    // copy. This removes the adoption-cost ceiling that capped the
    // snapshot tier at terminal entries (TODO.perf/4/8) and persists
    // the previous parse tree alongside the memo window, which the
    // prefix-splice design builds on.
    // ========================================================================

    /// Parse input for the first time into the session-owned arena.
    ///
    /// The returned [`AstNode`] indexes [`Self::retained_arena`] and
    /// remains valid until the next `parse*_retained` call or
    /// [`Self::clear`]. Unlike [`Self::parse`], the tree is NOT
    /// rebuilt into a caller-provided arena — that is what lets the
    /// memo retain whole subtrees across edits.
    pub fn parse_retained(&mut self, input: &str) -> Result<AstNode, ParseError> {
        let (result, _, _) = self.retained_parse(input, false);
        result
    }

    /// Re-parse after an edit, into the session-owned arena.
    pub fn parse_with_edit_retained(
        &mut self,
        input: &str,
        edit: Edit,
    ) -> Result<IncrementalResult, ParseError> {
        self.dirty_tracker.mark_edit(&edit);
        self.pending_shift = Some((
            edit.offset as u32 + edit.old_length as u32,
            edit.delta() as i64,
        ));
        self.finish_retained(input)
    }

    /// Re-parse after multiple edits, into the session-owned arena.
    pub fn parse_with_edits_retained(
        &mut self,
        input: &str,
        edits: &[Edit],
    ) -> Result<IncrementalResult, ParseError> {
        for edit in edits {
            self.dirty_tracker.mark_edit(edit);
        }
        self.finish_retained(input)
    }

    /// The arena the retained parse family builds into. Nodes from the
    /// most recent `parse*_retained` call index this arena.
    #[inline]
    pub fn retained_arena(&self) -> &AstArena {
        &self.output_arena
    }

    /// The shared engine of the retained family: cold-start reset when
    /// `fresh`, otherwise parse against the retained cache window.
    fn retained_parse(
        &mut self,
        input: &str,
        fresh: bool,
    ) -> (Result<AstNode, ParseError>, usize, usize) {
        // Budget guard first: a persistent output arena past the cap
        // is reset wholesale (retention with it) instead of growing
        // without bound across an edit session. Resetting BEFORE the
        // parse keeps the about-to-be-built tree valid.
        if self.output_arena.memory_usage() > OUTPUT_ARENA_BUDGET {
            self.output_arena = AstArena::new();
            // Pool-backed entries reference the reset arena and dangle;
            // arena-free entries (terminals, failures) do not — keep
            // them, or long sessions go cold every parse past the cap
            // (the doc3 trace: 200 retained parses × 152 KB blew the
            // budget, cleared the window, reused 0 forever).
            self.cache
                .retain(|entry| matches!(entry.node_tag, NodeTag::Nil | NodeTag::InputRef));
            // Stale tree nodes reference the reset arena — the splice
            // must not graft from them.
            self.last_root = None;
            self.body_chain.clear();
        }

        let before = self.cache.len();
        let cutoff = if fresh {
            // First (or cold) parse: no edit invalidated anything.
            self.cache = DenseCache::for_input(input.len(), self.grammar.atom_count());
            self.dirty_tracker.clear();
            self.prev_input_len = 0;
            usize::MAX
        } else {
            earliest_dirty_offset(&self.dirty_tracker)
        };
        let root_atom = self.grammar.root as u16;
        let input_len_changed = self.prev_input_len != input.len();
        self.prev_input_len = input.len();

        // Same pre-parse invalidation as the snapshot tier.
        if self.pending_shift.is_some() {
            self.retain_suffix_shift(cutoff, root_atom, input_len_changed);
        } else {
            self.applied_suffix_floor = usize::MAX;
            self.last_edit = None;
            self.cache.retain(|entry| {
                let entry_end = entry.end_pos as usize;
                let is_root_at_start = entry.pos == 0 && entry.atom_id == root_atom;
                entry_end < cutoff && !(input_len_changed && is_root_at_start)
            });
        }

        // Splice eligibility (v1): a repetition-spine document — root
        // Repetition, optionally behind one Named — with no Dynamic
        // atoms, one dirty region, and a previous tree to graft from.
        let splice = if fresh {
            None
        } else {
            let single_edit = self.dirty_tracker.regions().len() == 1;
            let has_dynamic = self
                .grammar
                .atoms
                .iter()
                .any(|a| matches!(a, Atom::Dynamic { .. }));
            let rep = match self.grammar.get_atom(root_atom as usize) {
                Some(Atom::Repetition { atom, max, tag, .. }) => Some((*atom, *max, *tag, None)),
                Some(Atom::Named { atom, name, .. }) => match self.grammar.get_atom(*atom) {
                    Some(Atom::Repetition { atom, max, tag, .. }) => {
                        Some((*atom, *max, *tag, Some(name.clone())))
                    }
                    _ => None,
                },
                _ => None,
            };
            let prev_root = self.last_root.clone();
            match (single_edit, has_dynamic, rep, prev_root) {
                (true, false, Some((body, max, tag, name)), Some(root)) => {
                    Some((body, max, tag, name, root))
                }
                _ => None,
            }
        };

        // The generalized splice plan (rs#542 slice 2b): the spine
        // sits BELOW the root. The previous tree navigates the
        // structural path (portable::spine) down to the spine array;
        // everything here is clone-out before the parser takes the
        // arena. Any mismatch aborts to the full parse — sound.
        struct PathPlan {
            body: usize,
            items_prev: Vec<AstNode>,
            leading_siblings: Vec<Vec<AstNode>>,
        }
        let mut path_plan: Option<PathPlan> = None;
        let mut spine_kept: Option<crate::portable::spine::Spine> = None;
        if !fresh
            && self.dirty_tracker.regions().len() == 1
            && !self
                .grammar
                .atoms
                .iter()
                .any(|a| matches!(a, Atom::Dynamic { .. }))
            && splice.is_none()
        {
            let spine = crate::portable::spine::discover_spine(&self.grammar);
            if let Some(spine) = spine {
                if !spine.path.is_empty() {
                    let mut current = self.last_root.clone();
                    let mut leading_siblings: Vec<Vec<AstNode>> = Vec::new();
                    let mut ok = current.is_some();
                    for step in &spine.path {
                        let node = current.take();
                        let node_ref = node.as_ref();
                        match (step, node_ref) {
                            (
                                crate::portable::spine::SpineStep::Named { name },
                                Some(AstNode::Hash { pool_index, length }),
                            ) => {
                                let items = self
                                    .output_arena
                                    .get_hash_items(*pool_index as usize, *length as usize);
                                ok = items.first().is_some_and(|(k, _)| k == name);
                                current = items.into_iter().next().map(|(_, v)| v);
                            }
                            (
                                crate::portable::spine::SpineStep::Sequence { index, .. },
                                Some(AstNode::Array { pool_index, length }),
                            ) => {
                                // The walker's sequences store tagged
                                // arrays: tree slot 0 is the tag, so
                                // the atom at index i sits at slot i+1.
                                let children = self
                                    .output_arena
                                    .get_array(*pool_index as usize, *length as usize);
                                ok = *index + 1 < children.len();
                                if ok {
                                    leading_siblings.push(children[1..*index + 1].to_vec());
                                    current = Some(children[*index + 1].clone());
                                }
                            }
                            // The alternative passes its chosen
                            // branch's value through untagged — no
                            // wrapper to navigate; the branch is
                            // confirmed by the next step's key check.
                            // (The node was already taken: put it back.)
                            (crate::portable::spine::SpineStep::Alternative { .. }, _) => {
                                current = node_ref.cloned();
                            }
                            _ => {
                                ok = false;
                                current = None;
                            }
                        }
                        if !ok {
                            break;
                        }
                    }
                    // Landed: the spine items array (tagged — the tag
                    // rides slot 0).
                    let items_prev: Vec<AstNode> = match current {
                        Some(AstNode::Array { pool_index, length }) => self
                            .output_arena
                            .get_array(pool_index as usize, length as usize)
                            .into_iter()
                            .skip(1)
                            .collect(),
                        _ => Vec::new(),
                    };
                    if ok && !items_prev.is_empty() {
                        path_plan = Some(PathPlan {
                            body: spine.body,
                            items_prev,
                            leading_siblings,
                        });
                        spine_kept = Some(spine);
                    }
                }
            }
        }

        // The repetition-spine body (root Repetition, optionally behind
        // one Named) — none only when the grammar has no spine. The
        // splice path parses its suffix; every parse records the root
        // repetition's iteration bounds for the next plan.
        let spine_body = match self.grammar.get_atom(root_atom as usize) {
            Some(Atom::Repetition { atom, .. }) => Some(*atom),
            Some(Atom::Named { atom, .. }) => match self.grammar.get_atom(*atom) {
                Some(Atom::Repetition { atom, .. }) => Some(*atom),
                _ => None,
            },
            _ => None,
        };

        self.output_arena.set_input(input.to_owned());

        // Graft source: the previous root's items (same persistent arena
        // — resolving them is a Vec copy of small nodes).
        let graft: Option<(Vec<AstNode>, usize)> =
            splice.as_ref().and_then(|(_, _, _, _, root)| {
                // The root node is the Named wrapper's hash {name: [items]}
                // (or a bare tagged Array when the repetition is unnamed);
                // the graft source is the inner item array.
                let array = match root {
                    AstNode::Array { .. } => root.clone(),
                    AstNode::Hash { pool_index, length } => {
                        let items = self
                            .output_arena
                            .get_hash_items(*pool_index as usize, *length as usize);
                        items.first().map(|(_, v)| v.clone())?
                    }
                    _ => return None,
                };
                match array {
                    AstNode::Array { pool_index, length } => Some((
                        self.output_arena
                            .get_array(pool_index as usize, length as usize),
                        length as usize,
                    )),
                    _ => None,
                }
            });

        // The previous parse's body-item boundaries (pos, end), collected
        // during the last retention pass.
        let chain = std::mem::take(&mut self.body_chain);
        let splice_plan = splice.as_ref().and_then(|(_, max, _tag, _, _)| {
            // Chain-walk the consecutive item boundaries from 0 that end
            // before the edit: the reusable prefix is exactly k items.
            let mut p = 0usize;
            let mut k = 0usize;
            for (pos, end) in &chain {
                let (pos, end) = (*pos as usize, *end as usize);
                if end >= cutoff {
                    break;
                }
                if pos == p {
                    p = end;
                    k += 1;
                } else if pos > p {
                    break;
                }
            }
            (k > 0).then_some((p, k, *max))
        });

        let mut parser = super::parser::PortableParser::new_with_cache_in_place(
            &self.grammar,
            input,
            &mut self.output_arena,
            std::mem::take(&mut self.cache),
        );
        // The root repetition records its iteration bounds directly
        // (rs#258): repetition-body memo entries cannot serve the next
        // splice plan, because outcomes spanning a trivia drain are
        // never stored, so the cache's body-entry set silently thins
        // out as the document grows. One (start, end) pair per root
        // iteration is exact at O(count) cost. The watch keys on the
        // spine starting at byte 0 (the splice shape's own premise);
        // a root offset by an entry wrapper's leading trivia simply
        // keeps the full-parse fallback.
        parser.spine_watch = spine_body.map(|b| (b, 0));
        parser.spine_watch_bodies = self.spine_bodies.clone();

        // The splice: parse ONLY the suffix iterations of the body atom
        // from the last intact item boundary; rebuild the root by
        // grafting the k untouched prefix items (same-arena nodes, O(1)
        // each) in front of the new ones. Any mismatch — chain broken,
        // suffix not consuming to EOF, budget — falls back to the full
        // parse below with the same parser (its memo entries from the
        // suffix attempts only help).
        let mut spliced: Option<(&str, Vec<AstNode>)> = None;
        let mut root_name: Option<String> = None;
        let mut splice_k = 0usize;
        if let (
            Some((body, _, tag, name, _)),
            Some((splice_pos, k, max)),
            Some((prev_items, prev_len)),
        ) = (splice.as_ref(), splice_plan.as_ref(), graft.as_ref())
        {
            if *k <= *prev_len {
                root_name = name.clone();
                let tag_str = match tag {
                    RepetitionTag::Maybe => ":maybe",
                    RepetitionTag::Repetition => ":repetition",
                };
                let mut suffix: Vec<AstNode> = Vec::new();
                let mut suffix_bounds: Vec<(u32, u32)> = Vec::new();
                let mut s_pos = *splice_pos;
                loop {
                    if let Some(mx) = max {
                        if k + suffix.len() >= *mx {
                            break;
                        }
                    }
                    match parser.try_atom_impl(*body, s_pos, false) {
                        Ok(r) => {
                            let zero_width = r.end_pos == s_pos;
                            suffix_bounds.push((s_pos as u32, r.end_pos as u32));
                            suffix.push(r.value.clone());
                            s_pos = r.end_pos;
                            // Same no-progress rule as the repetition
                            // guard: count one empty match, stop.
                            if zero_width {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
                if s_pos == input.len() {
                    // prev_items[0] is the array's tag; items live at
                    // slots 1..=k. store_tagged_array re-prepends the tag.
                    let mut items: Vec<AstNode> =
                        prev_items.iter().take(k + 1).skip(1).cloned().collect();
                    items.extend(suffix);
                    splice_k = *k;
                    // The next plan's chain: the untouched prefix keeps
                    // its recorded bounds; the suffix items start fresh.
                    self.last_spine_trace = suffix_bounds;
                    spliced = Some((tag_str, items));
                }
            }
        }

        // The generalized splice (rs#542 slice 2b): the v1 root-splice
        // did not engage, but the spine sits below the root. Probe-drain
        // the outer list's bounds against the warm memo, split at the
        // edit, replay the suffix items, parse the path's sibling tails
        // fresh. Any mismatch yields None — the full-parse fallback,
        // never speculative.
        let mut path_data: Option<SpliceOutcome> = None;
        if spliced.is_none() {
            if let (Some(plan), Some(spine)) = (path_plan.as_ref(), spine_kept.as_ref()) {
                let body = plan.body;
                // Bounds from the PERSISTED chain: seeded by one drain
                // after a full parse (the body atom carries no memo —
                // late-flattened atoms hold none — so bounds cannot
                // come from the cache). Each keystroke shifts the
                // chain (below unchanged, above += delta) and probes
                // ONCE at the bridge: the edited item's new span.
                let mut bounds: Vec<(usize, usize)> = Vec::new();
                let mut straddler_old: Option<(usize, usize)> = None;
                let mut deep: Option<DeepPatch> = None;
                let mut bridge: Option<AstNode> = None;
                let mut bridge_end: Option<usize> = None;

                if self.path_chain.is_empty() {
                    // Seed from the last FULL parse's spine records —
                    // exact bounds, no scan (a scan probing the body
                    // AT an interior anchor byte fails bodies that
                    // start before their anchor). The run pairs to the
                    // tree walk by item count: the path's node carries
                    // one item per iteration.
                    // The run must SPAN TO THE INPUT'S END: recorded
                    // runs include failed backtrack attempts, whose
                    // item chains stop mid-document; the splice's own
                    // premise is a list covering the input.
                    let input_len = input.len();
                    let spans_to_end = |items: &[(u32, u32)]| {
                        items
                            .last()
                            .is_some_and(|b| (b.1 as i64 - input_len as i64).abs() <= 64)
                    };
                    // Recorded runs include failed backtrack attempts
                    // whose chains stop mid-document; the successful
                    // interpretation consumes the most, so among the
                    // count-matching candidates the LONGEST chain wins.
                    let pick = self
                        .spine_runs
                        .iter()
                        .filter(|(b, _, _, items)| {
                            *b == body
                                && items.len() == plan.items_prev.len()
                                && items.len() > 1
                                && spans_to_end(items)
                        })
                        .max_by_key(|(_, _, depth, items)| {
                            (
                                usize::from(*depth == 1),
                                items.last().map(|b| b.1).unwrap_or(0),
                            )
                        });
                    // The seed lands in the CHAIN (last-full-parse
                    // coordinates); the split below shifts it — the
                    // bounds it would seed directly are PRE-EDIT.
                    if let Some((_, _, _, items)) = pick {
                        self.path_chain = items.clone();
                    }
                }
                if !self.path_chain.is_empty() {
                    if let Some((edit_start, edit_end_old, delta)) = self.last_edit {
                        // Below the edit: unchanged. Above: shifted. The
                        // straddling item gets ONE bridge probe. An edit
                        // landing before the first item (inside the
                        // container's leading siblings) falls back: the
                        // cloned leading siblings would be stale.
                        if let Some(&(first_p, _)) = self.path_chain.first() {
                            if cutoff < first_p as usize {
                                self.path_chain.clear();
                            }
                        }
                        if !self.path_chain.is_empty() {
                            let mut below: Vec<(usize, usize)> = Vec::new();
                            let mut above: Vec<(usize, usize)> = Vec::new();
                            let mut straddler = None;
                            for &(p, e) in &self.path_chain {
                                let (p, e) = (p as usize, e as usize);
                                if e <= edit_start {
                                    below.push((p, e));
                                } else if (p as i64) >= edit_end_old as i64 {
                                    above.push((
                                        ((p as i64 + delta).max(0)) as usize,
                                        ((e as i64 + delta).max(0)) as usize,
                                    ));
                                } else if straddler.is_none() {
                                    // The item spanning the edit.
                                    straddler = Some((p, e));
                                }
                            }
                            match straddler {
                                Some((p0, e0)) => {
                                    straddler_old = Some((p0, e0));
                                    // Multi-level splice (rs#542): when a
                                    // SMALLER spine run nested inside the
                                    // straddling item contains the edit,
                                    // bridge THERE — one small item parse —
                                    // and patch the containers up to the
                                    // level-0 item surgically (tree copies,
                                    // zero parsing). The enclosing member
                                    // never re-parses.
                                    let mut edit_data = self.last_edit;
                                    if let Some((edit_start_l, edit_end_old, delta)) = edit_data {
                                        let _ = edit_start_l;
                                        // The recorded runs include failed
                                        // backtrack attempts — a candidate
                                        // must match the PREVIOUS TREE:
                                        // walk each spine body's tail once
                                        // to the actual deep list node and
                                        // keep runs whose item count
                                        // equals the node's.
                                        let prev_item = plan.items_prev[below.len()].clone();
                                        let mut body_plans: std::collections::HashMap<
                                            usize,
                                            Option<(
                                                Vec<crate::portable::spine::SpineStep>,
                                                AstNode,
                                                usize,
                                            )>,
                                        > = std::collections::HashMap::new();
                                        for b in &self.spine_bodies {
                                            let Some(&rep_d) = self.body_reps.get(b) else {
                                                continue;
                                            };
                                            let Some(steps) = crate::portable::spine::path_between(
                                                &self.grammar,
                                                body,
                                                rep_d,
                                            ) else {
                                                continue;
                                            };
                                            let Some(containers) =
                                                walk_containers(parser.arena(), &prev_item, &steps)
                                            else {
                                                continue;
                                            };
                                            let node =
                                                containers.last().and_then(|(step, cnode)| {
                                                    match step {
                                                crate::portable::spine::SpineStep::Sequence {
                                                    index,
                                                    ..
                                                } => {
                                                    if let AstNode::Array {
                                                        pool_index,
                                                        length,
                                                    } = cnode
                                                    {
                                                        parser.arena().get_array(
                                                            *pool_index as usize,
                                                            *length as usize,
                                                        )
                                                        .get(*index + 1)
                                                        .cloned()
                                                    } else {
                                                        None
                                                    }
                                                }
                                                _ => None,
                                            }
                                                });
                                            if let Some(AstNode::Array { pool_index, length }) =
                                                &node
                                            {
                                                let count = parser
                                                    .arena()
                                                    .get_array(
                                                        *pool_index as usize,
                                                        *length as usize,
                                                    )
                                                    .len()
                                                    .saturating_sub(1);
                                                let list_node = node.clone().expect("node");
                                                body_plans
                                                    .insert(*b, Some((steps, list_node, count)));
                                            }
                                        }
                                        // Candidate runs: contain the edit,
                                        // match the tree's item count,
                                        // strictly smaller straddling item
                                        // than level 0's.
                                        let mut best: Option<(usize, usize, usize, DeepCandidate)> =
                                            None;
                                        for (ri, (b, rs, _depth, items)) in
                                            self.spine_runs.iter().enumerate()
                                        {
                                            let Some(Some((
                                                tail_steps,
                                                prev_deep_list,
                                                tree_count,
                                            ))) = body_plans.get(b)
                                            else {
                                                continue;
                                            };
                                            if *tree_count != items.len() {
                                                continue;
                                            }
                                            let items_us: Vec<(usize, usize)> = items
                                                .iter()
                                                .map(|&(p, e)| (p as usize, e as usize))
                                                .collect();
                                            let run_end = items_us.last().map(|x| x.1).unwrap_or(0);
                                            if (*rs as usize) > edit_start || run_end <= edit_start
                                            {
                                                continue;
                                            }
                                            let mut below_d: Vec<(usize, usize)> = Vec::new();
                                            let mut above_d: Vec<(usize, usize)> = Vec::new();
                                            let mut str_d: Option<(usize, usize)> = None;
                                            for &(p, e) in &items_us {
                                                if e <= edit_start {
                                                    below_d.push((p, e));
                                                } else if (p as i64) >= edit_end_old as i64 {
                                                    above_d.push((
                                                        ((p as i64 + delta).max(0)) as usize,
                                                        ((e as i64 + delta).max(0)) as usize,
                                                    ));
                                                } else if str_d.is_none() {
                                                    str_d = Some((p, e));
                                                }
                                            }
                                            let (Some((pd, ed)), true) = (
                                                str_d,
                                                below_d.len() + above_d.len() + 1 == items_us.len(),
                                            ) else {
                                                continue;
                                            };
                                            let span = ed.saturating_sub(pd);
                                            if span >= e0 - p0 {
                                                continue;
                                            }
                                            let better = best
                                                .as_ref()
                                                .is_none_or(|(_, _, _, c)| c.span <= span);
                                            if better {
                                                best = Some((
                                                    ri,
                                                    *b,
                                                    below_d.len(),
                                                    DeepCandidate {
                                                        item_start: pd,
                                                        item_end_old: ed,
                                                        span,
                                                        below: below_d.clone(),
                                                        above_shifted: above_d.clone(),
                                                        tail_steps: tail_steps.clone(),
                                                        prev_deep_list: prev_deep_list.clone(),
                                                    },
                                                ));
                                            }
                                        }
                                        let _ = &mut edit_data;
                                        if let Some((
                                            ri,
                                            body_d,
                                            below_count_d,
                                            DeepCandidate {
                                                item_start: pd,
                                                item_end_old: ed,
                                                span: _,
                                                below: below_d,
                                                above_shifted: above_d,
                                                tail_steps,
                                                prev_deep_list,
                                            },
                                        )) = best
                                        {
                                            // ONE small parse: the edited
                                            // item of the nested list. The
                                            // shift identity guards it like
                                            // the level-0 bridge.
                                            let probed_d = parser.try_atom_impl(body_d, pd, false);
                                            let deep_ok = probed_d.as_ref().is_ok_and(|r| {
                                                r.end_pos as i64 == ed as i64 + delta
                                                    && r.end_pos > pd
                                            });
                                            if deep_ok {
                                                let r = probed_d.as_ref().expect("deep_ok");
                                                let mut new_run_items: Vec<(u32, u32)> = below_d
                                                    .iter()
                                                    .map(|&(p, e)| (p as u32, e as u32))
                                                    .collect();
                                                new_run_items.push((pd as u32, r.end_pos as u32));
                                                new_run_items.extend(
                                                    above_d
                                                        .iter()
                                                        .map(|&(p, e)| (p as u32, e as u32)),
                                                );
                                                deep = Some(DeepPatch {
                                                    bridge_node: r.value.clone(),
                                                    prev_deep_list,
                                                    prev_containers: {
                                                        // The containers walk
                                                        // rides along: rebuild
                                                        // it from the stored
                                                        // steps.
                                                        walk_containers(
                                                            parser.arena(),
                                                            &prev_item,
                                                            &tail_steps,
                                                        )
                                                        .unwrap_or_default()
                                                    },
                                                    below_d_count: below_count_d,
                                                    new_run_items,
                                                    run_idx: ri,
                                                });
                                                // Level-0 bounds without any
                                                // parse: the shift identity
                                                // gives the patched item's
                                                // new end.
                                                let new_e0 = ((e0 as i64 + delta).max(0)) as usize;
                                                bounds = below.clone();
                                                bounds.push((p0, new_e0));
                                                bounds.extend(above.clone());
                                                bridge_end = Some(new_e0);
                                            }
                                        }
                                    }
                                    if deep.is_none() {
                                        // The level-0 bridge: ONE parse of
                                        // the straddling item — the memo
                                        // inside it serves everything except
                                        // the edited bytes. The splice's
                                        // precondition: the bytes above the
                                        // edit are unchanged, so the item's
                                        // new end MUST be old_end + delta —
                                        // any other outcome means the item's
                                        // shape changed and a shifted-copy
                                        // rebuild would misalign.
                                        let probed = parser.try_atom_impl(body, p0, false);
                                        let expect_end = (e0 as i64 + delta).max(0) as usize;
                                        if let Ok(r) = probed {
                                            if r.end_pos == expect_end {
                                                bounds = below.clone();
                                                bounds.push((p0, r.end_pos));
                                                bounds.extend(above.clone());
                                            }
                                        }
                                    }
                                }
                                None => {
                                    // No straddler: the edit sits in the
                                    // trailing tail region — the tails
                                    // parse fresh below, so the bounds
                                    // are the unbroken below-chain.
                                    bounds = below.clone();
                                }
                            }
                        }
                    }
                }
                // The probe and the previous tree must agree on the
                // item count (the tree carries one item per iteration).
                if !bounds.is_empty() && bounds.len() == plan.items_prev.len() {
                    // The below count = the straddler's index: items
                    // [..k] clone, item k = the bridge (or the deep
                    // surgery's patched item), items [k+1..] = shifted
                    // deep copies (zero parsing).
                    let k = bounds.iter().take_while(|(_, end)| *end < cutoff).count();
                    let splice_pos = if k > 0 { bounds[k - 1].1 } else { bounds[0].0 };
                    let _ = splice_pos;
                    if deep.is_none() && bridge.is_none() && k < plan.items_prev.len() {
                        // No deep surgery ran and no level-0 bridge
                        // yet (e.g. a no-straddler edit): parse the
                        // item after the cut point as the bridge.
                        let bridged = parser.try_atom_impl(body, splice_pos, false);
                        if let Ok(r) = bridged {
                            if r.end_pos > splice_pos {
                                bridge = Some(r.value.clone());
                                bridge_end = Some(r.end_pos);
                            }
                        }
                    }
                    // The path's sibling tails parse from the END OF
                    // THE REBUILT LIST (the last bound's new end) —
                    // the atoms after the repetition — not from the
                    // bridge end (a mid-list edit leaves siblings
                    // between the bridge and the list's tail).
                    let s_pos = bounds.last().map(|b| b.1).unwrap_or(splice_pos);
                    let mut tails: Vec<Vec<AstNode>> = Vec::new();
                    let mut tail_cursor = s_pos;
                    let mut tails_ok = true;
                    for step in spine.path.iter().rev() {
                        if let crate::portable::spine::SpineStep::Sequence {
                            siblings_after, ..
                        } = step
                        {
                            let mut vals = Vec::new();
                            for &atom in siblings_after.iter() {
                                match parser.try_atom_impl(atom, tail_cursor, false) {
                                    Ok(r) => {
                                        vals.push(r.value.clone());
                                        tail_cursor = r.end_pos;
                                    }
                                    Err(_) => {
                                        tails_ok = false;
                                        break;
                                    }
                                }
                            }
                            tails.push(vals);
                        }
                        if !tails_ok {
                            break;
                        }
                    }
                    let bridge_ok =
                        bridge.is_some() || deep.is_some() || k == plan.items_prev.len();
                    // The shift identity applies to the bridge's own
                    // span as well: bytes above the edit are unchanged,
                    // so the straddling item's new end must be
                    // old_end + delta. A different outcome means the
                    // item's shape changed — the splice must not run.
                    let shift_ok = match (bridge_end, straddler_old, self.last_edit) {
                        (Some(bend), Some((_, e_old)), Some((_, _, d))) => {
                            bend as i64 == e_old as i64 + d
                        }
                        _ => true,
                    };
                    if tails_ok && bridge_ok && shift_ok && tail_cursor == input.len() {
                        // The chain persists in the new coordinates:
                        // the next keystroke derives from it without a
                        // drain. The spine runs follow the same
                        // coordinates (the deep candidates read them
                        // next keystroke) — shift every bound above
                        // the edit on every splice.
                        self.path_chain =
                            bounds.iter().map(|&(p, e)| (p as u32, e as u32)).collect();
                        let (e_old, d) = self
                            .last_edit
                            .map(|(_, e, d)| (e as usize, d))
                            .unwrap_or((0, 0));
                        for (_, _, _, items) in &mut self.spine_runs {
                            for bnd in items.iter_mut() {
                                if (bnd.0 as i64) >= e_old as i64 {
                                    bnd.0 = (bnd.0 as i64 + d) as u32;
                                    bnd.1 = (bnd.1 as i64 + d) as u32;
                                }
                            }
                        }
                        if deep.is_none() {
                            self.spine_runs.clear();
                        }
                        path_data = Some(SpliceOutcome {
                            k,
                            bridge: bridge.clone().unwrap_or(AstNode::Nil),
                            tails,
                            deep,
                        });
                    }
                }
            }
        }

        let was_spliced = spliced.is_some() || path_data.is_some();
        let result = if let Some(SpliceOutcome {
            k,
            bridge,
            tails,
            deep,
        }) = path_data
        {
            // Consume the parser first so the arena borrow ends (the
            // v1 splice's discipline), then rebuild walking the path
            // in reverse.
            let plan = path_plan.as_ref().expect("path data implies a plan");
            let spine = spine_kept.as_ref().expect("path data implies a spine");
            self.cache = parser.into_cache();
            self.spliced_parses += 1;
            let delta = self.last_edit.map(|(_, _, d)| d).unwrap_or(0);
            // The deep surgery runs with the session arena back: the
            // deep list's items come from the previous tree, the
            // bridge rides in from the derivation probe.
            let bridge = match deep.as_ref() {
                Some(dp) => {
                    self.spliced_deep += 1;
                    self.deep_patch(dp, delta)
                }
                None => bridge,
            };
            let mut suffix: Vec<AstNode> = Vec::new();
            if k < plan.items_prev.len() {
                suffix.push(bridge);
                // The items above the bridge: shifted deep copies of
                // the previous trees — no parsing, O(nodes).
                for item in &plan.items_prev[k + 1..] {
                    suffix.push(copy_shift_tree(item, delta, &mut self.output_arena));
                }
            }
            let mut all: Vec<AstNode> = plan.items_prev.iter().take(k).cloned().collect();
            all.extend(suffix);
            let (pool, len) = self.output_arena.store_tagged_array(":repetition", &all);
            let mut inner = AstNode::Array {
                pool_index: pool,
                length: len,
            };
            // leading_siblings was collected in path order; the
            // reverse walk consumes them back-to-front.
            let seq_count = plan.leading_siblings.len();
            let mut rev_seq = 0usize;
            for step in spine.path.iter().rev() {
                match step {
                    crate::portable::spine::SpineStep::Sequence { .. } => {
                        let before = &plan.leading_siblings[seq_count - 1 - rev_seq];
                        let after = &tails[rev_seq];
                        rev_seq += 1;
                        let mut children: Vec<AstNode> = before.to_vec();
                        children.push(inner);
                        children.extend(after.iter().cloned());
                        // Tagged, like the walker's sequence values —
                        // the tag rides slot 0.
                        let (p, l) = self.output_arena.store_tagged_array(":sequence", &children);
                        inner = AstNode::Array {
                            pool_index: p,
                            length: l,
                        };
                    }
                    crate::portable::spine::SpineStep::Named { name } => {
                        let (hp, hl) = self.output_arena.store_hash(&[(name, inner)]);
                        inner = AstNode::Hash {
                            pool_index: hp,
                            length: hl,
                        };
                    }
                    crate::portable::spine::SpineStep::Alternative { .. } => {}
                }
            }
            Ok(inner)
        } else {
            match spliced {
                Some((tag_str, items)) => {
                    // Consume the parser FIRST so the arena borrow ends,
                    // then rebuild the root from grafted + new items.
                    self.cache = parser.into_cache();
                    self.spliced_parses += 1;
                    let (pool_idx, len) = self.output_arena.store_tagged_array(tag_str, &items);
                    let array = AstNode::Array {
                        pool_index: pool_idx,
                        length: len,
                    };
                    match root_name.as_deref() {
                        Some(name) => {
                            let (hp, hl) = self.output_arena.store_hash(&[(name, array)]);
                            Ok(AstNode::Hash {
                                pool_index: hp,
                                length: hl,
                            })
                        }
                        None => Ok(array),
                    }
                }
                None => {
                    let r = parser.parse();
                    self.last_spine_trace = std::mem::take(&mut parser.spine_trace);
                    // The fallback leaves a VALID chain: the drain here
                    // is all memo hits (the full parse just cached
                    // every body-atom entry — 540 on doc3), so the
                    // next keystroke splices instead of re-draining
                    // cold. A cold drain after the clear cost a full
                    // re-parse and doubled every fallback.
                    self.spine_runs = std::mem::take(&mut parser.spine_records);
                    let spine_owned = spine_kept
                        .is_none()
                        .then(|| crate::portable::spine::discover_spine(&self.grammar))
                        .flatten();
                    if let Some(spine) = spine_kept.as_ref().or(spine_owned.as_ref()) {
                        let body = spine.body;
                        let want = path_plan.as_ref().map(|p| p.items_prev.len()).unwrap_or(0);
                        // Pair to the tree walk FIRST (the path's
                        // node carries one item per iteration — a
                        // count match is the identity), then any
                        // top-level run. The run must also span to
                        // the input's end: recorded runs include
                        // failed backtrack attempts, whose chains
                        // stop mid-document.
                        let input_len = input.len();
                        let spans_to_end = |items: &[(u32, u32)]| {
                            items
                                .last()
                                .is_some_and(|b| (b.1 as i64 - input_len as i64).abs() <= 64)
                        };
                        // The successful interpretation consumes the
                        // most: among count-matching candidates the
                        // LONGEST chain wins (failed backtrack
                        // attempts record first, shorter).
                        let pick = self
                            .spine_runs
                            .iter()
                            .filter(|(b, _, _, items)| {
                                *b == body && want > 1 && items.len() == want && spans_to_end(items)
                            })
                            .max_by_key(|(_, _, depth, items)| {
                                (
                                    usize::from(*depth == 1),
                                    items.last().map(|b| b.1).unwrap_or(0),
                                )
                            });
                        match pick {
                            Some((_, _, _, items)) => self.path_chain = items.clone(),
                            None => self.path_chain.clear(),
                        }
                    } else {
                        self.path_chain.clear();
                    }
                    self.cache = parser.into_cache();
                    r
                }
            }
        };
        let post_parse = self.cache.len();

        // Retention keeps every success fully before the edit cutoff:
        // the arena is persistent, so pool-backed container entries
        // need no adoption (an identity "adopt" just marks them
        // snapshot, which exempts them from arena-generation checks).
        // Failures stay never-retained, exactly like the snapshot
        // tier.
        // The repetition-body item boundaries for the NEXT splice plan
        // come straight from the root repetition's trace (full parses)
        // or from the grafted prefix plus the suffix bounds (splices):
        // the memo cache cannot serve this, because outcomes spanning
        // a trivia drain are never stored, so its body-entry set
        // silently thins out as the document grows (rs#258).
        if was_spliced {
            let mut next_chain: Vec<(u32, u32)> = chain.iter().take(splice_k).cloned().collect();
            next_chain.append(&mut self.last_spine_trace);
            self.body_chain = next_chain;
        } else if spine_body.is_some() {
            self.body_chain = std::mem::take(&mut self.last_spine_trace);
        }
        let suffix_floor = self.applied_suffix_floor;
        self.cache.retain_snapshot_adopt(
            |entry| {
                let entry_end = entry.end_pos as usize;
                let is_root_at_start = entry.pos == 0 && entry.atom_id == root_atom;
                let below =
                    entry.success && entry_end < cutoff && !(input_len_changed && is_root_at_start);
                // Fresh suffix entries the parse just inserted sit
                // above the edit in the NEW coordinates — they are as
                // valid as the below-window ones, and dropping them
                // re-collapsed the window every keystroke (the doc3
                // reused-0 tail of rs#542's slice-1 measurement).
                let above = entry.success && entry.pos as usize >= suffix_floor;
                below || above
            },
            |node| node.clone(),
        );
        // The capacity-wide maintenance (the adopt remap + hole repair,
        // plus the suffix shift's slot rebuilds) scales with the
        // SESSION-START table; shrink to the survivors so every later
        // keystroke's maintenance is O(window), not O(2M slots) — the
        // rs#464 late-band floor, now on the retained tier too.
        self.cache.shrink_to_entries();
        self.dirty_tracker.clear();

        if let Ok(ast) = &result {
            self.last_root = Some(ast.clone());
        }
        (result, before, post_parse)
    }

    /// Retained variant of [`Self::finish_parse`].
    fn finish_retained(&mut self, input: &str) -> Result<IncrementalResult, ParseError> {
        let (result, _retained_before, post_parse) = self.retained_parse(input, false);
        Ok(IncrementalResult {
            ast: result?,
            reused_cache_entries: self.cache.len(),
            invalidated_cache_entries: post_parse - self.cache.len(),
        })
    }

    /// Execute the nested-run surgery: rebuild the deep list (below
    /// clones | the deep bridge | shifted above), reassemble the
    /// containers up to the level-0 item, refresh every spine run's
    /// bounds, and hand back the patched item tree.
    fn deep_patch(&mut self, dp: &DeepPatch, delta: i64) -> AstNode {
        let edit_end_old = self.last_edit.map(|(_, e, _)| e as usize).unwrap_or(0);
        // get_array slots include the tag at slot 0 — the items are
        // slots 1..; store_tagged_array adds its own tag back.
        let prev_items: Vec<AstNode> = match &dp.prev_deep_list {
            AstNode::Array { pool_index, length } => self
                .output_arena
                .get_array(*pool_index as usize, *length as usize)
                .into_iter()
                .skip(1)
                .collect(),
            _ => Vec::new(),
        };
        let bd = dp.below_d_count.min(prev_items.len());
        let mut deep_items: Vec<AstNode> = prev_items[..bd].to_vec();
        deep_items.push(dp.bridge_node.clone());
        for item in &prev_items[bd + 1..] {
            deep_items.push(copy_shift_tree(item, delta, &mut self.output_arena));
        }
        let (pl, ln) = self
            .output_arena
            .store_tagged_array(":repetition", &deep_items);
        let mut inner = AstNode::Array {
            pool_index: pl,
            length: ln,
        };
        for (step, prev_node) in dp.prev_containers.iter().rev() {
            match step {
                crate::portable::spine::SpineStep::Named { name } => {
                    let (hp, hl) = self.output_arena.store_hash(&[(name.as_str(), inner)]);
                    inner = AstNode::Hash {
                        pool_index: hp,
                        length: hl,
                    };
                }
                crate::portable::spine::SpineStep::Sequence { index, .. } => {
                    if let AstNode::Array { pool_index, length } = prev_node {
                        // Slots include the tag — the atoms are
                        // slots 1..; the grammar's atom `index` sits
                        // at atom-position `index` in the tag-less
                        // list.
                        let children: Vec<AstNode> = self
                            .output_arena
                            .get_array(*pool_index as usize, *length as usize)
                            .into_iter()
                            .skip(1)
                            .collect();
                        let mut rebuilt: Vec<AstNode> = Vec::with_capacity(children.len());
                        for (jc, ch) in children.iter().enumerate() {
                            rebuilt.push(if jc == *index {
                                inner.clone()
                            } else {
                                shift_if_above(ch, edit_end_old, delta, &mut self.output_arena)
                            });
                        }
                        let (pl2, ln2) =
                            self.output_arena.store_tagged_array(":sequence", &rebuilt);
                        inner = AstNode::Array {
                            pool_index: pl2,
                            length: ln2,
                        };
                    }
                }
                crate::portable::spine::SpineStep::Alternative { .. } => {}
            }
        }
        // The edited deep run takes its freshly derived items (the
        // gate's wide shift already moved every OTHER run).
        if dp.run_idx < self.spine_runs.len() {
            self.spine_runs[dp.run_idx].3 = dp.new_run_items.clone();
        }
        inner
    }

    /// Number of retained parses rebuilt via the splice path.
    #[inline]
    pub fn spliced_parses(&self) -> usize {
        self.spliced_parses
    }

    /// Number of retained parses rebuilt via the nested-run surgery.
    #[inline]
    pub fn spliced_deep(&self) -> usize {
        self.spliced_deep
    }

    /// Get cache statistics
    #[inline]
    pub fn cache_stats(&self) -> (u64, u64, f64) {
        self.cache.stats()
    }

    /// Get the number of dirty regions
    #[inline]
    pub fn dirty_region_count(&self) -> usize {
        self.dirty_tracker.regions().len()
    }

    /// Clear all cached state
    pub fn clear(&mut self) {
        self.cache.clear();
        self.snapshot_arena = AstArena::new();
        self.output_arena = AstArena::new();
        self.last_root = None;
        self.body_chain.clear();
        self.path_chain.clear();
        self.spliced_parses = 0;
        self.dirty_tracker.clear();
        self.prev_input_len = 0;
    }
}

/// Earliest dirty-region start, or `usize::MAX` when nothing is
/// dirty: the retention cutoff every retained entry must end before.
fn earliest_dirty_offset(tracker: &DirtyRegionTracker) -> usize {
    tracker
        .regions()
        .iter()
        .map(|r| r.start)
        .min()
        .unwrap_or(usize::MAX)
}

/// Result of an incremental parse
#[derive(Debug)]
pub struct IncrementalResult {
    /// The parsed AST
    pub ast: AstNode,

    /// Number of cache entries that were reused
    pub reused_cache_entries: usize,

    /// Number of cache entries that were invalidated
    pub invalidated_cache_entries: usize,
}

impl IncrementalResult {
    /// Get the efficiency ratio (0.0 = 0% reused, 1.0 = 100% reused)
    #[inline]
    pub fn efficiency(&self) -> f64 {
        let total = self.reused_cache_entries + self.invalidated_cache_entries;
        if total == 0 {
            0.0
        } else {
            self.reused_cache_entries as f64 / total as f64
        }
    }
}

#[cfg(test)]
mod splice_probe {
    use super::*;

    /// Does the walker's cache carry body-atom entries after a full
    /// parse of the json artifact? (The splice seed's cost depends on
    /// it: hits = ms, misses = a full re-parse.)
    #[test]
    fn probe_body_entries_after_full_parse() {
        let Ok(dir) = std::env::var("PARSANOL_PROBE_DIR") else {
            return;
        };
        let artifact_text =
            std::fs::read_to_string(format!("{dir}/json-artifact.json")).expect("artifact");
        let artifact = crate::parg::PargArtifact::from_json(&artifact_text).expect("loads");
        let grammar = artifact.grammar("main").expect("decodes");
        let input = std::fs::read_to_string(format!("{dir}/corpora/json/doc3.json")).expect("doc");
        let mut arena = AstArena::for_input(input.len());
        arena.set_input(input.clone());
        let mut parser = crate::portable::parser::PortableParser::new(&grammar, &input, &mut arena);
        let _ = parser.parse();
        let cache = parser.into_cache();
        let all: Vec<&crate::portable::cache::CacheEntry> = cache.entries().collect();
        let max_atom = all.iter().map(|e| e.atom_id).max().unwrap_or(0);
        println!(
            "PROBE: entries={} max_atom={max_atom} atoms={} successes={}",
            all.len(),
            grammar.atoms.len(),
            all.iter().filter(|e| e.success).count()
        );
        let spine = crate::portable::spine::discover_spine(&grammar).expect("spine");
        let count = all
            .iter()
            .filter(|e| e.success && e.atom_id as usize == spine.body)
            .count();
        println!("PROBE: body={} body_entries={count}", spine.body);
        for idx in 95..grammar.atoms.len().min(115) {
            println!(
                "PROBE: atom {idx} = {:?}",
                grammar
                    .get_atom(idx)
                    .map(|a| format!("{a:?}").chars().take(60).collect::<String>())
            );
        }
    }

    #[test]
    fn probe_placeholder() {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edit_creation() {
        let insert = Edit::insert(5, 3);
        assert_eq!(insert.offset, 5);
        assert_eq!(insert.old_length, 0);
        assert_eq!(insert.new_length, 3);
        assert_eq!(insert.delta(), 3);

        let delete = Edit::delete(5, 3);
        assert_eq!(delete.old_length, 3);
        assert_eq!(delete.new_length, 0);
        assert_eq!(delete.delta(), -3);

        let replace = Edit::replace(5, 3, 4);
        assert_eq!(replace.old_length, 3);
        assert_eq!(replace.new_length, 4);
        assert_eq!(replace.delta(), 1);
    }

    #[test]
    fn test_edit_position_translation() {
        // Insert at position 5, length 3
        // Old: positions 0-9, New: positions 0-12 (inserted 3 chars at pos 5)
        let edit = Edit::insert(5, 3);

        assert_eq!(edit.translate_position(0), 0); // Before edit: unchanged
        assert_eq!(edit.translate_position(4), 4); // Before edit: unchanged
        assert_eq!(edit.translate_position(5), 5); // At edit start: unchanged
        assert_eq!(edit.translate_position(6), 9); // After edit: shifted by delta
        assert_eq!(edit.translate_position(10), 13); // After edit: shifted by delta

        // Delete at position 5, length 3
        // Old: positions 0-9, New: positions 0-6 (deleted 3 chars at pos 5-7)
        let edit = Edit::delete(5, 3);

        assert_eq!(edit.translate_position(0), 0); // Before edit: unchanged
        assert_eq!(edit.translate_position(4), 4); // Before edit: unchanged
        assert_eq!(edit.translate_position(5), 5); // At edit start: maps to new position 5
        assert_eq!(edit.translate_position(6), 5); // Inside deleted: maps to position 5
        assert_eq!(edit.translate_position(7), 5); // Inside deleted: maps to position 5
        assert_eq!(edit.translate_position(8), 5); // At end of deleted: maps to position 5
        assert_eq!(edit.translate_position(9), 6); // After edit: shifted by delta
        assert_eq!(edit.translate_position(10), 7); // After edit: shifted by delta
    }

    #[test]
    fn test_dirty_region_tracker() {
        let mut tracker = DirtyRegionTracker::new();

        // Mark first region
        tracker.mark_dirty(DirtyRegion::new(5, 10));
        assert!(tracker.is_dirty(5));
        assert!(tracker.is_dirty(7));
        assert!(!tracker.is_dirty(4));
        assert!(!tracker.is_dirty(10));

        // Mark overlapping region (should merge)
        tracker.mark_dirty(DirtyRegion::new(8, 15));
        assert_eq!(tracker.regions().len(), 1);
        assert_eq!(tracker.regions()[0].start, 5);
        assert_eq!(tracker.regions()[0].end, 15);

        // Mark non-overlapping region
        tracker.mark_dirty(DirtyRegion::new(20, 25));
        assert_eq!(tracker.regions().len(), 2);
    }

    #[test]
    fn test_dirty_region_merge() {
        let mut tracker = DirtyRegionTracker::new();

        // Two adjacent regions should merge
        tracker.mark_dirty(DirtyRegion::new(5, 10));
        tracker.mark_dirty(DirtyRegion::new(10, 15));

        assert_eq!(tracker.regions().len(), 1);
        assert_eq!(tracker.regions()[0].start, 5);
        assert_eq!(tracker.regions()[0].end, 15);
    }

    #[test]
    fn test_dirty_region_range_check() {
        let tracker = DirtyRegionTracker::new();
        let mut t = tracker;

        t.mark_dirty(DirtyRegion::new(10, 20));

        // Non-overlapping
        assert!(!t.is_range_dirty(0, 5));
        assert!(!t.is_range_dirty(25, 30));

        // Overlapping
        assert!(t.is_range_dirty(5, 15));
        assert!(t.is_range_dirty(15, 25));
        assert!(t.is_range_dirty(0, 30));
    }

    #[test]
    fn test_incremental_result_efficiency() {
        let result = IncrementalResult {
            ast: AstNode::Nil,
            reused_cache_entries: 80,
            invalidated_cache_entries: 20,
        };

        assert!((result.efficiency() - 0.8).abs() < 0.01);
    }

    #[test]
    fn test_edit_old_range() {
        // Insert at position 5, length 3 (old_length = 0)
        let insert = Edit::insert(5, 3);
        let range = insert.old_range();
        assert_eq!(range, 5..5); // Empty range for insert

        // Delete at position 5, length 3
        let delete = Edit::delete(5, 3);
        let range = delete.old_range();
        assert_eq!(range, 5..8);

        // Replace "hello" (5 chars) with "hi" (2 chars) at position 10
        let replace = Edit::replace(10, 5, 2);
        let range = replace.old_range();
        assert_eq!(range, 10..15);
    }

    // The P2 invariant specs (PLAN-2027 program 2, the specs-first
    // step): the incremental re-parse must equal the cold parse of the
    // edited source, end to end through the real cache -- a cache
    // entry spanning the edit's cutoff can never serve a stale prefix,
    // and the retained-suffix shift keeps the tree byte-identical.

    const DOC_GRAMMAR: &str = r#"{"atoms":[{"Re":{"pattern":"[a-c]"}},{"Named":{"name":"item","atom":0}},{"Repetition":{"atom":1,"min":1,"max":null,"tag":"Repetition"}},{"Named":{"name":"doc","atom":2}},{"Str":{"pattern":"z"}},{"Sequence":{"atoms":[3,4]}},{"Entity":{"atom":5}},{"Named":{"name":"root","atom":6}}],"root":7}"#;

    fn cold_tree(grammar_json: &str, input: &str) -> String {
        let g = crate::portable::Grammar::from_json(grammar_json).unwrap();
        let mut arena = crate::portable::AstArena::for_input(input.len());
        arena.set_input(input.to_string());
        let mut p = crate::portable::PortableParser::new(&g, input, &mut arena);
        let ast = p.parse().expect("cold parse succeeds");
        format!("{ast:?}")
    }

    #[test]
    fn the_edit_sequence_incremental_tree_equals_the_cold_tree() {
        let g = crate::portable::Grammar::from_json(DOC_GRAMMAR).unwrap();
        let mut ip = IncrementalParser::new(&g);
        let input = "abcz";
        let mut arena = crate::portable::AstArena::for_input(64);
        arena.set_input(input.to_string());
        let _ = ip.parse(input, &mut arena).expect("first parse");

        // Each edit's cutoff sits inside the retained run, so the
        // strict-cutoff rule governs what the cache may serve; the
        // incremental tree must equal the cold parse of the edited
        // source every time. The sequence covers a mid-run replace, an
        // insertion, a head delete (the suffix shift), and a replace
        // after the shift.
        let sequence: Vec<(&str, crate::portable::incremental::Edit)> = vec![
            ("accz", crate::portable::incremental::Edit::replace(1, 1, 1)),
            ("abccz", crate::portable::incremental::Edit::insert(1, 1)),
            ("bccz", crate::portable::incremental::Edit::delete(0, 1)),
            ("bacz", crate::portable::incremental::Edit::replace(1, 1, 1)),
        ];
        for (next, edit) in sequence {
            let mut fresh = crate::portable::AstArena::for_input(next.len() + 16);
            fresh.set_input(next.to_string());
            let result = ip
                .parse_with_edit(next, &mut fresh, edit)
                .expect("incremental re-parse");
            assert_eq!(
                format!("{:?}", result.ast),
                cold_tree(DOC_GRAMMAR, next),
                "the incremental tree diverged from the cold parse for {next}"
            );
        }
    }

    #[test]
    fn test_edit_affects_position() {
        // Edit at position 5, length 3
        let edit = Edit::replace(5, 3, 4);

        // Positions before edit are not affected
        assert!(!edit.affects_position(0));
        assert!(!edit.affects_position(4));

        // Positions at or after edit offset are affected
        assert!(edit.affects_position(5));
        assert!(edit.affects_position(6));
        assert!(edit.affects_position(8));
        assert!(edit.affects_position(100));

        // Delete edit
        let delete = Edit::delete(10, 5);
        assert!(!delete.affects_position(9));
        assert!(delete.affects_position(10));
        assert!(delete.affects_position(15));
        assert!(delete.affects_position(100));
    }
}

#[cfg(test)]
mod edit_sequence_tests {
    use super::*;
    use crate::portable::arena::AstArena;
    use crate::portable::grammar::{Atom, Grammar};
    use crate::portable::parser::PortableParser;

    /// Line-based "key=value" grammar: repetition of
    /// seq(key, "=", value, newline).
    fn kv_grammar() -> Grammar {
        let mut g = Grammar::new();
        let key = g.add_atom(Atom::Re {
            pattern: "[a-z][a-z0-9]*".to_string(),
        });
        let val = g.add_atom(Atom::Re {
            pattern: "[0-9]+".to_string(),
        });
        let eq = g.add_atom(Atom::Str {
            pattern: "=".to_string(),
        });
        let nl = g.add_atom(Atom::Str {
            pattern: "\n".to_string(),
        });
        let pair = g.add_atom(Atom::Sequence {
            atoms: vec![key, eq, val, nl],
        });
        let root = g.add_atom(Atom::Repetition {
            atom: pair,
            min: 0,
            max: None,
            tag: crate::portable::grammar::RepetitionTag::Repetition,
        });
        g.root = root;
        g
    }

    fn doc(lines: usize) -> String {
        (0..lines)
            .map(|i| format!("key{}={}\n", i, i * 7))
            .collect()
    }

    fn walk_strings(node: &AstNode, arena: &AstArena, input: &str, out: &mut Vec<String>) {
        match node {
            AstNode::InputRef { offset, length } => {
                out.push(input[*offset as usize..*offset as usize + *length as usize].to_string())
            }
            AstNode::Array { pool_index, length } => {
                for child in arena.get_array(*pool_index as usize, *length as usize) {
                    walk_strings(&child, arena, input, out);
                }
            }
            AstNode::StringRef { pool_index } => {
                out.push(arena.get_string(*pool_index as usize).to_string())
            }
            _ => out.push(format!("{node:?}")),
        }
    }

    /// Full parse flattened to a logical string list (arena indices
    /// differ between parses; the logical tree must not).
    fn full_parse(grammar: &Grammar, input: &str) -> Option<Vec<String>> {
        let mut arena = AstArena::new();
        let mut parser = PortableParser::new(grammar, input, &mut arena);
        match parser.parse() {
            Ok(tree) => {
                let mut out = Vec::new();
                walk_strings(&tree, &arena, input, &mut out);
                Some(out)
            }
            Err(_) => None,
        }
    }

    fn flatten(node: &AstNode, arena: &AstArena, input: &str) -> Vec<String> {
        let mut out = Vec::new();
        walk_strings(node, arena, input, &mut out);
        out
    }

    /// TODO.perf/4 gate: incremental re-parses after a deterministic
    /// edit sequence must produce the same trees as full re-parses,
    /// and edits late in the document must reuse the early cache
    /// window (the whole point of the retention rule).
    #[test]
    fn incremental_matches_full_reparse_on_edit_sequences() {
        let grammar = kv_grammar();
        let mut input = doc(1200);

        let mut inc = IncrementalParser::owned(grammar.clone());
        let mut arena = AstArena::new();
        let initial = inc.parse(&input, &mut arena).expect("initial parse");
        let reference = full_parse(&grammar, &input).expect("reference parse");
        assert_eq!(flatten(&initial, &arena, &input), reference);

        // Deterministic pseudo-random edit sequence (LCG), biased to
        // line boundaries so intermediate documents stay parseable.
        let mut seed: u64 = 0x2545F4914F6CDD1D;
        let mut next = move || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) as usize
        };

        for step in 0..30 {
            let line = next() % input.lines().count().max(1);
            let offset = input
                .lines()
                .take(line)
                .map(|l| l.len() + 1)
                .sum::<usize>()
                .min(input.len());
            let edit = match next() % 3 {
                0 => {
                    // insert a line before `offset`
                    let text = format!("added{}={}\n", step, step);
                    input.insert_str(offset.min(input.len()), &text);
                    Edit::insert(offset, text.len())
                }
                1 => {
                    // delete one full line if possible
                    let rest = &input[offset.min(input.len())..];
                    let len = rest.find('\n').map(|i| i + 1).unwrap_or(0);
                    if len == 0 {
                        continue;
                    }
                    input.replace_range(offset..offset + len, "");
                    Edit::delete(offset, len)
                }
                _ => {
                    // replace a line's value digits
                    let rest = &input[offset.min(input.len())..];
                    let len = rest.find('\n').unwrap_or(rest.len());
                    if len == 0 {
                        continue;
                    }
                    let mut replacement = format!("key{}={}", next() % 1000, next() % 100000);
                    replacement.push('\n');
                    let range = offset..offset + len;
                    replacement.truncate(len);
                    input.replace_range(range.clone(), &replacement);
                    Edit::replace(offset, len, replacement.len())
                }
            };

            // Acceptance must agree: the incremental parse fails
            // exactly when a full re-parse would.
            let reference = full_parse(&grammar, &input);
            let mut arena = AstArena::new();
            let result = inc.parse_with_edit(&input, &mut arena, edit);
            match (reference, result) {
                (Some(flat), Ok(r)) => assert_eq!(
                    flatten(&r.ast, &arena, &input),
                    flat,
                    "tree mismatch at edit step {step} (line {line})"
                ),
                (None, Err(_)) => {}
                (f, r) => panic!(
                    "acceptance mismatch at step {step}: full_ok={} inc_ok={} err={r:?}",
                    f.is_some(),
                    r.is_ok()
                ),
            }
        }
    }

    /// The Ruby serializer emits named rules (Entity/Named atoms); a
    /// rule-referenced body exercises rule-boundary memo entries the
    /// flat grammar above does not.
    #[test]
    fn rule_based_grammar_edit_matches_full_parse() {
        let mut g = Grammar::new();
        let key = g.add_atom(Atom::Re {
            pattern: "[a-z][a-z0-9]*".to_string(),
        });
        let val = g.add_atom(Atom::Re {
            pattern: "[0-9]+".to_string(),
        });
        let eq = g.add_atom(Atom::Str {
            pattern: "=".to_string(),
        });
        let nl = g.add_atom(Atom::Str {
            pattern: "\n".to_string(),
        });
        let pair_body = g.add_atom(Atom::Sequence {
            atoms: vec![key, eq, val, nl],
        });
        let pair_rule = g.add_atom(Atom::Named {
            name: "pair".to_string(),
            atom: pair_body,
        });
        let root = g.add_atom(Atom::Repetition {
            atom: pair_rule,
            min: 0,
            max: None,
            tag: crate::portable::grammar::RepetitionTag::Repetition,
        });
        g.root = root;

        let doc: String = (1..200).map(|i| format!("key{}={}\n", i, i * 3)).collect();
        let mut inc = IncrementalParser::owned(g.clone());
        let mut arena = AstArena::new();
        inc.parse(&doc, &mut arena).expect("initial");

        let target = "key50=150\n";
        let offset = doc.find(target).expect("line");
        let edited = doc.replacen(target, "zed=9\n", 1);
        let mut arena = AstArena::new();
        let result = inc.parse_with_edit(
            &edited,
            &mut arena,
            Edit::replace(offset, target.len(), "zed=9\n".len()),
        );
        assert!(
            result.is_ok(),
            "rule-based incremental parse failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn late_edits_reuse_the_early_cache_window() {
        let grammar = kv_grammar();
        let input = doc(3000);
        let mut inc = IncrementalParser::owned(grammar);
        let mut arena = AstArena::new();
        inc.parse(&input, &mut arena).expect("initial parse");

        // One edit near the end: everything before it is reusable.
        let last_line_start = input.rfind('\n').map(|i| i + 1).unwrap_or(0);
        let last_len = input.len() - last_line_start;
        let edited = format!("{}zed=1\n", &input[..last_line_start]);
        let mut arena = AstArena::new();
        let result = inc
            .parse_with_edit(
                &edited,
                &mut arena,
                Edit::replace(last_line_start, last_len, 7),
            )
            .expect("incremental parse");
        assert!(
            result.reused_cache_entries > 100,
            "expected substantial reuse, got {} reused / {} invalidated",
            result.reused_cache_entries,
            result.invalidated_cache_entries
        );
    }

    #[test]
    fn suffix_entries_survive_scattered_edits_on_non_spine_roots() {
        // rs#542's window collapse: a non-spine root makes the splice
        // ineligible, and the single-cutoff policy kept ZERO entries
        // once an edit landed earlier than the previous one. The
        // suffix shift keys the entries above the edit at shifted
        // positions, so the window survives scattered edits.
        let mut g = Grammar::new();
        let key = g.add_atom(Atom::Re {
            pattern: "[a-z][a-z0-9]*".to_string(),
        });
        let val = g.add_atom(Atom::Re {
            pattern: "[0-9]+".to_string(),
        });
        let eq = g.add_atom(Atom::Str {
            pattern: "=".to_string(),
        });
        let nl = g.add_atom(Atom::Str {
            pattern: "\n".to_string(),
        });
        let pair = g.add_atom(Atom::Sequence {
            atoms: vec![key, eq, val, nl],
        });
        let pairs = g.add_atom(Atom::Repetition {
            atom: pair,
            min: 0,
            max: None,
            tag: crate::portable::grammar::RepetitionTag::Repetition,
        });
        // A one-element Sequence root: NOT a Repetition/Named(Repetition),
        // so the splice is ineligible and reuse rides the memo window.
        let root = g.add_atom(Atom::Sequence { atoms: vec![pairs] });
        g.root = root;

        let input = doc(2000);
        let mut inc = IncrementalParser::owned(g.clone());
        inc.parse_retained(&input).expect("initial parse");

        // Two scattered edits: a later line first, then an EARLIER
        // one (the old policy's collapse case). Identity must hold on
        // both, and the second (earlier) edit must still retain.
        let mut text = input.clone();
        let mut total_reused = 0;
        for line_no in [1500, 300] {
            let start = text
                .lines()
                .take(line_no)
                .map(|l| l.len() + 1)
                .sum::<usize>();
            let old_line_len = text[start..].find('\n').map(|i| i + 1).unwrap_or(0);
            let replacement = format!("zed{}=1\n", line_no);
            let next = format!(
                "{}{}{}",
                &text[..start],
                replacement,
                &text[start + old_line_len..]
            );
            let result = inc
                .parse_with_edit_retained(
                    &next,
                    Edit::replace(start, old_line_len, replacement.len()),
                )
                .expect("incremental parse");
            total_reused += result.reused_cache_entries;
            // Identity: the retained tree equals a fresh parse's tree.
            let reference = full_parse(&g, &next).expect("reference parse");
            let mut retained_strings = Vec::new();
            walk_strings(
                &result.ast,
                inc.retained_arena(),
                &next,
                &mut retained_strings,
            );
            assert_eq!(
                retained_strings, reference,
                "line {line_no}: the retained tree diverged from the fresh parse"
            );
            text = next;
        }
        assert!(
            total_reused > 100,
            "scattered edits must retain the suffix window: {total_reused}"
        );
    }
}

#[cfg(test)]
mod deep_splice_tests {
    use super::*;
    use crate::portable::arena::AstArena;
    use crate::portable::grammar::{Atom, Grammar};
    use crate::portable::parser::PortableParser;

    /// Nested member-list grammar (package-lock shape): an outer
    /// object whose `packages` member holds a list of objects — the
    /// level-0 spine is the outer member list; the nested member list
    /// inside each package is the deep spine.
    fn nested_grammar() -> Grammar {
        let mut g = Grammar::new();
        let ws_re = g.add_atom(Atom::Re {
            pattern: "[ \\n]".to_string(),
        });
        let ws = g.add_atom(Atom::Repetition {
            atom: ws_re,
            min: 0,
            max: None,
            tag: crate::portable::grammar::RepetitionTag::Repetition,
        });
        let key = g.add_atom(Atom::Re {
            pattern: "\"[a-z0-9]+\"".to_string(),
        });
        let colon = g.add_atom(Atom::Str {
            pattern: ":".to_string(),
        });
        let comma = g.add_atom(Atom::Str {
            pattern: ",".to_string(),
        });
        let lbrace = g.add_atom(Atom::Str {
            pattern: "{".to_string(),
        });
        let rbrace = g.add_atom(Atom::Str {
            pattern: "}".to_string(),
        });
        let str_val = g.add_atom(Atom::Re {
            pattern: "\"[a-z0-9-]+\"".to_string(),
        });

        // inner member: ws key ":" ws str_val  (scalar objects only —
        // the deep list sits at ONE nesting below the outer list)
        let inner_member = g.add_atom(Atom::Sequence {
            atoms: vec![ws, key, colon, ws, str_val],
        });
        let inner_body = g.add_atom(Atom::Sequence {
            atoms: vec![ws, comma, inner_member],
        });
        let inner_list = g.add_atom(Atom::Repetition {
            atom: inner_body,
            min: 0,
            max: None,
            tag: crate::portable::grammar::RepetitionTag::Repetition,
        });
        let inner_first = g.add_atom(Atom::Sequence {
            atoms: vec![inner_member, inner_list],
        });
        let inner_obj = g.add_atom(Atom::Sequence {
            atoms: vec![ws, lbrace, inner_first, ws, rbrace],
        });

        // outer member: ws key ":" ws inner_obj
        let outer_member = g.add_atom(Atom::Sequence {
            atoms: vec![ws, key, colon, ws, inner_obj],
        });
        let outer_body = g.add_atom(Atom::Sequence {
            atoms: vec![ws, comma, outer_member],
        });
        let outer_list = g.add_atom(Atom::Repetition {
            atom: outer_body,
            min: 0,
            max: None,
            tag: crate::portable::grammar::RepetitionTag::Repetition,
        });
        let outer_first = g.add_atom(Atom::Sequence {
            atoms: vec![outer_member, outer_list],
        });
        let root = g.add_atom(Atom::Sequence {
            atoms: vec![ws, lbrace, outer_first, ws, rbrace],
        });
        g.root = root;
        g
    }

    fn doc() -> String {
        // one outer member per line group; each inner object holds
        // three inner members — the outer list = the level-0 spine,
        // each inner object's list = a deep run.
        let mut out = String::from("{\n");
        for p in 0..40 {
            out.push_str("\"pkg");
            out.push_str(&p.to_string());
            out.push_str("\": {\n");
            for m in 0..3 {
                out.push_str("\"dep");
                out.push_str(&m.to_string());
                out.push_str("\": \"v");
                out.push_str(&m.to_string());
                out.push('"');
                if m < 2 {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push('}');
            if p < 39 {
                out.push(',');
            }
            out.push('\n');
        }
        out.push('}');
        out
    }

    fn full_strings(g: &Grammar, input: &str) -> Vec<String> {
        let mut arena = AstArena::new();
        let mut parser = PortableParser::new(g, input, &mut arena);
        let root = parser.parse().expect("parse");
        let mut out = Vec::new();
        walk_strings(&root, &arena, input, &mut out);
        out
    }

    fn walk_strings(node: &AstNode, arena: &AstArena, input: &str, out: &mut Vec<String>) {
        match node {
            AstNode::InputRef { offset, length } => {
                out.push(input[*offset as usize..*offset as usize + *length as usize].to_string())
            }
            AstNode::Array { pool_index, length } => {
                for child in arena.get_array(*pool_index as usize, *length as usize) {
                    walk_strings(&child, arena, input, out);
                }
            }
            AstNode::StringRef { pool_index } => {
                out.push(arena.get_string(*pool_index as usize).to_string())
            }
            _ => out.push(format!("{node:?}")),
        }
    }

    /// The multi-level splice's identity contract: an edit inside a
    /// nested member list must produce the same tree as a fresh parse
    /// — and must engage the deep surgery (the rs#542 regression
    /// where a nested-list edit re-parsed the whole enclosing member,
    /// and the earlier failure where a failed backtrack attempt's run
    /// bounds patched a wrong interpretation).
    #[test]
    fn deep_edit_matches_fresh_parse() {
        let g = nested_grammar();
        let text = doc();
        let mut inc = IncrementalParser::owned(g.clone());
        let _ = inc.parse_retained(&text).expect("initial parse");

        // Edit dep2's value in package 20: dep2 sits INSIDE the
        // nested member list (dep1 is the list's first member, held
        // outside the repetition), so only the deep surgery can
        // splice it without re-parsing the whole package member.
        let pkg20 = text.find("\"pkg20\"").expect("pkg20");
        let val_pos = text[pkg20..].find("\"v2\"").expect("v2") + pkg20;
        let edited = format!(
            "{}{}{}",
            &text[..val_pos],
            "\"v2-rc1\"",
            &text[val_pos + 4..]
        );
        let result = inc
            .parse_with_edit_retained(&edited, Edit::replace(val_pos, 4, 8))
            .expect("incremental parse");
        let expected = full_strings(&g, &edited);
        let mut got = Vec::new();
        walk_strings(&result.ast, inc.retained_arena(), &edited, &mut got);
        assert_eq!(got, expected, "the deep-spliced tree diverged");
        assert!(
            inc.spliced_deep() >= 1,
            "the deep surgery must engage on a nested-list edit"
        );
    }
}

#[cfg(test)]
mod dynamic_session_tests {
    use super::*;
    use crate::portable::dynamic::{register_dynamic_callback, DynamicCallback, DynamicContext};

    struct PassthroughCallback;

    impl DynamicCallback for PassthroughCallback {
        fn resolve(&self, _ctx: &DynamicContext) -> Option<Atom> {
            Some(Atom::Str {
                pattern: "k=1\n".to_string(),
            })
        }
        fn description(&self) -> &str {
            "passthrough"
        }
    }

    /// Dynamic grammars are eligible for incremental sessions
    /// (parsanol-ruby#80 item 3): the walker's dynamic-dependent memo
    /// filter keeps retention sound, and a dynamic line's edit
    /// re-parses to the same tree as a full parse.
    #[test]
    fn dynamic_grammar_session_edit_matches_full_parse() {
        let cb_id = register_dynamic_callback(Box::new(PassthroughCallback));
        let mut g = Grammar::new();
        let dyn_line = g.add_atom(Atom::Dynamic { callback_id: cb_id });
        let rep = g.add_atom(Atom::Repetition {
            atom: dyn_line,
            min: 0,
            max: None,
            tag: crate::portable::grammar::RepetitionTag::Repetition,
        });
        g.root = rep;

        let doc = "k=1\nk=1\nk=1\n";
        let edited = "k=1\nk=1\n";

        let mut inc = IncrementalParser::owned(g.clone());
        let mut arena = AstArena::new();
        inc.parse(doc, &mut arena).expect("initial dynamic parse");

        let mut arena = AstArena::new();
        let result = inc
            .parse_with_edit(edited, &mut arena, Edit::delete(8, 4))
            .expect("dynamic incremental parse");

        // Tree parity with a full parse of the edited input.
        let mut arena2 = AstArena::new();
        let mut full = crate::portable::parser::PortableParser::new(&g, edited, &mut arena2);
        let full_tree = full.parse().expect("full parse");
        assert_eq!(
            format!("{:?}", result.ast),
            format!("{full_tree:?}"),
            "arena-free comparison: both trees are InputRef/Array shapes"
        );
    }
}
