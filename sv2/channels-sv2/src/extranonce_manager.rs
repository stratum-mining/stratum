use bitvec::prelude::*;

/// Represents a unique extranonce prefix for a client connection.
#[derive(Eq, Hash, PartialEq, Clone, Debug)]
pub struct ExtranoncePrefix {
    bit_index: u16,
    id: [u8; 4],
}
impl PartialOrd for ExtranoncePrefix {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for ExtranoncePrefix {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.bit_index.cmp(&other.bit_index)
    }
}

/// Extranonce prefixes are generated using a bit vector where each prefix represents a bit
/// in the vector. The prefix is then composed of the index of the assigned bit
/// and the server ID. When a channel ends, the bit is cleared, allowing it to be reused.
///
/// # Generic:
/// `NUM_BIT_CONTAINERS`: The number of u64 containers allocated for generating unique extranonce prefixes (ex. `ExtranoncePrefixManager<625>` results in `40k` bits for generating extranonce prefixes)
/// The extranonce prefix component of the extranonce1 is capped at 2 bytes, so the maximum useful value for NUM_BIT_CONTAINERS is 1024 (65536 bits).
pub struct ExtranoncePrefixManager<const NUM_BIT_CONTAINERS: usize> {
    server_id: Option<u8>,
    prefix_bits: BitVec<u64>,
    last_freed_bit_index: Option<usize>,
    last_allocated_bit_index: Option<usize>,
}

impl<const NUM_BIT_CONTAINERS: usize> ExtranoncePrefixManager<NUM_BIT_CONTAINERS> {
    /// NOTE: we are using u64 as the underlying type for the bit vector for performance reasons (~90% faster than u8).
    /// The performance is most likely attributed to less underlying container loads. To calculate the total
    /// available bits, we take the number of containers and multiply by 64 (bits in a u64).
    pub const AVAILABLE_BITS: usize = NUM_BIT_CONTAINERS * 64;
    pub fn new(server_id: Option<u8>) -> Self {
        assert!(
            Self::AVAILABLE_BITS - 1 <= u16::MAX as usize,
            "Cannot allocate more than 65536 bits for extranonce prefix"
        );
        let prefix_bits = bitvec!(u64, Lsb0; 0; Self::AVAILABLE_BITS);
        Self {
            server_id,
            prefix_bits,
            last_freed_bit_index: None,
            last_allocated_bit_index: None,
        }
    }

    /// this is slow, so we should only use for testing
    /// "full" is when there is only one zero bit left in the prefix_bits bit vector
    /// (this is for optimization purposes)
    #[cfg(test)]
    fn is_full(&self) -> bool {
        let zero_count = self.prefix_bits.count_ones();
        Self::AVAILABLE_BITS - zero_count <= 1
    }

    pub fn generate_extranonce_prefix(&mut self) -> Option<ExtranoncePrefix> {
        // start at the last_allocated_index because it's more likely to have free slots after it that have not just been freed
        let start_index = self
            .last_allocated_bit_index
            .map_or(0, |idx| (idx + 1) % self.prefix_bits.len());
        // create an open slot iterator that starts from the last allocated bit index and wraps around
        // PERFORMANCE NOTE: `.iter_zeros()` is ~12x more performant that iterating over all bits
        let iter_zeros = self.prefix_bits.iter_zeros();
        let first_set = iter_zeros.skip_while(|bit_index| *bit_index < start_index); // this only clones the iterator state, not the underlying vector
        let second_set = iter_zeros.take_while(|bit_index| *bit_index < start_index);
        let shifted_zeros_bit_iter = first_set.chain(second_set);

        // search for the first zero bit, skipping the last freed bit index if necessary
        let mut bit_index = None;
        for zero_bit_index in shifted_zeros_bit_iter {
            // this is only a sanity check to prevent reusing the last freed bit index,
            // since we start searching from the last set bit index
            if let Some(last_freed_bit_index) = self.last_freed_bit_index {
                if zero_bit_index == last_freed_bit_index {
                    continue;
                }
            }
            bit_index = Some(zero_bit_index);
            break;
        }

        // update state if we found a zero bit, else let the caller know to drop the attempted connection
        let client_bit_index = if let Some(index) = bit_index {
            self.prefix_bits.set(index, true);
            self.last_allocated_bit_index = Some(index);
            index as u16
        } else {
            return None;
        };

        // convert bits to ExtranoncePrefix
        let mut extranonce_prefix_bytes = [0u8; 4];
        extranonce_prefix_bytes[0..2].copy_from_slice(&(client_bit_index).to_le_bytes());
        let server_id_bytes = match self.server_id {
            Some(id) => (id as u16).to_be_bytes(),
            None => 0u16.to_be_bytes(),
        };
        extranonce_prefix_bytes[2..4].copy_from_slice(&server_id_bytes);
        Some(ExtranoncePrefix {
            bit_index: client_bit_index,
            id: extranonce_prefix_bytes,
        })
    }

