use alloc::vec::Vec;
use binary_sv2::{binary_codec_sv2, Deserialize, Serialize, U256};
use core::{convert::TryInto, fmt};

/// Message used by upstream to share or distribute the latest block hash.
///
/// This message may be shared by all downstream nodes (sent only once to each channel group).
///
/// Downstream must immediately start to mine on the provided [`SetNewPrevHash::prevhash`].
///
/// When a downstream receives this message, only the job referenced by [`SetNewPrevHash::job_id`]
/// is valid. Remaining jobs have to be dropped.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SetNewPrevHash<'decoder> {
    /// Group channel or channel that this prevhash is valid for.
    pub channel_id: u32,
    /// Job identfier that is to be used for mining.
    ///
    /// A pool may have provided multiple jobs for the next block height (e.g. an empty block or a
    /// block with transactions that are complementary to the set of transactions present in the
    /// current block template).
    pub job_id: u32,
    /// Latest block hash observed by the Template Provider.
    pub prev_hash: U256<'decoder>,
    /// Smallest `nTime` value available for hashing.
    pub min_ntime: u32,
    /// Block header field.
    pub nbits: u32,
}

impl fmt::Display for SetNewPrevHash<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SetNewPrevHash(channel_id={}, job_id={}, prev_hash={}, min_ntime={}, nbits=0x{:08x})",
            self.channel_id, self.job_id, self.prev_hash, self.min_ntime, self.nbits
        )
    }
}
