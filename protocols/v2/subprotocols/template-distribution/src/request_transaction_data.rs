use alloc::{fmt, vec::Vec};
use binary_sv2::{
    binary_codec_sv2::{self, free_vec, free_vec_2, CVec, CVec2},
    Deserialize, Error, Seq064K, Serialize, Str0255, B016M, B064K,
};
use core::convert::TryInto;

/// Message used by a downstream to request data about all transactions in a block template.
///
/// Data includes the full transaction data and any additional data required to block validation.
///
/// Note that the coinbase transaction is excluded from this data.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Copy)]
#[repr(C)]
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

/// C representation of [`RequestTransactionDataSuccess`].
#[repr(C)]
pub struct CRequestTransactionDataSuccess {
    template_id: u64,
    excess_data: CVec,
    transaction_list: CVec2,
}

impl<'a> CRequestTransactionDataSuccess {
    /// Converts C struct to Rust struct.
    #[allow(clippy::wrong_self_convention)]
    pub fn to_rust_rep_mut(&'a mut self) -> Result<RequestTransactionDataSuccess<'a>, Error> {
        let excess_data: B064K = self.excess_data.as_mut_slice().try_into()?;
        let transaction_list_ = self.transaction_list.as_mut_slice();
        let mut transaction_list: Vec<B016M> = Vec::new();
        for cvec in transaction_list_ {
            transaction_list.push(cvec.as_mut_slice().try_into()?);
        }
        let transaction_list = Seq064K::new(transaction_list)?;
        Ok(RequestTransactionDataSuccess {
            template_id: self.template_id,
            excess_data,
            transaction_list,
        })
    }
}

/// Drops the CRequestTransactionDataSuccess object.
#[no_mangle]
pub extern "C" fn free_request_tx_data_success(s: CRequestTransactionDataSuccess) {
    drop(s)
}

impl Drop for CRequestTransactionDataSuccess {
    fn drop(&mut self) {
        free_vec(&mut self.excess_data);
        free_vec_2(&mut self.transaction_list);
    }
}

impl<'a> From<RequestTransactionDataSuccess<'a>> for CRequestTransactionDataSuccess {
    fn from(v: RequestTransactionDataSuccess<'a>) -> Self {
        Self {
            template_id: v.template_id,
            excess_data: v.excess_data.into(),
            transaction_list: v.transaction_list.into(),
        }
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

/// C representation of [`RequestTransactionDataError`].
#[repr(C)]
pub struct CRequestTransactionDataError {
    template_id: u64,
    error_code: CVec,
}

impl<'a> CRequestTransactionDataError {
    /// Converts C struct to Rust struct.
    #[allow(clippy::wrong_self_convention)]
    pub fn to_rust_rep_mut(&'a mut self) -> Result<RequestTransactionDataError<'a>, Error> {
        let error_code: Str0255 = self.error_code.as_mut_slice().try_into()?;
        Ok(RequestTransactionDataError {
            template_id: self.template_id,
            error_code,
        })
    }
}

/// Drops the CRequestTransactionDataError object.
#[no_mangle]
pub extern "C" fn free_request_tx_data_error(s: CRequestTransactionDataError) {
    drop(s)
}

impl Drop for CRequestTransactionDataError {
    fn drop(&mut self) {
        free_vec(&mut self.error_code);
    }
}

impl<'a> From<RequestTransactionDataError<'a>> for CRequestTransactionDataError {
    fn from(v: RequestTransactionDataError<'a>) -> Self {
        Self {
            template_id: v.template_id,
            error_code: v.error_code.into(),
        }
    }
}
