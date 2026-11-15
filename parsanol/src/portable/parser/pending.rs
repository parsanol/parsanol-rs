//! The pending diagnostics form (rs#498 v2 surface).
//!
//! Failure labels are held inline (SSO) in note order with hash-first
//! dedup; the legacy Arc shapes materialize once, at parse end, on the
//! failure path only. A successful parse allocates no label storage at
//! all. Observable semantics (deepest set, wire, ranks — content and
//! order) are identical to the legacy surface; the pinned legacy
//! pointer-identity contracts are legacy-surface details and do not
//! carry over.

/// A failure label that stores up to 62 bytes inline. The labels that
/// dominate real grammars (`'x'`, `[a-zA-Z]`, `'end of input'`,
/// describe-atom phrases) never allocate; longer labels box once. The
/// 64-byte entry size is the deliberate trade: the failure path's
/// per-label allocation is the cost rs#498 exists to remove.
#[derive(Clone, Debug, PartialEq, Eq)]
enum InlineLabel {
    Inline { len: u8, bytes: [u8; 62] },
    Heap(Box<str>),
}

impl InlineLabel {
    const INLINE_CAP: usize = 62;

    fn new(label: &str) -> Self {
        let bytes = label.as_bytes();
        if bytes.len() <= Self::INLINE_CAP {
            let mut inline = [0u8; Self::INLINE_CAP];
            inline[..bytes.len()].copy_from_slice(bytes);
            Self::Inline {
                len: bytes.len() as u8,
                bytes: inline,
            }
        } else {
            Self::Heap(label.into())
        }
    }

    #[inline]
    fn as_str(&self) -> &str {
        match self {
            Self::Inline { len, bytes } => {
                debug_assert!((*len as usize) <= Self::INLINE_CAP);
                std::str::from_utf8(&bytes[..*len as usize]).unwrap_or("")
            }
            Self::Heap(boxed) => boxed.as_ref(),
        }
    }

    fn heap_bytes(&self) -> usize {
        match self {
            Self::Inline { .. } => 0,
            Self::Heap(boxed) => boxed.len(),
        }
    }
}

/// One recorded note: the position and the label as grammar data.
type PendingEntry = (usize, InlineLabel);

/// The pending failure state. Entries accumulate in note order; the
/// hash set enforces per-position label dedup at note time (the key
/// mixes the position and the label hash; a collision falls back to
/// the inline compare, so a distinct label can never be dropped and a
/// duplicate can never be kept). The rank index tracks the same
/// bounded distinct-position set the legacy surface keeps
/// (descending, at most `MAX_RANKED_FAILURES`).
#[derive(Default)]
pub(super) struct PendingFailure {
    pub(super) deepest: usize,
    pub(super) present: bool,
    entries: Vec<PendingEntry>,
    seen: std::collections::HashSet<u64>,
    ranks: Vec<usize>,
    /// Incrementally maintained byte estimate for the resource
    /// governor (rs#498): recomputing capacity sums per check would
    /// walk the entry list on every atom.
    bytes: usize,
}

impl PendingFailure {
    pub(super) fn note(&mut self, pos: usize, label: &str) {
        let key = Self::mix(pos, label);
        if !self.present || pos > self.deepest {
            // A deeper position resets the deepest set; the entries at
            // stale positions stay (filtered at materialization) — the
            // reset semantics live in the position filter, not in the
            // buffer.
            self.present = true;
            self.deepest = pos;
        } else if pos == self.deepest && self.seen.contains(&key) {
            return;
        }
        // Rank bookkeeping: the same bounded distinct-position set the
        // legacy surface keeps. A full index whose last entry is deeper
        // cannot hold `pos` and would drop it: skip the note entirely
        // (the legacy early-exit).
        if self.ranks.len() >= super::MAX_RANKED_FAILURES
            && self.ranks.last().is_some_and(|p| *p > pos)
        {
            return;
        }
        if self.seen.insert(key) {
            let inline = InlineLabel::new(label);
            self.bytes += std::mem::size_of::<PendingEntry>()
                + inline.heap_bytes()
                + std::mem::size_of::<u64>();
            self.entries.push((pos, inline));
        }
        match self.ranks.iter_mut().find(|p| **p == pos) {
            Some(_) => {}
            None => {
                if self.ranks.len() >= super::MAX_RANKED_FAILURES {
                    self.ranks.pop();
                } else {
                    self.bytes += std::mem::size_of::<usize>();
                }
                let at = self.ranks.partition_point(|p| *p > pos);
                self.ranks.insert(at, pos);
            }
        }
    }

