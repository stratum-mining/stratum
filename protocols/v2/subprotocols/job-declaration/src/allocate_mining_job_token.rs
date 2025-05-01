use alloc::vec::Vec;
use binary_sv2::{binary_codec_sv2, Deserialize, Serialize, Str0255, B0255, B064K};
use core::convert::TryInto;

/// Message used by JDC to request an identifier for a future mining job from JDS.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct AllocateMiningJobToken<'decoder> {
    /// Unconstrained sequence of bytes. Whatever is needed by the JDS to
    /// identify/authenticate the client. Additional restrictions can be imposed by the
    /// JDS. It is highly recommended that UTF-8 encoding is used.
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
    pub mining_job_token: B0255<'decoder>,
    /// The maximum additional serialized bytes which the JDS will add in coinbase transaction
    /// outputs.
    pub coinbase_output_max_additional_size: u32,
    /// The maximum additional sigops which the JDS will add in coinbase transaction
    /// outputs.
    pub coinbase_output_max_additional_sigops: u16,
    /// Bitcoin transaction outputs added by JDS.
    pub coinbase_output: B064K<'decoder>,
}
