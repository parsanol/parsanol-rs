//! Byte-class run scanning (TODO.perf/2) — the single engine both
//! parsers use for character-class repetition runs.
//!
//! A run of members ("scan while byte is in the class") is the hot
//! loop of identifier/whitespace/digit tokens. Plans decompose class
//! membership into a bounded number of closed ranges (a stray single
//! byte is a degenerate range); long runs scan 32-byte windows with a
//! vector membership kernel (paired NEON loads on aarch64, AVX2 with
//! runtime detection then SSE2 on x86_64, scalar elsewhere) and only
//! the block that ends the run is searched byte-wise. Anything that
//! does not decompose into at most eight ranges falls back to a
//! 256-entry table.
//!
//! Both entry points preserve their engine's existing stepping
//! semantics:
//!
//! - [`ScanPlan::scan_run`] steps whole UTF-8 characters (the VM's
//!   `Span` checks the lead byte and skips the character);
//! - [`ScanPlan::scan_run_bytewise`] advances byte-by-byte (the
//!   tree-walker's bulk repetition semantics).

/// Maximum ranges a plan may decompose into before falling back to
/// the table. Covers the common classes: digit 1, alpha 2, alnum 3,
/// word 4, hex 3, whitespace 6.
const MAX_RANGES: usize = 8;

/// Membership plan for one character class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanPlan {
    /// Membership is a fixed chain of closed ranges; `ranges[..n]`
    /// are live.
    Ranges {
        /// Live member ranges (a stray single byte is a degenerate
        /// range); entries beyond `n` are zero padding.
        ranges: [(u8, u8); MAX_RANGES],
        /// Number of live entries in `ranges`.
        n: u8,
    },
    /// Undecomposable: lookup table.
    Table(Box<[bool; 256]>),
}

/// Membership test over exactly N ranges held by value: the compare
/// chain runs on registers, the shape LLVM vectorizes.
#[inline(always)]
fn member_of<const N: usize>(ranges: [(u8, u8); N], b: u8) -> bool {
    let mut m = false;
    for range in ranges.iter().take(N) {
        m |= range.0 <= b && b <= range.1;
    }
    m
}

/// First non-member inside a 16-byte block, or `None` when the whole
/// block belongs to the class. SIMD kernel: every range contributes a
/// 16-lane compare, OR-ed into one mask — the same technique as
/// libc's strspn. LLVM refused to auto-vectorize this shape (an
/// early-exit search with a memory-reading predicate), so the
/// baseline-vector instructions are explicit.
#[inline]
#[allow(clippy::too_many_lines)]
fn block_first_non_member<const N: usize>(r: [(u8, u8); N], block: &[u8]) -> Option<usize> {
    debug_assert_eq!(block.len(), 16);

    #[cfg(target_arch = "aarch64")]
    {
        // SAFETY: NEON is baseline on aarch64; `block` is exactly 16
        // bytes of initialized memory; the vector-to-array transmute
        // is the documented same-layout idiom.
        unsafe {
            use std::arch::aarch64::*;
            let bytes = vld1q_u8(block.as_ptr());
            let mut acc = vdupq_n_u8(0);
            for range in r.iter().take(N) {
                let lo = vdupq_n_u8(range.0);
                let hi = vdupq_n_u8(range.1);
                let ge = vcgeq_u8(bytes, lo);
                let le = vcleq_u8(bytes, hi);
                acc = vorrq_u8(acc, vandq_u8(ge, le));
            }
            // All lanes member (0xFF) => run continues through the
            // block.
            if vminvq_u8(acc) == 0xFF {
                None
            } else {
                let ones: [u8; 16] = std::mem::transmute(acc);
                ones.iter().position(|&m| m == 0)
            }
        }
    }

    #[cfg(target_arch = "x86_64")]
    {
        use std::arch::x86_64::*;
        // SAFETY: sse2 is baseline on x86_64.
        unsafe {
            let bytes = _mm_loadu_si128(block.as_ptr().cast());
            let mut acc = _mm_setzero_si128();
            for range in r.iter().take(N) {
                let lo = _mm_set1_epi8(range.0 as i8);
                let hi = _mm_set1_epi8(range.1 as i8);
                // Unsigned range test via saturating ops:
                // lo <= b <= hi  <=>  max(b,lo)==b && min(b,hi)==b
                let ge = _mm_cmpeq_epi8(_mm_max_epu8(bytes, lo), bytes);
                let le = _mm_cmpeq_epi8(_mm_min_epu8(bytes, hi), bytes);
                acc = _mm_or_si128(acc, _mm_and_si128(ge, le));
            }
            let mask = _mm_movemask_epi8(acc) as u16;
            if mask == 0xFFFF {
                None
            } else {
                Some(mask.trailing_ones() as usize)
            }
        }
    }

    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        block.iter().position(|&b| !member_of::<N>(r, b))
    }
}

