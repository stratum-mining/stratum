#[cfg(not(feature = "with_serde"))]
use alloc::vec::Vec;
#[cfg(not(feature = "with_serde"))]
use binary_sv2::binary_codec_sv2;
use binary_sv2::{Deserialize, Serialize, U256};
#[cfg(not(feature = "with_serde"))]
use core::convert::TryInto;

/// # SetNewPrevHash (Server -> Client, broadcast)
///
/// Prevhash is distributed whenever a new block is detected in the network by an upstream node.
/// This message MAY be shared by all downstream nodes (sent only once to each channel group).
/// Clients MUST immediately start to mine on the provided prevhash. When a client receives this
/// message, only the job referenced by Job ID is valid. The remaining jobs already queued by the
/// client have to be made invalid.
/// Note: There is no need for block height in this message.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SetNewPrevHash<'decoder> {
    /// Group channel or channel that this prevhash is valid for.
    pub channel_id: u32,
    /// ID of a job that is to be used for mining with this prevhash. A pool may
    /// have provided multiple jobs for the next block height (e.g. an empty
    /// block or a block with transactions that are complementary to the set of
    /// transactions present in the current block template).
    pub job_id: u32,
    /// Previous block’s hash, block header field.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub prev_hash: U256<'decoder>,
    /// Smallest nTime value available for hashing.
    pub min_ntime: u32,
    /// Block header field.
    pub nbits: u32,
}

#[cfg(feature = "with_serde")]
use binary_sv2::GetSize;
#[cfg(feature = "with_serde")]
impl<'d> GetSize for SetNewPrevHash<'d> {
    fn get_size(&self) -> usize {
        self.channel_id.get_size()
            + self.job_id.get_size()
            + self.prev_hash.get_size()
            + self.min_ntime.get_size()
            + self.nbits.get_size()
    }
}
#[cfg(feature = "with_serde")]
impl<'a> SetNewPrevHash<'a> {
    pub fn into_static(self) -> SetNewPrevHash<'static> {
        panic!("This function shouldn't be called by the Messaege Generator");
    }
    pub fn as_static(&self) -> SetNewPrevHash<'static> {
        panic!("This function shouldn't be called by the Messaege Generator");
    }
}
