extern crate alloc;

use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;

use super::{bitvector::BitVector, MAX_EXTRANONCE_LEN};

/// An extranonce prefix owned by a channel.
///
/// Carries the raw prefix bytes — accessible via [`as_bytes`](Self::as_bytes) —
/// and, optionally, an internal `PrefixAllocation` record that ties the prefix to an
/// [`ExtranonceAllocator`](super::allocator::ExtranonceAllocator)'s bitmap.
///
/// There are two ways to build an [`ExtranoncePrefix`]:
///
/// - [`ExtranoncePrefix::from_wire`] — for bytes received over the wire
///   (e.g. in `OpenExtendedMiningChannelSuccess`,
///   `OpenStandardMiningChannelSuccess`, or `SetExtranoncePrefix`).
///   These prefixes carry no allocation record and [`Drop`] is a no-op.
/// - Converted via [`From<AllocatedExtranoncePrefix>`] from an allocator-produced
///   [`AllocatedExtranoncePrefix`] (see
///   [`ExtranonceAllocator::allocate_extended`](super::allocator::ExtranonceAllocator::allocate_extended)
///   /
///   [`allocate_standard`](super::allocator::ExtranonceAllocator::allocate_standard)).
///   These carry an internal `PrefixAllocation` with a [`Weak`] back-reference to the
///   allocator's bitmap. The current prefix and any internally retained snapshots
///   share this record; dropping its last owner clears the corresponding bit,
///   returning the slot to the allocator's free pool.
///
/// Server-side channels do **not** accept this loose type directly. They
/// require an [`AllocatedExtranoncePrefix`] at the API boundary so that
/// the allocator's bitmap always reflects the set of live server channels.
/// Client-side channels accept either, because client-held prefixes
/// legitimately come from both sources — wire prefixes received from an
/// upstream server, or allocator-produced prefixes when an application
/// mints its own prefixes locally (e.g. a proxy that sub-allocates an
/// upstream-assigned extranonce space and tracks each downstream with a
/// client channel).
///
/// # Automatic release on drop
///
/// For allocator-produced prefixes, the allocation is returned to the allocator
/// automatically when the current prefix and its retained snapshots are dropped. Typical usage is
/// therefore to store the prefix on the channel and let it drop with the
/// channel:
///
/// ```ignore
/// // On channel open:
/// let prefix = allocator.allocate_extended(min_size)?;
/// let channel = ExtendedChannel::new_for_pool(.., prefix, ..)?;
///
/// // On channel close:
/// drop(channel); // prefix drops with it -> allocation is released
/// ```
///
/// There is no explicit release API — correct cleanup is enforced by
/// ownership. Because [`ExtranoncePrefix`] is neither `Copy` nor `Clone`,
/// callers cannot duplicate allocation ownership. Internally, the allocation record is shared
/// through [`Arc`], whose last owner releases the slot exactly once.
///
/// If the allocator itself is dropped before an outstanding prefix, releasing the
/// allocation record becomes a silent no-op (the [`Weak`] reference fails to upgrade).
/// This is safe — the bitmap is gone, so there is nothing to update.
///
/// Note that "the prefix is no longer in use" is not the same as "the
/// channel stopped using it as its current prefix". Jobs only carry a copy
/// of the prefix bytes they were created under and stay valid across a
/// prefix rotation, so channels deliberately *defer* the drop: on
/// `set_extranonce_prefix` (server and client channels alike) the
/// rotated-out prefix is retained by the channel and only dropped once no
/// future, active or past job created under it remains. Releasing it
/// eagerly would let the allocator hand the same extranonce space to a
/// second live channel while those jobs still validate shares.
///
/// An upstream-only update preserves the allocation and the local suffix. Before updating,
/// channels snapshot the old bytes and share the same allocation record with that snapshot.
/// A snapshot is retained only while a future, active or past job uses its bytes. This also
/// protects the old jobs if a later whole-prefix rotation changes the channel's allocation:
/// the slot stays reserved until neither the current prefix nor any retained snapshot owns it.
/// Snapshots do not allocate additional bitmap slots or change the jobs' captured bytes.
#[derive(Debug)]
pub struct ExtranoncePrefix {
    prefix: Vec<u8>,
    /// Length of the leading `upstream_prefix` region.
    ///
    /// For wire-sourced prefixes, the entire prefix is upstream-assigned. For
    /// allocator-produced prefixes, the remaining bytes contain
    /// `local_prefix | local_index` and, for standard channels, rollable padding.
    upstream_prefix_len: u8,
    /// `Some(_)` when the prefix was minted by an allocator; `None` when it
    /// was built from wire bytes via [`ExtranoncePrefix::from_wire`].
    allocation: Option<Arc<PrefixAllocation>>,
}

