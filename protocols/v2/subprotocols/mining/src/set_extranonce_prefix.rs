#[cfg(not(feature = "with_serde"))]
use alloc::vec::Vec;
#[cfg(not(feature = "with_serde"))]
use binary_sv2::binary_codec_sv2;
use binary_sv2::{Deserialize, Serialize, B032};
#[cfg(not(feature = "with_serde"))]
use core::convert::TryInto;

/// Message used by upstream to change downstream node’s extranonce prefix.
///
/// [`SetExtranoncePrefix::extranonce_prefix`], a constant, is part of the full extranonce and is
/// set by the upstream.
///
/// Note that this message is applicable only for opened Standard or Extended Channels, not Group
/// Channels.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SetExtranoncePrefix<'decoder> {
    /// Extended or Standard Channel identifier.
    pub channel_id: u32,
    /// New extranonce prefix.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub extranonce_prefix: B032<'decoder>,
}
#[cfg(feature = "with_serde")]
use binary_sv2::GetSize;
#[cfg(feature = "with_serde")]
impl<'d> GetSize for SetExtranoncePrefix<'d> {
    fn get_size(&self) -> usize {
        self.channel_id.get_size() + self.extranonce_prefix.get_size()
    }
}
#[cfg(feature = "with_serde")]
impl<'a> SetExtranoncePrefix<'a> {
    pub fn into_static(self) -> SetExtranoncePrefix<'static> {
        panic!("This function shouldn't be called by the Message Generator");
    }
    pub fn as_static(&self) -> SetExtranoncePrefix<'static> {
        panic!("This function shouldn't be called by the Message Generator");
    }
}