/// Whether both halves of a 32-byte window are entirely members —
/// the wide fast path. aarch64: two NEON loads and ONE membership
/// reduce per window; x86_64 AVX2: one 32-byte kernel.
#[inline]
fn window_all_members<const N: usize>(r: [(u8, u8); N], lo16: &[u8], hi16: &[u8]) -> bool {
    debug_assert_eq!(lo16.len(), 16);
    debug_assert_eq!(hi16.len(), 16);

    #[cfg(target_arch = "aarch64")]
    {
        // SAFETY: NEON is baseline on aarch64; both windows are 16
        // initialized bytes.
        unsafe {
            use std::arch::aarch64::*;
            let a = vld1q_u8(lo16.as_ptr());
            let b = vld1q_u8(hi16.as_ptr());
            let mut acc = vdupq_n_u8(0);
            for range in r.iter().take(N) {
                let lo = vdupq_n_u8(range.0);
                let hi = vdupq_n_u8(range.1);
                acc = vorrq_u8(acc, vandq_u8(vcgeq_u8(a, lo), vcleq_u8(a, hi)));
                acc = vorrq_u8(acc, vandq_u8(vcgeq_u8(b, lo), vcleq_u8(b, hi)));
            }
            vminvq_u8(acc) == 0xFF
        }
    }

    #[cfg(target_arch = "x86_64")]
    {
        if avx2_available() {
            // SAFETY: guarded by the cached runtime detection; both
            // windows are 16 initialized bytes forming one aligned
            // 32-byte window.
            unsafe { x86_wide::window32_all_members_avx2::<N>(r, lo16, hi16) }
        } else {
            block_first_non_member::<N>(r, lo16).is_none()
                && block_first_non_member::<N>(r, hi16).is_none()
        }
    }

    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        lo16.iter().chain(hi16).all(|&b| member_of::<N>(r, b))
    }
}

/// Byte-wise run scan over exactly N leading ranges. Long runs scan
/// 32-byte windows (two 16-byte halves in one membership reduce on
/// aarch64; one AVX2 kernel on x86_64 when detected); only the window
/// that ends the run is located block-wise, and runs shorter than a
/// window keep the 16-byte block path. Short remainders stay scalar.
#[inline(always)]
fn scan_bytewise_ranges<const N: usize>(ranges: &[(u8, u8)], input: &[u8], from: usize) -> usize {
    assert!(ranges.len() >= N);
    let r: [(u8, u8); N] = ranges[..N].try_into().expect("exact range count");
    let bytes = &input[from.min(input.len())..];

    let mut windows = bytes.chunks_exact(32);
    let mut consumed = 0usize;
    for window in windows.by_ref() {
        let (lo16, hi16) = window.split_at(16);
        if !window_all_members::<N>(r, lo16, hi16) {
            // The run ends inside this window; locate the byte.
            if let Some(i) = block_first_non_member::<N>(r, lo16) {
                return from + consumed + i;
            }
            if let Some(i) = block_first_non_member::<N>(r, hi16) {
                return from + consumed + 16 + i;
            }
            unreachable!("window flagged a non-member");
        }
        consumed += 32;
    }

    let mut chunks = windows.remainder().chunks_exact(16);
    for chunk in chunks.by_ref() {
        if let Some(i) = block_first_non_member::<N>(r, chunk) {
            return from + consumed + i;
        }
        consumed += 16;
    }
    match chunks
        .remainder()
        .iter()
        .position(|&b| !member_of::<N>(r, b))
    {
        Some(i) => from + consumed + i,
        None => input.len(),
    }
}

