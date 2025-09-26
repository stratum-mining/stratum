use alloc::{fmt, vec::Vec};
use binary_sv2::{binary_codec_sv2, Deserialize, Serialize, B064K};
use core::convert::TryInto;

/// Message used by a downstream to submit a successful solution to a previously provided template.
///
/// The downstream is expected to send this message in addition to the `SubmitSolution` message
/// from the Mining Protocol in order to propagate the solution to the Bitcoin network as soon as
/// possible.
///
/// Upon receiving this message, upstream(Template Provider) **must** immediately construct the
/// corresponding full block and attempt to propagate it to the Bitcoin network.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SubmitSolution<'decoder> {
    /// Identifies the template to which this solution corresponds.
    ///
    /// This is acquired from the [`crate::NewTemplate`] message.
    pub template_id: u64,
    /// Version field in the block header.
    ///
    /// Bits not defined by
    /// [BIP320](https://github.com/bitcoin/bips/blob/master/bip-0320.mediawiki) as additional
    /// nonce **must** be the same as they appear in the `NewMiningJob` or `NewExtendedMiningJob`
    /// message, other bits may be set to any value.
    pub version: u32,
    /// nTime field in the block header.
    ///
    /// This **must** be greater than or equal to previously received
    /// [`crate::SetNewPrevHash::header_timestamp`] and lower than or equal to that value plus the
    /// number of seconds since receiving [`crate::SetNewPrevHash`] that message.
    pub header_timestamp: u32,
    /// Nonce field in the header.
    pub header_nonce: u32,
    /// Full serialized coinbase transaction, meeting all the requirements of the `NewMiningJob` or
    /// `NewExtendedMiningJob` message.
    pub coinbase_tx: B064K<'decoder>,
}

impl fmt::Display for SubmitSolution<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SubmitSolution {{ template_id: {}, version: 0x{:08x}, header_timestamp: {}, header_nonce: 0x{:08x}, coinbase_tx: {} }}",
            self.template_id,
            self.version,
            self.header_timestamp,
            self.header_nonce,
            self.coinbase_tx
        )
    }
}