/// An [`ExtranoncePrefix`] that is guaranteed, at the type level, to have
/// been produced by an
/// [`ExtranonceAllocator`](super::allocator::ExtranonceAllocator).
///
/// Server-side channel constructors require this type so that every
/// server channel holds a prefix that reserves a slot in the allocator's
/// bitmap and releases it on drop — i.e. the set of live server channels
/// is always reflected in the allocator's `allocated_count`.
///
/// There is no public constructor: an `AllocatedExtranoncePrefix` can only
/// be obtained from
/// [`ExtranonceAllocator::allocate_extended`](super::allocator::ExtranonceAllocator::allocate_extended)
/// or
/// [`allocate_standard`](super::allocator::ExtranonceAllocator::allocate_standard).
/// It converts into the wider [`ExtranoncePrefix`] via
/// [`From`]/[`Into`] — the conversion is one-way; allocation provenance
/// cannot be forged in the other direction.
#[derive(Debug)]
pub struct AllocatedExtranoncePrefix(ExtranoncePrefix);

/// Tracks an [`ExtranoncePrefix`]'s allocation in an
/// [`ExtranonceAllocator`](super::allocator::ExtranonceAllocator)'s bitmap.
///
/// The [`Weak`] reference means an outstanding prefix does not keep the
/// allocator's bitmap alive past the allocator's own lifetime.
#[derive(Debug)]
struct PrefixAllocation {
    local_index: u32,
    bitmap: Weak<BitVector>,
}

impl ExtranoncePrefix {
    /// Build an [`ExtranoncePrefix`] from bytes received over the wire.
    ///
    /// Intended for client-side channels whose extranonce prefix is chosen
    /// by the upstream server (e.g. via `OpenExtendedMiningChannelSuccess`,
    /// `OpenStandardMiningChannelSuccess`, or `SetExtranoncePrefix`).
    /// The resulting prefix carries no allocation record and dropping it releases no slot.
    ///
    /// Returns [`ExtranoncePrefixError::ExceedsMaxLength`] if `prefix` is
    /// longer than [`MAX_EXTRANONCE_LEN`]. This boundary check ensures the
    /// Sv2 extranonce-length invariant is enforced wherever raw bytes enter
    /// the module.
    #[inline]
    pub fn from_wire(prefix: Vec<u8>) -> Result<Self, ExtranoncePrefixError> {
        if prefix.len() > MAX_EXTRANONCE_LEN as usize {
            return Err(ExtranoncePrefixError::ExceedsMaxLength);
        }
        let upstream_prefix_len = prefix.len() as u8;
        Ok(Self {
            prefix,
            upstream_prefix_len,
            allocation: None,
        })
    }

