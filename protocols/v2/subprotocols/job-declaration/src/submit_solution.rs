#[cfg(not(feature = "with_serde"))]
use alloc::vec::Vec;
#[cfg(not(feature = "with_serde"))]
use binary_sv2::binary_codec_sv2;
use binary_sv2::{Deserialize, Serialize, B032, U256};
#[cfg(not(feature = "with_serde"))]
use core::convert::TryInto;

/// TODO: comment
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct SubmitSolutionJd<'decoder> {
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub extranonce: B032<'decoder>,
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub prev_hash: U256<'decoder>,
    pub ntime: u32,
    pub nonce: u32,
    pub nbits: u32,
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
