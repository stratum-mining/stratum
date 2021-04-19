use serde::{Deserialize, Serialize};
use serde_sv2::{B064K, U32, U64};

/// ## SubmitSolution (Client -> Server)
/// Upon finding a coinbase transaction/nonce pair which double-SHA256 hashes at or below
/// [`crate::SetNewPrevHash.target`], the client MUST immediately send this message, and the server
/// MUST then immediately construct the corresponding full block and attempt to propagate it to
/// the Bitcoin network.
#[derive(Serialize, Deserialize, Debug)]
pub struct SubmitSolution<'a> {
    /// The template_id field as it appeared in NewTemplate.
    template_id: U64,
    /// The version field in the block header. Bits not defined by [BIP320](TODO link) as
    /// additional nonce MUST be the same as they appear in the [NewWork](TODO link)
    /// message, other bits may be set to any value.
    version: U32,
    /// The nTime field in the block header. This MUST be greater than or equal
    /// to the header_timestamp field in the latest [`crate::SetNewPrevHash`] message
    /// and lower than or equal to that value plus the number of seconds since
    /// the receipt of that message.
    header_timestamp: U32,
    /// The nonce field in the header.
    header_nonce: U32,
    /// The full serialized coinbase transaction, meeting all the requirements of
    /// the NewWork message, above.
    #[serde(borrow)]
    coinbase_tx: B064K<'a>,
}
