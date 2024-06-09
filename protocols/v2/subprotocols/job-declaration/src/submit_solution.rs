#[cfg(not(feature = "with_serde"))]
use alloc::vec::Vec;
#[cfg(not(feature = "with_serde"))]
use binary_sv2::binary_codec_sv2;
use binary_sv2::{Deserialize, Serialize, B032, U256};
#[cfg(not(feature = "with_serde"))]
use core::convert::TryInto;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct SubmitSolutionJd<'decoder> {
    /// Extranonce bytes which need to be added to coinbase to form a fully valid submission.
    /// (This is the full extranonce)
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub extranonce: B032<'decoder>,
    /// Hash of the last block
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub prev_hash: U256<'decoder>,
    /// The nTime field in the block header.
    pub ntime: u32,
    /// Nonce leading to the hash being submitted
    pub nonce: u32,
    /// Block header field
    pub nbits: u32,
    /// Header version field      
    pub version: u32,
}

#[cfg(feature = "with_serde")]
use binary_sv2::GetSize;
#[cfg(feature = "with_serde")]
impl<'d> GetSize for SubmitSolutionJd<'d> {
    fn get_size(&self) -> usize {
        self.extranonce.get_size()
            + self.prev_hash.get_size()
            + self.ntime.get_size()
            + self.nonce.get_size()
            + self.nbits.get_size()
    }
}
