extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;

/// A compact bit vector that tracks which local indexes are in use.
///
/// Bits are packed into `u64` words — each word holds the allocation state for
/// 64 consecutive indexes. A set bit means the index is allocated; a clear bit
/// means it is free.
///
/// ```text
///   words[0]               words[1]               words[2]  ...
///  ┌────────────────────┐ ┌────────────────────┐ ┌──────────────
///  │ indexes 0..63      │ │ indexes 64..127    │ │ indexes 128..191
///  │ (64 bits per word) │ │                    │ │
///  └────────────────────┘ └────────────────────┘ └──────────────
/// ```
///
/// `capacity` is the actual number of valid indexes (i.e. `max_channels`).
/// Because the storage is rounded up to the nearest multiple of 64, the
/// last word may contain trailing bits beyond `capacity` — these are
/// never set and are excluded from search results.
///
/// Word-level operations allow bulk checks:
/// - A word equal to `u64::MAX` means all 64 indexes in that range are taken —
///   the scanner skips the entire word in one comparison.
/// - `trailing_zeros()` on the inverted word finds the first free index within
///   a word using a single CPU instruction (`TZCNT`/`BSF` on x86).
/// - `count_ones()` compiles to the hardware `POPCNT` instruction.
pub(crate) struct BitVector {
    /// Packed allocation bits. `words[i]` covers indexes `i*64 .. (i+1)*64`.
    words: Vec<u64>,
    /// Number of valid indexes tracked (= `max_channels`). May be less than
    /// `words.len() * 64` when `max_channels` is not a multiple of 64.
    capacity: usize,
}

impl BitVector {
    pub(crate) fn new(capacity: usize) -> Self {
        let num_words = capacity.div_ceil(64);
        Self {
            words: vec![0u64; num_words],
            capacity,
        }
    }

    pub(crate) fn capacity(&self) -> usize {
        self.capacity
    }

    #[cfg(test)]
    pub(crate) fn get(&self, index: usize) -> bool {
        debug_assert!(index < self.capacity);
        let word = index / 64;
        let bit = index % 64;
        (self.words[word] >> bit) & 1 == 1
    }

    pub(crate) fn set(&mut self, index: usize, value: bool) {
        debug_assert!(index < self.capacity);
        let word = index / 64;
        let bit = index % 64;
        if value {
            self.words[word] |= 1u64 << bit;
        } else {
            self.words[word] &= !(1u64 << bit);
        }
    }

    pub(crate) fn count_ones(&self) -> usize {
        self.words.iter().map(|w| w.count_ones() as usize).sum()
    }

    /// Find the first zero bit in `[from, to)`, skipping fully-set `u64` words.
    /// Returns `None` if every bit in the range is set.
    pub(crate) fn find_first_zero_in_range(&self, from: usize, to: usize) -> Option<usize> {
        if from >= to {
            return None;
        }

        let mut idx = from;
        while idx < to {
            let word_idx = idx / 64;
            let word = self.words[word_idx];

            if word == u64::MAX {
                idx = (word_idx + 1) * 64;
                continue;
            }

            let bit_offset = idx % 64;
            let mask = !0u64 << bit_offset;
            let zeros_masked = !word & mask;

            if zeros_masked != 0 {
                let first_zero = word_idx * 64 + zeros_masked.trailing_zeros() as usize;
                if first_zero < to && first_zero < self.capacity {
                    return Some(first_zero);
                }
            }

            idx = (word_idx + 1) * 64;
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitvector_basic() {
        let mut bv = BitVector::new(128);
        assert_eq!(bv.capacity(), 128);
        assert!(!bv.get(0));

        bv.set(0, true);
        assert!(bv.get(0));

        bv.set(0, false);
        assert!(!bv.get(0));
    }

    #[test]
    fn bitvector_count_ones() {
        let mut bv = BitVector::new(256);
        assert_eq!(bv.count_ones(), 0);

        for i in (0..256).step_by(2) {
            bv.set(i, true);
        }
        assert_eq!(bv.count_ones(), 128);
    }

    #[test]
    fn bitvector_find_zero_skips_full_words() {
        let mut bv = BitVector::new(192);
        for i in 0..128 {
            bv.set(i, true);
        }
        let found = bv.find_first_zero_in_range(0, 192);
        assert_eq!(found, Some(128));
    }

    #[test]
    fn bitvector_find_zero_partial_range() {
        let mut bv = BitVector::new(64);
        bv.set(3, true);
        bv.set(4, true);

        assert_eq!(bv.find_first_zero_in_range(3, 10), Some(5));
        assert_eq!(bv.find_first_zero_in_range(0, 3), Some(0));
    }

    #[test]
    fn bitvector_find_zero_all_set() {
        let mut bv = BitVector::new(64);
        for i in 0..64 {
            bv.set(i, true);
        }
        assert_eq!(bv.find_first_zero_in_range(0, 64), None);
    }

    #[test]
    fn bitvector_non_multiple_of_64() {
        let mut bv = BitVector::new(100);
        assert_eq!(bv.capacity(), 100);

        for i in 0..100 {
            bv.set(i, true);
        }
        assert_eq!(bv.count_ones(), 100);
        assert_eq!(bv.find_first_zero_in_range(0, 100), None);

        bv.set(99, false);
        assert_eq!(bv.find_first_zero_in_range(0, 100), Some(99));
    }
}
