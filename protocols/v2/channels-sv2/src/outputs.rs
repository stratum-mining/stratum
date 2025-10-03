//! Utilities to deserialize outputs.
use bitcoin::{
    consensus::{deserialize, Decodable},
    transaction::TxOut,
};
use std::io::Cursor;

#[derive(Debug)]
pub struct OutputsDeserializationError;

/// Deserializes a vector of serialized outputs into a vector of `TxOut`s.
///
/// Only to be used for deserializing outputs from a `NewTemplate` message, as it asserts the
/// expected number of outputs.
///
/// Not suitable for deserializing outputs from a `SetCustomMiningJob` message or
/// `AllocateMiningJobToken.Success`.
pub fn deserialize_template_outputs(
    serialized_outputs: Vec<u8>,
    coinbase_tx_outputs_count: u32,
) -> Result<Vec<TxOut>, OutputsDeserializationError> {
    let mut cursor = Cursor::new(serialized_outputs);

    (0..coinbase_tx_outputs_count)
        .map(|_| TxOut::consensus_decode(&mut cursor).map_err(|_| OutputsDeserializationError))
        .collect()
}

/// Deserializes a vector of serialized outputs into a vector of TxOuts.
///
/// Does not assert the expected number of outputs.
pub fn deserialize_outputs(
    serialized_outputs: Vec<u8>,
) -> Result<Vec<TxOut>, OutputsDeserializationError> {
    match deserialize(serialized_outputs.as_slice()) {
        Ok(outputs) => Ok(outputs),
        Err(_) => Err(OutputsDeserializationError),
    }
}