    /// The raw prefix bytes.
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        &self.prefix
    }

    /// The length of the prefix in bytes.
    #[inline]
    pub fn len(&self) -> usize {
        self.prefix.len()
    }

    /// Whether the prefix is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.prefix.is_empty()
    }

    /// The length of the leading `upstream_prefix` region.
    ///
    /// For prefixes produced by an
    /// [`ExtranonceAllocator`](super::allocator::ExtranonceAllocator), this
    /// is the boundary in the standard
    /// `[upstream_prefix | local_prefix | local_index]` layout.
    ///
    /// For prefixes built via [`from_wire`](Self::from_wire), the entire
    /// prefix is upstream-assigned from the receiving node's perspective, so
    /// this equals [`len`](Self::len).
    ///
    /// Typical use: a proxy that sub-allocates an upstream-assigned
    /// extranonce and then rewrites shares before forwarding them upstream
    /// can slice the downstream channel's prefix at this offset to
    /// separate "bytes the upstream minted" from "bytes the proxy
    /// minted", without needing to keep the producing allocator around.
    #[inline]
    pub fn upstream_prefix_len(&self) -> u8 {
        self.upstream_prefix_len
    }

    /// Bytes retained when replacing the upstream region: `local_prefix | local_index`, plus
    /// any standard-channel rollable padding. Wire-sourced prefixes have no preserved region.
    #[inline]
    pub(crate) fn preserved_len(&self) -> usize {
        self.len() - self.upstream_prefix_len as usize
    }

    /// Replaces the upstream-assigned region of this prefix.
    ///
    /// Prefixes created by an [`ExtranonceAllocator`](super::allocator::ExtranonceAllocator)
    /// record the boundary after `upstream_prefix`. This method preserves `local_prefix |
    /// local_index` (and any standard-channel rollable padding) together with the allocator
    /// ownership record. Keeping that record attached ensures the bitmap slot cannot be reused
    /// while its channel remains live.
    ///
    /// Prefixes built with [`from_wire`](Self::from_wire) have no locally managed region: the
    /// entire opaque wire value is upstream-owned from this node's perspective. For those
    /// prefixes, this method replaces the complete value and keeps it wire-sourced.
    ///
    /// Jobs retain the prefix bytes captured when they were created; this only changes the
    /// current prefix used by future jobs. The prefix is left unchanged if the updated value would
    /// exceed [`MAX_EXTRANONCE_LEN`].
    ///
    /// This value does not track jobs. For channel-owned prefixes, use the channel's
    /// `set_upstream_extranonce_prefix` method, which also retains allocation ownership for
    /// old live jobs across subsequent whole-prefix rotations.
    pub fn set_upstream_prefix(
        &mut self,
        upstream_prefix: &[u8],
    ) -> Result<(), ExtranoncePrefixError> {
        let preserved_bytes = &self.prefix[self.upstream_prefix_len as usize..];
        let updated_len = upstream_prefix.len() + preserved_bytes.len();

        if updated_len > MAX_EXTRANONCE_LEN as usize {
            return Err(ExtranoncePrefixError::ExceedsMaxLength);
        }

        let mut updated_prefix = Vec::with_capacity(updated_len);
        updated_prefix.extend_from_slice(upstream_prefix);
        updated_prefix.extend_from_slice(preserved_bytes);

        self.prefix = updated_prefix;
        self.upstream_prefix_len = upstream_prefix.len() as u8;

        Ok(())
    }

    /// Captures the old bytes before an upstream-only update, sharing their allocation.
    ///
    /// Channels retire this snapshot only after a successful update, using the same live-job
    /// tracking as whole-prefix rotations. Unchanged upstream bytes, wire prefixes and prefixes
    /// whose allocator is gone need no snapshot.
    pub(crate) fn snapshot_for_upstream_update(&self, upstream_prefix: &[u8]) -> Option<Self> {
        if upstream_prefix == &self.prefix[..self.upstream_prefix_len as usize]
            || !self.holds_allocator_slot()
        {
            return None;
        }
        Some(Self {
            prefix: self.prefix.clone(),
            upstream_prefix_len: self.upstream_prefix_len,
            allocation: self.allocation.clone(),
        })
    }

    fn shares_allocation_with(&self, other: &Self) -> bool {
        match (&self.allocation, &other.allocation) {
            (Some(left), Some(right)) => Arc::ptr_eq(left, right),
            _ => false,
        }
    }

    /// Whether this prefix currently reserves a slot in a live allocator's bitmap.
    ///
    /// `false` for wire-sourced prefixes (see [`from_wire`](Self::from_wire)) and for
    /// allocator-produced ones whose allocator has since been dropped: their [`Drop`] is a
    /// no-op, so there is nothing to keep reserved by holding on to them. Channels use this to
    /// decide whether a rotated-out prefix is worth retaining while jobs created under it are
    /// still live.
    #[inline]
    pub fn holds_allocator_slot(&self) -> bool {
        self.allocation
            .as_ref()
            .is_some_and(|allocation| allocation.bitmap.strong_count() > 0)
    }
}

impl AllocatedExtranoncePrefix {
    /// Build an [`AllocatedExtranoncePrefix`] from an allocator's bitmap slot.
    ///
    /// Called internally by
    /// [`ExtranonceAllocator::allocate_extended`](super::allocator::ExtranonceAllocator::allocate_extended)
    /// and
    /// [`allocate_standard`](super::allocator::ExtranonceAllocator::allocate_standard).
    /// The resulting prefix carries a [`PrefixAllocation`] whose [`Drop`]
    /// impl clears the bit in the allocator's bitmap.
    #[inline]
    pub(crate) fn from_allocation(
        local_index: u32,
        upstream_prefix_len: u8,
        prefix: Vec<u8>,
        bitmap: Weak<BitVector>,
    ) -> Self {
        Self(ExtranoncePrefix {
            prefix,
            upstream_prefix_len,
            allocation: Some(Arc::new(PrefixAllocation {
                local_index,
                bitmap,
            })),
        })
    }

