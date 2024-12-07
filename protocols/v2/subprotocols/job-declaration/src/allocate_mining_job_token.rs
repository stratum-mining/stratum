#[cfg(not(feature = "with_serde"))]
use alloc::vec::Vec;
#[cfg(not(feature = "with_serde"))]
use binary_sv2::binary_codec_sv2;
use binary_sv2::{Deserialize, Serialize, Str0255, B0255, B064K};
#[cfg(not(feature = "with_serde"))]
use core::convert::TryInto;

/// Message used by JDC to request an identifier for a future mining job from JDS.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct AllocateMiningJobToken<'decoder> {
    /// Unconstrained sequence of bytes. Whatever is needed by the JDS to
    /// identify/authenticate the client. Additional restrictions can be imposed by the
    /// JDS. It is highly recommended that UTF-8 encoding is used.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub user_identifier: Str0255<'decoder>,
    /// A unique identifier for pairing the response/request.
    pub request_id: u32,
}

/// Message used by JDS to accept [`AllocateMiningJobToken`] message.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct AllocateMiningJobTokenSuccess<'decoder> {
    /// A unique identifier for pairing the response/request.
    ///
    /// This **must** be the same as the received [`AllocateMiningJobToken::request_id`].
    pub request_id: u32,
    /// A token that makes the JDC eligible for committing a mining job for approval/transactions
    /// declaration or for identifying custom mining job on mining connection.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub mining_job_token: B0255<'decoder>,
    /// The maximum additional size that can be added to the coinbase output.
    ///
    /// The maximum additional serialized bytes which the JDS will add in coinbase transaction
    /// outputs.
    pub coinbase_output_max_additional_size: u32,
    /// Bitcoin transaction outputs added by JDS.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub coinbase_output: B064K<'decoder>,
    /// Whether the client is allowed to mine asynchronously.
    ///
    /// If set to true, the [`AllocateMiningJobTokenSuccess::mining_job_token`] can be used
    /// immediately on a mining connection in the `SetCustomMiningJob` message, even before
    /// [`crate::DeclareMiningJob`] and [`DeclareMiningJobSuccess`] messages have been exchanged.
    ///
    /// If set to false, JDC **must** use this token for [`crate::DeclareMiningJob`] only.
    ///
    /// This **must** be set to true if `SetupConnection::flags` included
    /// `REQUIRES_ASYNC_JOB_MINING`.
    pub async_mining_allowed: bool,
}

#[cfg(feature = "with_serde")]
use binary_sv2::GetSize;
#[cfg(feature = "with_serde")]
impl<'d> GetSize for AllocateMiningJobToken<'d> {
    fn get_size(&self) -> usize {
        self.user_identifier.get_size() + self.request_id.get_size()
    }
}
#[cfg(feature = "with_serde")]
impl<'d> GetSize for AllocateMiningJobTokenSuccess<'d> {
    fn get_size(&self) -> usize {
        self.request_id.get_size()
            + self.mining_job_token.get_size()
            + self.coinbase_output_max_additional_size.get_size()
            + self.coinbase_output.get_size()
            + self.async_mining_allowed.get_size()
    }
}