#[cfg(target_arch = "x86_64")]
mod x86_wide {
    use std::arch::x86_64::*;

    /// One 32-byte membership kernel over the two adjacent windows.
    ///
    /// # Safety
    /// AVX2 must be available and `lo16`/`hi16` must be 16 initialized
    /// bytes each, adjacent or not (unaligned loads).
    #[target_feature(enable = "avx2")]
    pub unsafe fn window32_all_members_avx2<const N: usize>(
        r: [(u8, u8); N],
        lo16: &[u8],
        hi16: &[u8],
    ) -> bool {
        // SAFETY: caller guarantees AVX2 and initialized windows.
        unsafe {
            let a = _mm256_loadu_si256(lo16.as_ptr().cast());
            // The windows are adjacent slices of one 32-byte chunk, so
            // a single load at lo16 covers both halves.
            let _ = hi16;
            let mut acc = _mm256_setzero_si256();
            for range in r.iter().take(N) {
                let lo = _mm256_set1_epi8(range.0 as i8);
                let hi = _mm256_set1_epi8(range.1 as i8);
                // Unsigned range test via saturating ops:
                // lo <= b <= hi <=> max(b,lo)==b && min(b,hi)==b
                let ge = _mm256_cmpeq_epi8(_mm256_max_epu8(a, lo), a);
                let le = _mm256_cmpeq_epi8(_mm256_min_epu8(a, hi), a);
                acc = _mm256_or_si256(acc, _mm256_and_si256(ge, le));
            }
            let mask = _mm256_movemask_epi8(acc) as u32;
            mask == 0xFFFF_FFFF
        }
    }
}

/// Cached AVX2 detection (TODO.perf/6): detection is a CPUID walk,
/// too expensive per scan; cache the answer per process.
#[cfg(target_arch = "x86_64")]
fn avx2_available() -> bool {
    use std::sync::atomic::{AtomicU8, Ordering};
    static STATE: AtomicU8 = AtomicU8::new(0);
    match STATE.load(Ordering::Relaxed) {
        1 => true,
        2 => false,
        _ => {
            let has = std::arch::is_x86_feature_detected!("avx2");
            STATE.store(if has { 1 } else { 2 }, Ordering::Relaxed);
            has
        }
    }
}

/// UTF-8-stepping run scan over exactly N leading ranges: the lead
/// byte decides membership, continuation bytes are consumed
/// unexamined.
#[inline(always)]
fn scan_utf8_ranges<const N: usize>(ranges: &[(u8, u8)], input: &[u8], from: usize) -> usize {
    assert!(ranges.len() >= N);
    let r: [(u8, u8); N] = ranges[..N].try_into().expect("exact range count");
    let mut pos = from;
    while pos < input.len() {
        let b = input[pos];
        if !member_of::<N>(r, b) {
            break;
        }
        pos += crate::portable::char_class::utf8_char_len(b);
    }
    pos
}

/// Dispatch on the live range count, calling the const-generic scan
/// monomorphization for exactly that many leading ranges.
macro_rules! dispatch_ranges {
    ($n:expr, $ranges:expr, $f:ident, $input:expr, $from:expr) => {
        match $n {
            1 => $f::<1>(&$ranges[..1], $input, $from),
            2 => $f::<2>(&$ranges[..2], $input, $from),
            3 => $f::<3>(&$ranges[..3], $input, $from),
            4 => $f::<4>(&$ranges[..4], $input, $from),
            5 => $f::<5>(&$ranges[..5], $input, $from),
            6 => $f::<6>(&$ranges[..6], $input, $from),
            7 => $f::<7>(&$ranges[..7], $input, $from),
            _ => $f::<8>(&$ranges[..8], $input, $from),
        }
    };
}