    /// Test-only constructor that produces an [`AllocatedExtranoncePrefix`]
    /// carrying no allocation record (its [`Drop`] is a no-op).
    ///
    /// Provided so that server-side channel tests can build a channel
    /// without standing up a real [`ExtranonceAllocator`]. The resulting
    /// value is indistinguishable from an allocator-produced prefix at
    /// the type boundary; it is only reachable under `#[cfg(test)]`.
    ///
    /// Returns [`ExtranoncePrefixError::ExceedsMaxLength`] if `prefix` is
    /// longer than [`MAX_EXTRANONCE_LEN`].
    #[cfg(test)]
    #[inline]
    pub fn for_test(prefix: Vec<u8>) -> Result<Self, ExtranoncePrefixError> {
        Ok(Self(ExtranoncePrefix::from_wire(prefix)?))
    }

    /// The raw prefix bytes.
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    /// The length of the prefix in bytes.
    #[inline]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the prefix is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The length of the `upstream_prefix` region.
    ///
    /// See [`ExtranoncePrefix::upstream_prefix_len`] for the full semantics.
    #[inline]
    pub fn upstream_prefix_len(&self) -> u8 {
        self.0.upstream_prefix_len()
    }
}

impl From<AllocatedExtranoncePrefix> for ExtranoncePrefix {
    #[inline]
    fn from(allocated: AllocatedExtranoncePrefix) -> Self {
        allocated.0
    }
}

impl PartialEq for ExtranoncePrefix {
    fn eq(&self, other: &Self) -> bool {
        self.prefix == other.prefix
    }
}

impl Eq for ExtranoncePrefix {}

impl Drop for PrefixAllocation {
    fn drop(&mut self) {
        if let Some(bitmap) = self.bitmap.upgrade() {
            bitmap.set(self.local_index as usize, false);
        }
    }
}

/// Extranonce prefixes rotated out of a channel that are kept alive while a job created under
/// their bytes can still accept shares.
///
/// Jobs only carry a copy of the prefix bytes they were created under and stay valid across a
/// prefix rotation. Dropping the rotated-out [`ExtranoncePrefix`] right away would return its
/// slot to the allocator while those jobs still validate shares under those bytes, letting the
/// allocator hand the same extranonce space to a second live channel. Holding the object here
/// keeps the slot reserved. The slot is released only when the last owner of its allocation
/// record drops; upstream-only updates can leave several byte snapshots sharing one record.
///
/// Only prefixes that [hold a slot](ExtranoncePrefix::holds_allocator_slot) are ever kept:
/// wire-sourced prefixes and allocator-produced ones whose allocator is gone reserve nothing,
/// and retaining them would let byte-identical rotations grow this set once per update.
/// Snapshots with the same allocation record and bytes are kept only once, so repeated
/// upstream updates cannot grow the set without additional live job prefixes. Byte-identical
/// prefixes from distinct allocations are retained independently to keep both slots reserved.
#[derive(Debug, Default)]
pub(crate) struct RetiredExtranoncePrefixes(Vec<ExtranoncePrefix>);

