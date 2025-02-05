use alloc::vec::Vec;

use binary_sv2::{binary_codec_sv2, Deserialize, Serialize, B032};

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
    pub extranonce_prefix: B032<'decoder>,
}
