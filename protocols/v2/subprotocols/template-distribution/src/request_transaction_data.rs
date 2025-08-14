use alloc::{fmt, vec::Vec};
use binary_sv2::{binary_codec_sv2, Deserialize, Seq064K, Serialize, Str0255, B016M, B064K};
use core::convert::TryInto;

/// Message used by a downstream to request data about all transactions in a block template.
///
/// Data includes the full transaction data and any additional data required to block validation.
///
/// Note that the coinbase transaction is excluded from this data.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Copy)]

pub struct RequestTransactionData {
    /// Identifier of the template that the downstream node is requesting transaction data for.
    ///
    /// This must be identical to previously exchanged [`crate::NewTemplate::template_id`].
    pub template_id: u64,
}

impl fmt::Display for RequestTransactionData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "RequestTransactionData(template_id: {})",
            self.template_id
        )
    }
}

/// Message used by an upstream(Template Provider) to respond successfully to a
/// [`RequestTransactionData`] message.
///
/// A response to [`RequestTransactionData`] which contains the set of full transaction data and
/// excess data required for block validation. For practical purposes, the excess data is usually
/// the SegWit commitment, however the Job Declarator **must not** have any assumptions about it.
///
/// Note that the transaction data **must** be treated as opaque blobs and **must** include any
/// SegWit or other data which the downstream may require to verify the transaction. For practical
/// purposes, the transaction data is likely the witness-encoded transaction today. However, to
/// ensure backward compatibility, the transaction data **may** be encoded in a way that is
/// different from the consensus serialization of Bitcoin transactions.
///
/// The [`RequestTransactionDataSuccess`] sender **must** ensure that provided data is forward and
/// backward compatible. This way the receiver of the data can interpret it, even in the face of
/// new, consensus-optional data.  This allows significantly more flexibility on both the
/// [`RequestTransactionDataSuccess`] generating and interpreting sides during upgrades, at the
/// cost of breaking some potential optimizations which would require version negotiation to
/// provide support for previous versions.
///
/// Having some method of negotiating the specific format of transactions between the Template
/// Provider and the downstream would be helpful but overly burdensome, thus the above requirements
/// are made explicit.
///
/// As a result, and as a non-normative suggested implementation for Bitcoin Core, this implies
/// that additional consensus-optional data appended at the end of transaction data will simply be
/// ignored by versions which do not understand it.
///
/// To work around the limitation of not being able to negotiate e.g. a transaction compression
/// scheme, the format of the opaque data in [`RequestTransactionDataSuccess`] messages **may** be
/// changed in a non-compatible way at the time of fork activation, given sufficient time from
/// code-release to activation and there being in protocol(Template Declaration) signaling of
/// support for the new fork (e.g. for soft-forks activated using [BIP 9]).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct RequestTransactionDataSuccess<'decoder> {
    /// The template_id corresponding to a NewTemplate/RequestTransactionData message.
    pub template_id: u64,
    /// Extra data which the Pool may require to validate the work.
    pub excess_data: B064K<'decoder>,
    /// The transaction data, serialized as a series of B0_16M byte arrays.
    pub transaction_list: Seq064K<'decoder, B016M<'decoder>>,
}

impl fmt::Display for RequestTransactionDataSuccess<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "RequestTransactionDataSuccess(template_id: {}, excess_data: {}, transaction_list: {})",
            self.template_id, self.excess_data, self.transaction_list
        )
    }
}

/// Message used by an upstream(Template Provider) to respond with an error to a
/// [`RequestTransactionData`] message.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct RequestTransactionDataError<'decoder> {
    /// Identifier of the template that the downstream node is requesting transaction data for.
    pub template_id: u64,
    /// Reason why no transaction data has been provided.
    ///
    /// Possible error codes:
    /// - template-id-not-found
    pub error_code: Str0255<'decoder>,
}

impl fmt::Display for RequestTransactionDataError<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "RequestTransactionDataError(template_id: {}, error_code: {})",
            self.template_id,
            self.error_code.as_utf8_or_hex()
        )
    }
}
