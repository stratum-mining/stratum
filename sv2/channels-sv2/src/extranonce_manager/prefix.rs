extern crate alloc;

use alloc::sync::Weak;
use alloc::vec::Vec;

use super::{bitvector::BitVector, MAX_EXTRANONCE_LEN};

/// An extranonce prefix owned by a channel.
///
/// Carries the raw prefix bytes — accessible via [`as_bytes`](Self::as_bytes) —
/// and, optionally, a [`PrefixAllocation`] record that ties the prefix to an
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
///   These carry a [`PrefixAllocation`] with a [`Weak`] back-reference to the
///   allocator's bitmap; [`Drop`] automatically clears the corresponding bit,
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
/// For allocator-produced prefixes, the allocation is returned to the
/// allocator automatically when the prefix is dropped. Typical usage is
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
/// double-release is impossible; because dropping the prefix always runs
/// its [`Drop`] impl, forgetting to release is impossible.
///
/// If the allocator itself is dropped before an outstanding prefix, the
/// prefix's [`Drop`] becomes a silent no-op (the [`Weak`] reference fails
/// to upgrade). This is safe — the bitmap is gone, so there is nothing to
/// update.
#[derive(Debug)]
pub struct ExtranoncePrefix {
    prefix: Vec<u8>,
    /// `Some(_)` when the prefix was minted by an allocator; `None` when it
    /// was built from wire bytes via [`ExtranoncePrefix::from_wire`].
    allocation: Option<PrefixAllocation>,
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
///
/// `upstream_prefix_len` is captured at allocation time so callers can
/// recover the `[upstream_prefix | local_prefix | local_index]` split from
/// the prefix alone, without having to keep the producing allocator
/// reachable.
#[derive(Debug)]
struct PrefixAllocation {
    local_index: u32,
    upstream_prefix_len: u8,
    bitmap: Weak<BitVector>,
}

impl ExtranoncePrefix {
    /// Build an [`ExtranoncePrefix`] from bytes received over the wire.
    ///
    /// Intended for client-side channels whose extranonce prefix is chosen
    /// by the upstream server (e.g. via `OpenExtendedMiningChannelSuccess`,
    /// `OpenStandardMiningChannelSuccess`, or `SetExtranoncePrefix`).
    /// The resulting prefix carries no allocation record and its [`Drop`]
    /// impl is a silent no-op.
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
        Ok(Self {
            prefix,
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

    /// The length of the `upstream_prefix` region, if known.
    ///
    /// Returns `Some(n)` for prefixes produced by an
    /// [`ExtranonceAllocator`](super::allocator::ExtranonceAllocator): `n`
    /// is the number of bytes at the start of [`as_bytes`](Self::as_bytes)
    /// that were assigned by the upstream (i.e. the
    /// `[upstream_prefix]` portion of the standard
    /// `[upstream_prefix | local_prefix | local_index]` layout).
    ///
    /// Returns `None` for prefixes built via
    /// [`from_wire`](Self::from_wire), because wire bytes carry no
    /// intra-prefix layout information — the entire slice is semantically
    /// "opaque upstream bytes" to whoever received it.
    ///
    /// Typical use: a proxy that sub-allocates an upstream-assigned
    /// extranonce and then rewrites shares before forwarding them upstream
    /// can slice the downstream channel's prefix at this offset to
    /// separate "bytes the upstream minted" from "bytes the proxy
    /// minted", without needing to keep the producing allocator around.
    #[inline]
    pub fn upstream_prefix_len(&self) -> Option<u8> {
        self.allocation.as_ref().map(|a| a.upstream_prefix_len)
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
            allocation: Some(PrefixAllocation {
                local_index,
                upstream_prefix_len,
                bitmap,
            }),
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
    /// See [`ExtranoncePrefix::upstream_prefix_len`] for the semantics of
    /// the returned value. Because an [`AllocatedExtranoncePrefix`] is
    /// always allocator-produced, the length is always known and this
    /// accessor returns a bare `u8` rather than an `Option`.
    #[inline]
    pub fn upstream_prefix_len(&self) -> u8 {
        self.0
            .upstream_prefix_len()
            .expect("AllocatedExtranoncePrefix always carries an allocation record")
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

impl Drop for ExtranoncePrefix {
    fn drop(&mut self) {
        if let Some(allocation) = &self.allocation {
            if let Some(bitmap) = allocation.bitmap.upgrade() {
                bitmap.set(allocation.local_index as usize, false);
            }
        }
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
