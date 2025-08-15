//! Utility to deserialize outputs from a `NewTemplate` message.
use bitcoin::{consensus::Decodable, transaction::TxOut};
use std::io::Cursor;

/// Deserializes a vector of serialized outputs into a vector of TxOuts.
///
/// Only to be used for deserializing outputs from a NewTemplate message.
///
/// Not suitable for deserializing outputs from a SetCustomMiningJob message or
/// AllocateMiningJobToken.Success.
pub fn deserialize_template_outputs(
    serialized_outputs: Vec<u8>,
    coinbase_tx_outputs_count: u32,
) -> Result<Vec<TxOut>, TemplateOutputsDeserializationError> {
    let mut cursor = Cursor::new(serialized_outputs);

    (0..coinbase_tx_outputs_count)
        .map(|_| {
            TxOut::consensus_decode(&mut cursor).map_err(|_| TemplateOutputsDeserializationError)
        })
        .collect()
}

pub struct TemplateOutputsDeserializationError;
