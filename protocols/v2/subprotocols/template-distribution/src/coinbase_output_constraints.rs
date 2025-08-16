use alloc::{fmt, vec::Vec};
use binary_sv2::{binary_codec_sv2, Deserialize, Serialize};
use core::convert::TryInto;

/// Message used by a downstream to indicate the size of the additional bytes they will need in
/// coinbase transaction outputs.
///
/// As the pool is responsible for adding coinbase transaction outputs for payouts and other uses,
/// the Template Provider will need to consider this reserved space when selecting transactions for
/// inclusion in a block(to avoid an invalid, oversized block).  Thus, this message indicates that
/// additional space in the block/coinbase transaction must be reserved for, assuming they will use
/// the entirety of this space.
///
/// The Job Declarator **must** discover the maximum serialized size of the additional outputs which
/// will be added by the pools it intends to use this work. It then **must** communicate the sum of
/// such size to the Template Provider via this message.
///
/// The Template Provider **must not** provide [`NewTemplate`] messages which would represent
/// consensus-invalid blocks once this additional size — along with a maximally-sized (100 byte)
/// coinbase field — is added. Further, the Template Provider **must** consider the maximum
/// additional bytes required in the output count variable-length integer in the coinbase
/// transaction when complying with the size limits.
///
/// [`NewTemplate`]: crate::NewTemplate
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq)]

pub struct CoinbaseOutputConstraints {
    /// Additional serialized bytes needed in coinbase transaction outputs.
    pub coinbase_output_max_additional_size: u32,
    /// Additional sigops needed in coinbase transaction outputs.
    pub coinbase_output_max_additional_sigops: u16,
}

impl fmt::Display for CoinbaseOutputConstraints {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "CoinbaseOutputConstraints(coinbase_output_max_additional_size: {}, coinbase_output_max_additional_sigops: {})",
            self.coinbase_output_max_additional_size,
            self.coinbase_output_max_additional_sigops
        )
    }
}