impl ScanPlan {
    /// Build a plan by probing membership over all 256 byte values.
    pub fn from_membership(membership: impl Fn(u8) -> bool) -> Self {
        let mut members: Vec<u8> = Vec::new();
        for b in 0u16..256 {
            if membership(b as u8) {
                members.push(b as u8);
            }
        }
        Self::from_members(&members)
    }

    /// Build a plan from an explicit member list.
    pub fn from_members(members: &[u8]) -> Self {
        let mut ranges: Vec<(u8, u8)> = Vec::new();
        let mut iter = members.iter().copied().peekable();
        while let Some(b) = iter.next() {
            let mut end = b;
            while end < u8::MAX && iter.peek() == Some(&(end + 1)) {
                end += 1;
                iter.next();
            }
            ranges.push((b, end));
        }
        if ranges.len() <= MAX_RANGES {
            let mut r = [(0u8, 0u8); MAX_RANGES];
            for (i, range) in ranges.iter().enumerate() {
                r[i] = *range;
            }
            ScanPlan::Ranges {
                ranges: r,
                n: ranges.len() as u8,
            }
        } else {
            let mut table = Box::new([false; 256]);
            for &b in members {
                table[b as usize] = true;
            }
            ScanPlan::Table(table)
        }
    }

    /// Whether the byte belongs to the class.
    #[inline]
    pub fn contains(&self, b: u8) -> bool {
        match self {
            ScanPlan::Ranges { ranges, n } => {
                let n = *n as usize;
                let mut m = false;
                for range in ranges.iter().take(n) {
                    m |= range.0 <= b && b <= range.1;
                }
                m
            }
            ScanPlan::Table(table) => table[b as usize],
        }
    }

    /// Whether every member is ASCII. ASCII-only plans can scan runs
    /// byte-by-byte (vectorizable); a non-ASCII member means the run
    /// may contain UTF-8 lead bytes whose continuations must be
    /// consumed as part of the character.
    #[inline]
    pub fn is_ascii_only(&self) -> bool {
        match self {
            ScanPlan::Ranges { ranges, n } => (0..*n as usize).all(|i| ranges[i].1 < 0x80),
            ScanPlan::Table(table) => table[0x80..].iter().all(|&m| !m),
        }
    }

    /// End of the member-run starting at `from` (exclusive index).
    /// Steps whole UTF-8 characters: the lead byte decides membership,
    /// continuations are consumed unexamined.
    pub fn scan_run(&self, input: &[u8], from: usize) -> usize {
        if self.is_ascii_only() {
            return self.scan_run_bytewise(input, from);
        }
        match self {
            ScanPlan::Ranges { ranges, n } => {
                dispatch_ranges!(*n, ranges, scan_utf8_ranges, input, from)
            }
            ScanPlan::Table(_) => {
                let mut pos = from;
                while pos < input.len() && self.contains(input[pos]) {
                    pos += crate::portable::char_class::utf8_char_len(input[pos]);
                }
                pos
            }
        }
    }

