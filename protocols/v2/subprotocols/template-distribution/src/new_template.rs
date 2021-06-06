#[cfg(not(feature = "with_serde"))]
use alloc::vec::Vec;
#[cfg(not(feature = "with_serde"))]
use binary_sv2::codec;
use binary_sv2::{Deserialize, Serialize};
use binary_sv2::{Seq0255, B0255, B064K, U256};

///// ## NewTemplate (Server -> Client)
///// The primary template-providing function. Note that the coinbase_tx_outputs bytes will appear
///// as is at the end of the coinbase transaction.
#[derive(Serialize, Deserialize, Debug)]
pub struct NewTemplate<'decoder> {
    /// Server’s identification of the template. Strictly increasing, the
    /// current UNIX time may be used in place of an ID.
    template_id: u64,
    /// True if the template is intended for future [`crate::SetNewPrevHash`]
    /// message sent on the channel. If False, the job relates to the last
    /// sent [`crate::SetNewPrevHash`] message on the channel and the miner
    /// should start to work on the job immediately.
    future_template: bool,
    /// Valid header version field that reflects the current network
    /// consensus. The general purpose bits (as specified in [BIP320](TODO link)) can
    /// be freely manipulated by the downstream node. The downstream
    /// node MUST NOT rely on the upstream node to set the BIP320 bits
    /// to any particular value.
    version: u32,
    /// The coinbase transaction nVersion field.
    coinbase_tx_version: u32,
    /// Up to 8 bytes (not including the length byte) which are to be placed
    /// at the beginning of the coinbase field in the coinbase transaction.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    coinbase_prefix: B0255<'decoder>,
    /// The coinbase transaction input’s nSequence field.
    coinbase_tx_input_sequence: u32,
    /// The value, in satoshis, available for spending in coinbase outputs
    /// added by the client. Includes both transaction fees and block
    /// subsidy.
    coinbase_tx_value_remaining: u64,
    /// The number of transaction outputs included in coinbase_tx_outputs.
    coinbase_tx_outputs_count: u32,
    /// Bitcoin transaction outputs to be included as the last outputs in the
    /// coinbase transaction.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    coinbase_tx_outputs: B064K<'decoder>,
    /// The locktime field in the coinbase transaction.
    coinbase_tx_locktime: u32,
    /// Merkle path hashes ordered from deepest.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    merkle_path: Seq0255<'decoder, U256<'decoder>>,
}
