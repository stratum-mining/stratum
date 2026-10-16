use alloc::{fmt, vec::Vec};
use binary_sv2::{Deserialize, Seq064K, Serialize, Str0255, B0255, B064K, U256};
use core::convert::TryInto;

/// Message used by JDC to proposes a selected set of transactions to JDS they wish to
/// mine on.
///
/// Used only under [`Full Template`] mode.
///
/// [`Full Template`]: https://github.com/stratum-mining/sv2-spec/blob/main/06-Job-Declaration-Protocol.md#632-full-template-mode
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct DeclareMiningJob<'decoder> {
    /// A unique identifier for this request.
    ///
    /// Used for pairing request/response.
    pub request_id: u32,
    /// Token received previously through [`crate::AllocateMiningJobTokenSuccess`] message.
    ///
    /// Not a secret. The token is a capability scoped to one job declaration, and presenting it
    /// requires an authenticated connection as the JDC it was issued to, so a copy recovered from
    /// a log grants no access on its own. It is therefore printed in full rather than redacted:
    /// the value is what correlates a declaration across JDC, JDS and pool logs.
    pub mining_job_token: B0255<'decoder>,
    /// Header version field.
    pub version: u32,
    /// Serialized bytes representing the initial part of the coinbase transaction (not including
    /// extranonce)
    pub coinbase_tx_prefix: B064K<'decoder>,
    /// Serialized bytes representing the final part of the coinbase transaction (after extranonce)
    pub coinbase_tx_suffix: B064K<'decoder>,
    /// List of wtxid contained in the template. JDS checks the list against its
    /// mempool and requests missing txs via [`crate::ProvideMissingTransactions`].
    ///
    /// This list Does not include the coinbase transaction (as there is no corresponding full data
    /// for it yet).
    pub wtxid_list: Seq064K<'decoder, U256<'decoder>>,
    /// Extra data which the JDS may require to validate the work.
    pub excess_data: B064K<'decoder>,
}

impl fmt::Display for DeclareMiningJob<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "DeclareMiningJob(request_id: {}, mining_job_token: {}, version: 0x{:08x}, coinbase_tx_prefix: {}, coinbase_tx_suffix: {}, tx_ids_list: {}, excess_data: {})",
            self.request_id,
            self.mining_job_token.as_hex(),
            self.version,
            self.coinbase_tx_prefix,
            self.coinbase_tx_suffix,
            self.wtxid_list,
            self.excess_data
        )
    }
}

impl fmt::Display for DeclareMiningJobOwned {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "DeclareMiningJob(request_id: {}, mining_job_token: {}, version: 0x{:08x}, coinbase_tx_prefix: {}, coinbase_tx_suffix: {}, tx_ids_list: {}, excess_data: {})",
            self.request_id,
            self.mining_job_token.as_hex(),
            self.version,
            self.coinbase_tx_prefix,
            self.coinbase_tx_suffix,
            self.wtxid_list,
            self.excess_data
        )
    }
}

/// Messaged used by JDS to accept [`DeclareMiningJob`] message.
///
/// If [`Full Template`] mode is used, JDS MAY request txdata via `ProvideMissingTransactions`
/// before making this commitment.
///
/// [`Full Template`]: https://github.com/stratum-mining/sv2-spec/blob/main/06-Job-Declaration-Protocol.md#632-full-template-mode
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct DeclareMiningJobSuccess<'decoder> {
    /// A unique identifier for this request.
    ///
    /// Must be the same as the received [`DeclareMiningJob::request_id`].
    pub request_id: u32,
    /// This **may** be the same token as [DeclareMiningJob::mining_job_token] if the pool allows
    /// to start mining on a non declared job. If the token is different (irrespective of if the
    /// downstream is already mining using it), the downstream **must** send a `SetCustomMiningJob`
    /// message on each connection which wishes to mine using the declared job.
    ///
    /// Not a secret. The token is a capability scoped to one job declaration, and presenting it
    /// requires an authenticated connection as the JDC it was issued to, so a copy recovered from
    /// a log grants no access on its own. It is therefore printed in full rather than redacted:
    /// the value is what correlates a declaration across JDC, JDS and pool logs.
    pub new_mining_job_token: B0255<'decoder>,
}