    /// End of the member-run, byte-wise: every byte is tested. The
    /// comparison-shaped predicate lets LLVM vectorize the loop.
    pub fn scan_run_bytewise(&self, input: &[u8], from: usize) -> usize {
        match self {
            ScanPlan::Ranges { ranges, n } => {
                dispatch_ranges!(*n, ranges, scan_bytewise_ranges, input, from)
            }
            ScanPlan::Table(table) => {
                if from >= input.len() {
                    return from;
                }
                match input[from..].iter().position(|&b| !table[b as usize]) {
                    Some(i) => from + i,
                    None => input.len(),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digit_plan() -> ScanPlan {
        ScanPlan::from_membership(|b| b.is_ascii_digit())
    }

    #[test]
    fn plan_decomposes_ascii_ranges() {
        match digit_plan() {
            ScanPlan::Ranges { n, .. } => assert_eq!(n, 1),
            other => panic!("expected ranges plan, got {other:?}"),
        }
    }

    #[test]
    fn scan_run_stops_at_first_non_member() {
        let plan = digit_plan();
        assert_eq!(plan.scan_run_bytewise(b"123abc", 0), 3);
        assert_eq!(plan.scan_run_bytewise(b"abc123", 0), 0);
        assert_eq!(plan.scan_run_bytewise(b"123", 0), 3);
        assert_eq!(plan.scan_run_bytewise(b"123", 3), 3);
    }

    #[test]
    fn word_plan_covers_ranges_and_single() {
        let plan = ScanPlan::from_membership(|b| b.is_ascii_alphanumeric() || b == b'_');
        assert!(
            plan.contains(b'a')
                && plan.contains(b'Z')
                && plan.contains(b'9')
                && plan.contains(b'_')
        );
        assert!(!plan.contains(b'-'));
        assert_eq!(plan.scan_run_bytewise(b"ab_9 ", 0), 4);
    }

    #[test]
    fn whitespace_plan_is_all_ranges() {
        let plan = ScanPlan::from_membership(|b| b" \t\n\r\x0b\x0c".contains(&b));
        match &plan {
            // 0x09..=0x0D collapse into one range, plus the space
            ScanPlan::Ranges { n, .. } => assert_eq!(*n, 2),
            other => panic!("expected ranges plan, got {other:?}"),
        }
        assert_eq!(plan.scan_run_bytewise(b"a  \t b", 1), 5);
    }

    #[test]
    fn table_fallback_for_undecomposable_sets() {
        let mut members: Vec<u8> = Vec::new();
        for i in 0..40u8 {
            members.push(i * 3);
        }
        match ScanPlan::from_members(&members) {
            ScanPlan::Table(_) => {}
            other => panic!("expected table plan, got {other:?}"),
        }
    }

    #[test]
    fn utf8_lead_stepping_matches_span_semantics() {
        // A set that admits the lead byte of é (0xC3) and its
        // continuation: run scanning must consume the whole char.
        let bytes: &[u8] = &[b'a', b'b', 0xC3, 0xA9, b'c'];
        let plan = ScanPlan::from_members(&[b'a', b'b', b'c', 0xC3, 0xA9]);
        assert_eq!(plan.scan_run(bytes, 0), 5);
    }

    #[test]
    fn ascii_plan_stops_before_multibyte_lead() {
        let plan = ScanPlan::from_membership(|b| b.is_ascii_alphabetic());
        let bytes: &[u8] = b"ab\xC3\xA9c";
        assert_eq!(plan.scan_run(bytes, 0), 2);
        assert_eq!(plan.scan_run_bytewise(bytes, 0), 2);
    }

    #[test]
    fn contains_agrees_across_range_counts() {
        for n in 1..=8usize {
            let members: Vec<u8> = (0..n as u8)
                .flat_map(|i| [i * 8, i * 8 + 1, i * 8 + 2])
                .collect();
            let plan = ScanPlan::from_members(&members);
            let ScanPlan::Ranges { n: got, .. } = &plan else {
                panic!("expected ranges plan");
            };
            assert_eq!(*got as usize, n);
            for b in 0u16..256 {
                let expected = members.contains(&(b as u8));
                assert_eq!(plan.contains(b as u8), expected, "n={n} b={}", b as u8);
            }
            // scan agrees with contains on a mixed run
            let mut corpus: Vec<u8> = Vec::new();
            for &m in &members {
                corpus.push(m);
            }
            corpus.push(0xFE); // guaranteed non-member
            assert_eq!(plan.scan_run_bytewise(&corpus, 0), members.len());
        }
    }
}
