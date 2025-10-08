use alloc::{fmt, vec::Vec};
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

impl fmt::Display for AllocateMiningJobToken<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "AllocateMiningJobToken(user_identifier: {}, request_id: {})",
            self.user_identifier.as_utf8_or_hex(),
            self.request_id
        )
    }
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
    /// Bitcoin transaction outputs added by JDS.
    pub coinbase_outputs: B064K<'decoder>,
}

impl fmt::Display for AllocateMiningJobTokenSuccess<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "AllocateMiningJobTokenSuccess(request_id: {}, mining_job_token: {}, coinbase_outputs: {})",
            self.request_id,
            self.mining_job_token.as_hex(),
            self.coinbase_outputs
        )
    }
}
