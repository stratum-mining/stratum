use alloc::{fmt, vec::Vec};
use binary_sv2::{Deserialize, Seq0255, Serialize, B064K, B08, U256};
use core::convert::TryInto;

/// Message used by an upstream(Template Provider) to provide a new template for downstream to mine
/// on.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct NewTemplate<'decoder> {
    /// Upstream’s identification of the template.
    ///
    /// Should be strictly increasing.
    pub template_id: u64,
    /// If `True`, the template is intended for future [`crate::SetNewPrevHash`] message sent on
    /// the channel.
    ///
    /// If `False`, the job relates to the last sent [`crate::SetNewPrevHash`] message on the
    /// channel and the miner should start to work on the job immediately.
    pub future_template: bool,
    /// Valid header version field that reflects the current network consensus.
    ///
    /// The general purpose bits, as specified in
    /// [BIP323](https://github.com/bitcoin/bips/blob/master/bip-0323.mediawiki), can be freely
    /// manipulated by the downstream node.
    ///
    /// The downstream **must not** rely on the upstream to set the
    /// [BIP323](https://github.com/bitcoin/bips/blob/master/bip-0323.mediawiki) bits to any
    /// particular value.
    pub version: u32,
    /// The coinbase transaction `nVersion` field.
    pub coinbase_tx_version: u32,
    /// Up to 8 bytes (not including the length byte) which are to be placed at the beginning of
    /// the coinbase field in the coinbase transaction.
    pub coinbase_prefix: B08<'decoder>,
    /// The coinbase transaction input’s `nSequence` field.
    pub coinbase_tx_input_sequence: u32,
    /// The value, in satoshis, available for spending in coinbase outputs added by the downstream.
    ///
    /// Includes both transaction fees and block subsidy.
    pub coinbase_tx_value_remaining: u64,
    /// The number of transaction outputs included in [`NewTemplate::coinbase_tx_outputs`].
    pub coinbase_tx_outputs_count: u32,
    /// Bitcoin transaction outputs to be included as the last outputs in the coinbase transaction.
    ///
    /// Note that those bytes will appear as is at the end of the coinbase transaction.
    pub coinbase_tx_outputs: B064K<'decoder>,
    /// The `locktime` field in the coinbase transaction.
    pub coinbase_tx_locktime: u32,
    /// Merkle path hashes ordered from deepest.
    pub merkle_path: Seq0255<'decoder, U256<'decoder>>,
}

impl fmt::Display for NewTemplate<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "NewTemplate(template_id: {}, future_template: {}, version: 0x{:08x}, coinbase_tx_version: 0x{:08x}, \
             coinbase_prefix: {}, coinbase_tx_input_sequence: 0x{:08x}, coinbase_tx_value_remaining: {}, \
             coinbase_tx_outputs_count: {}, coinbase_tx_outputs: {}, coinbase_tx_locktime: {}, \
             merkle_path: {})",
            self.template_id,
            self.future_template,
            self.version,
            self.coinbase_tx_version,
            self.coinbase_prefix.as_hex(),
            self.coinbase_tx_input_sequence,
            self.coinbase_tx_value_remaining,
            self.coinbase_tx_outputs_count,
            self.coinbase_tx_outputs,
            self.coinbase_tx_locktime,
            self.merkle_path
        )
    }
}

impl fmt::Display for NewTemplateOwned {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "NewTemplate(template_id: {}, future_template: {}, version: 0x{:08x}, coinbase_tx_version: 0x{:08x}, \
             coinbase_prefix: {}, coinbase_tx_input_sequence: 0x{:08x}, coinbase_tx_value_remaining: {}, \
             coinbase_tx_outputs_count: {}, coinbase_tx_outputs: {}, coinbase_tx_locktime: {}, \
             merkle_path: {})",
            self.template_id,
            self.future_template,
            self.version,
            self.coinbase_tx_version,
            self.coinbase_prefix.as_hex(),
            self.coinbase_tx_input_sequence,
            self.coinbase_tx_value_remaining,
            self.coinbase_tx_outputs_count,
            self.coinbase_tx_outputs,
            self.coinbase_tx_locktime,
            self.merkle_path
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use binary_sv2::GetSize;

    /// Spec 7.3: `coinbase_prefix` is `B0_255` with the payload capped at 8 bytes (not including
    /// the length byte), so a message carrying the maximum allowed payload must roundtrip
    /// through the codec unchanged.
    #[test]
    fn max_valid_coinbase_prefix_roundtrips() {
        let prefix = vec![0xCD_u8; 8];
        let msg = NewTemplate {
            template_id: 0,
            future_template: false,
            version: 0,
            coinbase_tx_version: 2,
            coinbase_prefix: B08::new(&prefix).unwrap(),
            coinbase_tx_input_sequence: 0,
            coinbase_tx_value_remaining: 0,
            coinbase_tx_outputs_count: 0,
            coinbase_tx_outputs: B064K::new(&[]).unwrap(),
            coinbase_tx_locktime: 0,
            merkle_path: Seq0255::new(Vec::<U256>::new()).unwrap(),
        };

        let mut encoded = vec![0_u8; msg.get_size()];
        msg.clone().to_bytes(&mut encoded).unwrap();

        let decoded: NewTemplate = binary_sv2::from_bytes(&mut encoded).unwrap();

        assert_eq!(decoded.coinbase_prefix.as_bytes(), prefix.as_slice());
    }

    /// Spec 7.3: a `coinbase_prefix` payload above the 8-byte cap is invalid, so it must be
    /// rejected at construction and the corresponding wire encoding must fail to decode.
    #[test]
    fn oversized_coinbase_prefix_is_rejected() {
        let oversized = vec![0xAB_u8; 9];
        assert!(
            B08::new(&oversized).is_err(),
            "construction must reject a 9-byte coinbase_prefix"
        );

        // a `NewTemplate` wire encoding carrying a 9-byte coinbase_prefix
        let mut wire = Vec::new();
        wire.extend_from_slice(&0_u64.to_le_bytes()); // template_id
        wire.push(0); // future_template = false
        wire.extend_from_slice(&0_u32.to_le_bytes()); // version
        wire.extend_from_slice(&2_u32.to_le_bytes()); // coinbase_tx_version
        wire.push(9); // coinbase_prefix length byte: one over the spec cap
        wire.extend_from_slice(&[0xAB_u8; 9]); // coinbase_prefix payload
        wire.extend_from_slice(&0_u32.to_le_bytes()); // coinbase_tx_input_sequence
        wire.extend_from_slice(&0_u64.to_le_bytes()); // coinbase_tx_value_remaining
        wire.extend_from_slice(&0_u32.to_le_bytes()); // coinbase_tx_outputs_count
        wire.extend_from_slice(&0_u16.to_le_bytes()); // coinbase_tx_outputs length
        wire.extend_from_slice(&0_u32.to_le_bytes()); // coinbase_tx_locktime
        wire.push(0); // merkle_path length

        assert!(
            binary_sv2::from_bytes::<NewTemplate>(&mut wire).is_err(),
            "decoding must reject a NewTemplate with an oversized coinbase_prefix"
        );
    }
}
