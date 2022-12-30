#[cfg(not(feature = "with_serde"))]
use alloc::vec::Vec;
#[cfg(not(feature = "with_serde"))]
use binary_sv2::binary_codec_sv2;
use binary_sv2::{Deserialize, Serialize,B064K};
use core::convert::TryInto;

/// ## CoinbaseOutputDataSize (Client -> Server)
/// Ultimately, the pool is responsible for adding coinbase transaction outputs for payouts and
/// other uses, and thus the Template Provider will need to consider this additional block size
/// when selecting transactions for inclusion in a block (to not create an invalid, oversized block).
/// Thus, this message is used to indicate that some additional space in the block/coinbase
/// transaction be reserved for the pool’s use (while always assuming the pool will use the entirety
/// of available coinbase space).
/// The Job Negotiator MUST discover the maximum serialized size of the additional outputs which
/// will be added by the pool(s) it intends to use this work. It then MUST communicate the
/// maximum such size to the Template Provider via this message. The Template Provider MUST
/// NOT provide NewWork messages which would represent consensus-invalid blocks once this
/// additional size — along with a maximally-sized (100 byte) coinbase field — is added. Further,
/// the Template Provider MUST consider the maximum additional bytes required in the output
/// count variable-length integer in the coinbase transaction when complying with the size limits.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct SetCoinbase<'decoder> {
    // Token valid for the below coinbase. When a downstream send a SetCostumMiningJob the pool
    // check if the token match a valid coinbase if so it respond with SetCostumMiningJob.Success
    pub token: u64,
    /// The maximum additional serialized bytes which the pool will add in
    /// coinbase transaction outputs. This can be extrapoleted from the below fields but for
    /// convenince is here.
    pub coinbase_output_max_additional_size: u32,
    /// Prefix part of the coinbase transaction*.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub coinbase_tx_prefix: B064K<'decoder>,
    /// Suffix part of the coinbase transaction.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub coinbase_tx_suffix: B064K<'decoder>,

}
#[cfg(feature = "with_serde")]
use binary_sv2::GetSize;
#[cfg(feature = "with_serde")]
impl GetSize for SetCoinbase {
    fn get_size(&self) -> usize {
        self.token.get_size()
            + self.coinbase_output_max_additional_size.get_size()
            + self.coinbase_tx_prefix.get_size()
            + self.coinbase_tx_suffix.get_size()
    }
}