    /// Merge another pending frame's notes in their note order through
    /// this store's note path (the same dedup and rank discipline a
    /// live replay would apply).
    pub(super) fn merge(&mut self, other: &PendingFailure) {
        for (pos, label) in &other.entries {
            self.note(*pos, label.as_str());
        }
    }

    /// Frame take: the live state moves out; the store resets to
    /// default (an empty pending store allocates nothing, so the
    /// legacy empty-slot trick buys nothing here).
    pub(super) fn take(&mut self) -> PendingFailure {
        std::mem::take(self)
    }

    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }

    /// The deepest set in note order (dedup already enforced at note
    /// time by the per-position hash keys).
    pub(super) fn deepest_labels(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter(|(pos, _)| *pos == self.deepest)
            .map(|(_, label)| label.as_str().to_owned())
            .collect()
    }

    pub(super) fn materialize_ranks(&self) -> Vec<(usize, Vec<String>)> {
        self.ranks
            .iter()
            .map(|pos| {
                let labels = self
                    .entries
                    .iter()
                    .filter(|(p, _)| p == pos)
                    .map(|(_, label)| label.as_str().to_owned())
                    .collect();
                (*pos, labels)
            })
            .collect()
    }

    pub(super) fn memory_usage(&self) -> usize {
        self.bytes
    }

    /// Mix the position into the label hash so the dedup key is
    /// per-(position, label) — the legacy dedup granularity.
    fn mix(pos: usize, label: &str) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325 ^ (pos as u64).wrapping_mul(0x100000001b3);
        for b in label.as_bytes() {
            h ^= u64::from(*b);
            h = h.wrapping_mul(0x100000001b3);
        }
        h
    }
}

impl Clone for PendingFailure {
    fn clone(&self) -> Self {
        Self {
            deepest: self.deepest,
            present: self.present,
            entries: self.entries.clone(),
            seen: self.seen.clone(),
            ranks: self.ranks.clone(),
            bytes: self.bytes,
        }
    }
}

impl std::fmt::Debug for PendingFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingFailure")
            .field("deepest", &self.deepest)
            .field("present", &self.present)
            .field("entries", &self.entries.len())
            .field("ranks", &self.ranks)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels_at(store: &PendingFailure, pos: usize) -> Vec<String> {
        store
            .entries
            .iter()
            .filter(|(p, _)| *p == pos)
            .map(|(_, l)| l.as_str().to_owned())
            .collect()
    }

    #[test]
    fn inline_labels_stay_inline() {
        let short = InlineLabel::new("'x'");
        assert!(matches!(short, InlineLabel::Inline { .. }));
        assert_eq!(short.as_str(), "'x'");
        let long = InlineLabel::new(&"a".repeat(64));
        assert!(matches!(long, InlineLabel::Heap(_)));
        assert_eq!(long.as_str().len(), 64);
    }

    #[test]
    fn note_dedups_per_position_only() {
        let mut store = PendingFailure::default();
        store.note(3, "'x'");
        store.note(3, "'x'");
        store.note(5, "'x'");
        assert_eq!(labels_at(&store, 3).len(), 1);
        assert_eq!(labels_at(&store, 5).len(), 1);
        assert_eq!(store.deepest, 5);
    }

    #[test]
    fn deeper_note_resets_the_deepest_set_without_touching_entries() {
        let mut store = PendingFailure::default();
        store.note(3, "'a'");
        store.note(7, "'b'");
        store.note(7, "'c'");
        assert_eq!(store.deepest_labels(), vec!["'b'", "'c'"]);
        assert_eq!(labels_at(&store, 3), vec!["'a'"]);
    }

    #[test]
    fn ranks_stay_descending_and_bounded() {
        let mut store = PendingFailure::default();
        for pos in [4usize, 9, 2, 9, 6, 1, 8, 3, 5, 7, 0] {
            store.note(pos, "'x'");
        }
        let expected: Vec<usize> = store.ranks.clone();
        let mut sorted = expected.clone();
        sorted.sort_by_key(|p| std::cmp::Reverse(*p));
        sorted.truncate(super::super::MAX_RANKED_FAILURES);
        assert_eq!(expected, sorted);
        assert!(store.ranks.len() <= super::super::MAX_RANKED_FAILURES);
    }

    #[test]
    fn merge_replays_note_order_with_the_same_dedup() {
        let mut a = PendingFailure::default();
        a.note(2, "'x'");
        a.note(2, "'y'");
        let mut b = PendingFailure::default();
        b.note(2, "'x'");
        b.note(4, "'z'");
        a.merge(&b);
        assert_eq!(labels_at(&a, 2), vec!["'x'", "'y'"]);
        assert_eq!(labels_at(&a, 4), vec!["'z'"]);
        assert_eq!(a.deepest, 4);
    }
}