    pub fn free_extranonce_prefix(&mut self, extranonce_prefix: &ExtranoncePrefix) {
        self.prefix_bits
            .set(extranonce_prefix.bit_index as usize, false);
        self.last_freed_bit_index = Some(extranonce_prefix.bit_index as usize);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_extranonce_prefix_manager() {
        const ALLOCATED_PREFIX_BITS: usize = 1024; // 1024 * u64 bits for extranonce prefix, allows for 65536 unique IDs
        let mut manager = ExtranoncePrefixManager::<{ ALLOCATED_PREFIX_BITS }>::new(Some(0));
        assert!(!manager.is_full());
        assert_eq!(
            ExtranoncePrefixManager::<{ ALLOCATED_PREFIX_BITS }>::AVAILABLE_BITS,
            u16::MAX as usize + 1
        );

        let mut used_ids = HashSet::new();
        // Start timing
        let start = std::time::Instant::now();

        // Generate all available prefixes
        for _ in 0..ExtranoncePrefixManager::<{ ALLOCATED_PREFIX_BITS }>::AVAILABLE_BITS {
            let extranonce_prefix = manager
                .generate_extranonce_prefix()
                .expect("Failed to generate extranonce prefix");
            assert_eq!(
                extranonce_prefix.id.len(),
                4,
                "ExtranoncePrefix is not 4 bytes"
            );
            assert!(used_ids.insert(extranonce_prefix));
        }

        // End timing
        let duration = start.elapsed();
        let avg = duration.as_secs_f64()
            / ExtranoncePrefixManager::<{ ALLOCATED_PREFIX_BITS }>::AVAILABLE_BITS as f64;
        println!(
            "Total time: {:?}, average per loop: {:.9} micros",
            duration,
            avg * 1_000_000.0
        );
        // try to generate one more extranonce prefix, should fail
        let extranonce_prefix = manager.generate_extranonce_prefix();
        assert!(extranonce_prefix.is_none());
        assert!(manager.is_full());

        // free 2 extranonce prefix and check if we can generate a new one
        let last_allocated_bit_index = manager
            .last_allocated_bit_index
            .expect("No last allocated bit index");
        let mut extranonce_prefix_sorted = used_ids.iter().cloned().collect::<Vec<_>>();
        extranonce_prefix_sorted.sort();
        let mut extranonce_prefix_iterator = extranonce_prefix_sorted.iter();
        let mut free_extranonce_prefix = ExtranoncePrefix {
            bit_index: 0,
            id: [0u8; 4],
        };
        for _i in 0..2 {
            let extranonce_prefix = extranonce_prefix_iterator
                .next()
                .cloned()
                .expect("No extranonce prefixes to free");
            free_extranonce_prefix = extranonce_prefix.clone();
            manager.free_extranonce_prefix(&extranonce_prefix);
        }
        assert!(!manager.is_full()); // after filling the manager, freeing one it should be full until 2 are freed
        let extranonce_prefix = manager
            .generate_extranonce_prefix()
            .expect("Failed to generate extranonce prefix");
        assert!(
            extranonce_prefix.bit_index < last_allocated_bit_index as u16,
            "ExtranoncePrefix generator did not wrap"
        );
        assert!(manager.is_full());
        assert!(
            manager.generate_extranonce_prefix().is_none(),
            "extranonce prefixes should be 'full' again"
        );
        assert_ne!(
            extranonce_prefix.bit_index, free_extranonce_prefix.bit_index,
            "Even when exhausting the manager, the new extranonce prefix bit index should not be the same as the last freed one"
        );
        assert_ne!(
            extranonce_prefix.id, free_extranonce_prefix.id,
            "Even when exhausting the manager, the new extranonce prefix should not be the same as the last freed one"
        );

        let mut used_ids_iter = used_ids.into_iter();
        let first_extranonce_prefix = used_ids_iter
            .next()
            .expect("No extranonce prefixes to check");
        let second_extranonce_prefix = used_ids_iter
            .next()
            .expect("No second extranonce prefix to check");

        // we need to know which extranonce prefix is smaller and which is bigger, so we can ensure we ignore the smaller one in the next test, since
        // `generate_extranonce_prefix` iterates zeros from smallest to largest bit_index
        let (smaller_extranonce_prefix, bigger_extranonce_prefix) =
            if first_extranonce_prefix.bit_index < second_extranonce_prefix.bit_index {
                (first_extranonce_prefix, second_extranonce_prefix)
            } else {
                (second_extranonce_prefix, first_extranonce_prefix)
            };

        manager.free_extranonce_prefix(&bigger_extranonce_prefix);
        manager.free_extranonce_prefix(&smaller_extranonce_prefix);
        let new_extranonce_prefix = manager
            .generate_extranonce_prefix()
            .expect("Failed to generate extranonce prefix after freeing");
        assert!(
            new_extranonce_prefix.bit_index != smaller_extranonce_prefix.bit_index,
            "The new extranonce prefix should pass over the last freed bit index"
        );
    }
}
