use serde::{Deserialize, Serialize};
use serde_sv2::{Seq064K, Str0255, B016M, B064K, U64};

/// ## RequestTransactionData (Client -> Server)
/// A request sent by the Job Negotiator to the Template Provider which requests the set of
/// transaction data for all transactions (excluding the coinbase transaction) included in a block, as
/// well as any additional data which may be required by the Pool to validate the work.
#[derive(Serialize, Deserialize, Debug)]
pub struct RequestTransactionData {
    /// The template_id corresponding to a NewTemplate message.
    template_id: U64,
}

/// ## RequestTransactionData.Success (Server->Client)
/// A response to [`RequestTransactionData`] which contains the set of full transaction data and
/// excess data required for validation. For practical purposes, the excess data is usually the
/// SegWit commitment, however the Job Negotiator MUST NOT parse or interpret the excess data
/// in any way. Note that the transaction data MUST be treated as opaque blobs and MUST include
/// any SegWit or other data which the Pool may require to verify the transaction. For practical
/// purposes, the transaction data is likely the witness-encoded transaction today. However, to
/// ensure backward compatibility, the transaction data MAY be encoded in a way that is different
/// from the consensus serialization of Bitcoin transactions.
/// Ultimately, having some method of negotiating the specific format of transactions between the
/// Template Provider and the Pool’s Template verification node would be overly burdensome,
/// thus the following requirements are made explicit. The RequestTransactionData.Success
/// sender MUST ensure that the data is provided in a forwards- and backwards-compatible way to
/// ensure the end receiver of the data can interpret it, even in the face of new,
/// consensus-optional data. This allows significantly more flexibility on both the
/// RequestTransactionData.Success-generating and -interpreting sides during upgrades, at the
/// cost of breaking some potential optimizations which would require version negotiation to
/// provide support for previous versions. For practical purposes, and as a non-normative
/// suggested implementation for Bitcoin Core, this implies that additional consensus-optional
/// data be appended at the end of transaction data. It will simply be ignored by versions which do
/// not understand it.
/// To work around the limitation of not being able to negotiate e.g. a transaction compression
/// scheme, the format of the opaque data in RequestTransactionData.Success messages MAY be
/// changed in non-compatible ways at the time a fork activates, given sufficient time from
/// code-release to activation (as any sane fork would have to have) and there being some
/// in-Template Negotiation Protocol signaling of support for the new fork (e.g. for soft-forks
/// activated using [BIP 9](TODO link)).
#[derive(Serialize, Deserialize, Debug)]
pub struct RequestTransactionDataSuccess<'a> {
    /// The template_id corresponding to a NewTemplate/RequestTransactionData message.
    template_id: U64,
    /// Extra data which the Pool may require to validate the work.
    #[serde(borrow)]
    excess_data: B064K<'a>,
    /// The transaction data, serialized as a series of B0_16M byte arrays.
    #[serde(borrow)]
    transaction_list: Seq064K<'a, B016M<'a>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RequestTransactionDataError {
    /// The template_id corresponding to a NewTemplate/RequestTransactionData message.
    template_id: U64,
    /// Reason why no transaction data has been provided
    /// Possible error codes:
    /// * template-id-not-found
    error_code: Str0255,
}