impl RetiredExtranoncePrefixes {
    /// Takes ownership of a prefix that is no longer a channel's current one.
    ///
    /// `live_prefixes` yields the prefix bytes of every job that can still accept shares. The
    /// prefix is kept only while it holds an allocator slot and one of them matches; otherwise it
    /// drops here, releasing its slot if this was its last allocation owner. The prefixes retired
    /// earlier are pruned against the same `live_prefixes`.
    pub(crate) fn retire<'a>(
        &mut self,
        prefix: ExtranoncePrefix,
        live_prefixes: impl Iterator<Item = &'a [u8]> + Clone,
    ) {
        self.prune(live_prefixes.clone());
        if prefix.holds_allocator_slot()
            && live_prefixes.clone().any(|live| live == prefix.as_bytes())
            && !self
                .0
                .iter()
                .any(|retired| retired == &prefix && retired.shares_allocation_with(&prefix))
        {
            self.0.push(prefix);
        }
    }

    /// Drops every retired prefix whose bytes no entry of `live_prefixes` matches (or whose
    /// allocator has since been dropped), releasing any slots with no remaining allocation owner.
    pub(crate) fn prune<'a>(&mut self, live_prefixes: impl Iterator<Item = &'a [u8]> + Clone) {
        self.0.retain(|prefix| {
            prefix.holds_allocator_slot()
                && live_prefixes.clone().any(|live| live == prefix.as_bytes())
        });
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Errors returned by [`ExtranoncePrefix`] constructors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtranoncePrefixError {
    /// The supplied prefix exceeds [`MAX_EXTRANONCE_LEN`] bytes.
    ExceedsMaxLength,
}

impl core::fmt::Display for ExtranoncePrefixError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ExceedsMaxLength => {
                write!(f, "extranonce prefix exceeds {MAX_EXTRANONCE_LEN} bytes")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extranonce_manager::ExtranonceAllocator;

    #[test]
    fn upstream_snapshots_release_slot_only_after_last_owner_drops() {
        let mut allocator =
            ExtranonceAllocator::from_upstream_prefix(vec![0xaa], vec![0xbb], 6, 1).unwrap();
        let mut current: ExtranoncePrefix = allocator.allocate_extended(3).unwrap().into();
        let first = current.snapshot_for_upstream_update(&[0xcc]).unwrap();
        current.set_upstream_prefix(&[0xcc]).unwrap();
        let second = current.snapshot_for_upstream_update(&[0xdd]).unwrap();
        current.set_upstream_prefix(&[0xdd]).unwrap();
        let live_bytes = [first.as_bytes().to_vec(), second.as_bytes().to_vec()];
        let mut retired = RetiredExtranoncePrefixes::default();
        retired.retire(first, live_bytes.iter().map(Vec::as_slice));
        retired.retire(second, live_bytes.iter().map(Vec::as_slice));
        assert_eq!(retired.len(), 2);
        assert_eq!(allocator.allocated_count(), 1);

        drop(current);
        retired.prune(live_bytes[..1].iter().map(Vec::as_slice));
        assert_eq!(retired.len(), 1);
        assert_eq!(allocator.allocated_count(), 1);
        retired.prune(core::iter::empty());
        assert!(retired.is_empty());
        assert_eq!(allocator.allocated_count(), 0);
        assert!(allocator.allocate_extended(3).is_ok());
    }

    #[test]
    fn repeated_upstream_snapshots_are_deduplicated_by_allocation_and_bytes() {
        let mut allocator =
            ExtranonceAllocator::from_upstream_prefix(vec![0xaa], vec![0xbb], 6, 1).unwrap();
        let mut current: ExtranoncePrefix = allocator.allocate_extended(3).unwrap().into();
        let old_bytes = current.as_bytes().to_vec();
        let mut other_bytes = old_bytes.clone();
        other_bytes[0] = 0xcc;
        let live_bytes = [old_bytes, other_bytes];
        let mut retired = RetiredExtranoncePrefixes::default();

        for _ in 0..100 {
            for upstream in [0xcc, 0xaa] {
                let snapshot = current.snapshot_for_upstream_update(&[upstream]).unwrap();
                current.set_upstream_prefix(&[upstream]).unwrap();
                retired.retire(snapshot, live_bytes.iter().map(Vec::as_slice));
                assert!(retired.len() <= 2);
                assert_eq!(allocator.allocated_count(), 1);
            }
        }
        assert_eq!(retired.len(), 2);

        // Pruning every snapshot must not release the still-current allocation.
        retired.prune(core::iter::empty());
        assert!(retired.is_empty());
        assert_eq!(allocator.allocated_count(), 1);
        drop(current);
        assert_eq!(allocator.allocated_count(), 0);
    }

    #[test]
    fn byte_identical_snapshots_from_distinct_allocators_keep_both_slots() {
        let mut first_allocator =
            ExtranonceAllocator::from_upstream_prefix(vec![0xaa], vec![0xbb], 6, 1).unwrap();
        let mut second_allocator =
            ExtranonceAllocator::from_upstream_prefix(vec![0xaa], vec![0xbb], 6, 1).unwrap();
        let first: ExtranoncePrefix = first_allocator.allocate_extended(3).unwrap().into();
        let second: ExtranoncePrefix = second_allocator.allocate_extended(3).unwrap().into();
        assert_eq!(first.as_bytes(), second.as_bytes());
        let live_bytes = first.as_bytes().to_vec();
        let mut retired = RetiredExtranoncePrefixes::default();
        for current in [first, second] {
            retired.retire(
                current.snapshot_for_upstream_update(&[0xcc]).unwrap(),
                core::iter::once(live_bytes.as_slice()),
            );
        }
        assert_eq!(retired.len(), 2);
        assert_eq!(first_allocator.allocated_count(), 1);
        assert_eq!(second_allocator.allocated_count(), 1);
        retired.prune(core::iter::empty());
        assert_eq!(first_allocator.allocated_count(), 0);
        assert_eq!(second_allocator.allocated_count(), 0);
    }

    #[test]
    fn upstream_snapshots_skip_noops_and_prefixes_without_live_allocators() {
        let wire = ExtranoncePrefix::from_wire(vec![0xaa]).unwrap();
        assert!(wire.snapshot_for_upstream_update(&[0xcc]).is_none());
        let mut allocator =
            ExtranonceAllocator::from_upstream_prefix(vec![0xaa], vec![0xbb], 6, 1).unwrap();
        let current: ExtranoncePrefix = allocator.allocate_extended(3).unwrap().into();
        assert!(current.snapshot_for_upstream_update(&[0xaa]).is_none());
        let snapshot = current.snapshot_for_upstream_update(&[0xcc]).unwrap();
        drop(allocator);
        assert!(!snapshot.holds_allocator_slot());
        assert!(current.snapshot_for_upstream_update(&[0xcc]).is_none());
        let mut retired = RetiredExtranoncePrefixes::default();
        retired.retire(snapshot, core::iter::once(current.as_bytes()));
        assert!(retired.is_empty());
    }

    #[test]
    fn preserved_len_includes_local_index_and_standard_padding() {
        let mut allocator =
            ExtranonceAllocator::from_upstream_prefix(vec![0xaa], vec![0xbb], 6, 256).unwrap();
        let extended: ExtranoncePrefix = allocator.allocate_extended(3).unwrap().into();
        let standard: ExtranoncePrefix = allocator.allocate_standard().unwrap().into();
        let wire = ExtranoncePrefix::from_wire(vec![0xaa, 0xbb]).unwrap();

        assert_eq!(extended.preserved_len(), 2);
        assert_eq!(standard.preserved_len(), 5);
        assert_eq!(wire.preserved_len(), 0);
    }

    #[test]
    fn upstream_prefix_update_preserves_local_regions_and_allocation() {
        let mut allocator =
            ExtranonceAllocator::from_upstream_prefix(vec![0xaa], vec![0xbb], 6, 256).unwrap();
        let mut prefix: ExtranoncePrefix = allocator.allocate_extended(3).unwrap().into();

        prefix.set_upstream_prefix(&[0xcc, 0xdd]).unwrap();

        assert_eq!(prefix.as_bytes(), &[0xcc, 0xdd, 0xbb, 0x00]);
        assert_eq!(prefix.upstream_prefix_len(), 2);
        assert_eq!(allocator.allocated_count(), 1);

        drop(prefix);
        assert_eq!(allocator.allocated_count(), 0);
    }

    #[test]
    fn upstream_prefix_update_replaces_a_wire_sourced_prefix() {
        let mut prefix = ExtranoncePrefix::from_wire(vec![0xaa, 0xbb]).unwrap();
        assert_eq!(prefix.upstream_prefix_len(), 2);

        prefix.set_upstream_prefix(&[0xcc]).unwrap();

        assert_eq!(prefix.as_bytes(), &[0xcc]);
        assert_eq!(prefix.upstream_prefix_len(), 1);
    }

    #[test]
    fn oversized_upstream_prefix_update_is_transactional() {
        let mut allocator =
            ExtranonceAllocator::from_upstream_prefix(vec![0xaa], vec![0xbb], 32, 256).unwrap();
        let mut prefix: ExtranoncePrefix = allocator.allocate_extended(29).unwrap().into();

        assert_eq!(
            prefix.set_upstream_prefix(&[0xcc; 31]),
            Err(ExtranoncePrefixError::ExceedsMaxLength)
        );
        assert_eq!(prefix.as_bytes(), &[0xaa, 0xbb, 0x00]);
        assert_eq!(prefix.upstream_prefix_len(), 1);
        assert_eq!(allocator.allocated_count(), 1);
    }
}