impl fmt::Display for DeclareMiningJobSuccess<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "DeclareMiningJobSuccess(request_id: {}, new_mining_job_token: {})",
            self.request_id,
            self.new_mining_job_token.as_hex()
        )
    }
}

impl fmt::Display for DeclareMiningJobSuccessOwned {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "DeclareMiningJobSuccess(request_id: {}, new_mining_job_token: {})",
            self.request_id,
            self.new_mining_job_token.as_hex()
        )
    }
}

/// Messaged used by JDS to reject [`DeclareMiningJob`] message.
///
/// Downstream should consider this as a trigger to fallback into some other Pool/JDS or solo
/// mining.
///
/// Decoding fails if [`DeclareMiningJobError::error_code`] contains control characters, see
/// [`DeclareMiningJobError::validate`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
#[validate]
pub struct DeclareMiningJobError<'decoder> {
    /// The unique identifier of the request.
    ///
    /// Must be the same as the received [`DeclareMiningJob::request_id`].
    pub request_id: u32,
    /// Possible values:
    ///
    /// - invalid-mining-job-token
    /// - missing-txs
    /// - invalid-coinbase-tx
    /// - invalid-coinbase-tx-input
    /// - internal-error
    /// - stale-chain-tip
    /// - invalid-job
    pub error_code: Str0255<'decoder>,
    /// Optional details about the error.
    pub error_details: B064K<'decoder>,
}

impl fmt::Display for DeclareMiningJobError<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "DeclareMiningJobError(request_id: {}, error_code: {}, error_details: {})",
            self.request_id,
            self.error_code.as_utf8_or_hex(),
            self.error_details
        )
    }
}

impl fmt::Display for DeclareMiningJobErrorOwned {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "DeclareMiningJobError(request_id: {}, error_code: {}, error_details: {})",
            self.request_id,
            self.error_code.as_utf8_or_hex(),
            self.error_details
        )
    }
}

impl DeclareMiningJobError<'_> {
    /// Check that [`DeclareMiningJobError::error_code`] contains no control characters.
    ///
    /// The spec forbids control characters in error codes and in the other human-readable string
    /// codes. The `Decodable` derive runs this on every decoded
    /// [`DeclareMiningJobError`], so a value carrying one fails at parse time with
    /// [`binary_sv2::Error::ControlCharInStringCode`].
    pub fn validate(&self) -> Result<(), binary_sv2::Error> {
        if let Some(byte) = self.error_code.first_control_char() {
            return Err(binary_sv2::Error::ControlCharInStringCode(byte));
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use alloc::{vec, vec::Vec};
    use binary_sv2::GetSize;

    fn parse_declare_mining_job_error(error_code: Vec<u8>) -> Result<(), binary_sv2::Error> {
        let msg = DeclareMiningJobError {
            request_id: 0,
            error_code: Str0255::new(&error_code).unwrap(),
            error_details: B064K::new(&[]).unwrap(),
        };
        let mut bytes = vec![0u8; msg.get_size()];
        msg.clone().to_bytes(&mut bytes).unwrap();
        DeclareMiningJobError::from_bytes(&mut bytes).map(|_| ())
    }

    #[test]
    fn parses_an_error_code_without_control_chars() {
        assert!(parse_declare_mining_job_error(b"stale-chain-tip".to_vec()).is_ok());
    }

    #[test]
    fn rejects_an_error_code_with_control_chars() {
        for byte in [0x00_u8, 0x0A, 0x1F, 0x7F] {
            assert_eq!(
                parse_declare_mining_job_error(vec![b'a', byte]),
                Err(binary_sv2::Error::ControlCharInStringCode(byte)),
                "0x{byte:02X} must be rejected"
            );
        }
    }
}
