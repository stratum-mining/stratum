use crate::COINBASE_PREFIX_MAX_LEN;
use alloc::{fmt, vec::Vec};
use binary_sv2::{Deserialize, Seq0255, Serialize, B0255, B064K, U256};
use core::convert::TryInto;

/// Message used by an upstream(Template Provider) to provide a new template for downstream to mine
/// on.
///
/// Decoding fails if [`NewTemplate::coinbase_prefix`] violates the constraints the spec defines
/// for it, see [`NewTemplate::validate`].
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[validate]
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
    pub coinbase_prefix: B0255<'decoder>,
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

impl NewTemplate<'_> {
    /// Check that [`NewTemplate::coinbase_prefix`] satisfies the constraints the spec defines
    /// for it.
    ///
    /// The prefix must be at most [`COINBASE_PREFIX_MAX_LEN`] bytes long, excluding the `B0_255`
    /// length prefix, and must begin with the block height push described by BIP34: a length
    /// byte followed by that many little-endian bytes of the height. The `Decodable` derive calls
    /// this on every decoded [`NewTemplate`], so a prefix violating either constraint fails at
    /// parse time instead of reaching application code.
    ///
    /// The check is structural: it verifies that the length byte of the height push fits in the
    /// prefix. The number the push encodes is not inspected, so the minimal-encoding rules the
    /// height push is subject to under Bitcoin consensus are not applied here.
    pub fn validate(&self) -> Result<(), binary_sv2::Error> {
        let prefix = self.coinbase_prefix.as_bytes();
        if prefix.len() > COINBASE_PREFIX_MAX_LEN {
            return Err(binary_sv2::Error::CoinbasePrefixTooLong(prefix.len()));
        }
        match prefix.split_first() {
            // BIP34 height push: a length byte followed by that many bytes of the height. A zero
            // length byte encodes height 0 as an empty push.
            Some((height_len, height)) if (*height_len as usize) <= height.len() => Ok(()),
            _ => Err(binary_sv2::Error::InvalidCoinbasePrefixHeightPush),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use alloc::{vec, vec::Vec};
    use binary_sv2::GetSize;

    fn new_template_with_prefix(prefix: &[u8]) -> NewTemplate<'_> {
        let empty_merkle_path: Vec<U256<'_>> = vec![];
        let merkle_path: Seq0255<'_, U256<'_>> = Seq0255::new(empty_merkle_path).unwrap();

        NewTemplate {
            template_id: 0,
            future_template: false,
            version: 0,
            coinbase_tx_version: 2,
            coinbase_prefix: B0255::new(prefix).unwrap(),
            coinbase_tx_input_sequence: 0,
            coinbase_tx_value_remaining: 0,
            coinbase_tx_outputs_count: 0,
            coinbase_tx_outputs: B064K::new(&[]).unwrap(),
            coinbase_tx_locktime: 0,
            merkle_path,
        }
    }

    /// Encodes a template with the given prefix and decodes it back, mapping a successful parse
    /// to `Ok(())` so the borrowed message does not outlive the buffer it reads from.
    fn parse_with_prefix(prefix: Vec<u8>) -> Result<(), binary_sv2::Error> {
        let template = new_template_with_prefix(&prefix);
        let mut bytes = vec![0u8; template.get_size()];
        template.clone().to_bytes(&mut bytes).unwrap();
        NewTemplate::from_bytes(&mut bytes).map(|_| ())
    }

    #[test]
    fn parses_a_bip34_height_push() {
        // height 159: a 2-byte push followed by 0x9F, 0x00
        assert!(parse_with_prefix(vec![2, 159, 0, 0]).is_ok());
        assert!(new_template_with_prefix(&[2, 159, 0, 0]).validate().is_ok());
    }

    #[test]
    fn parses_the_maximum_length_prefix() {
        assert!(parse_with_prefix(vec![7, 1, 2, 3, 4, 5, 6, 7]).is_ok());
    }

    #[test]
    fn rejects_a_prefix_longer_than_the_spec_allows() {
        let result = parse_with_prefix(vec![8, 1, 2, 3, 4, 5, 6, 7, 8]);

        assert_eq!(
            result,
            Err(binary_sv2::Error::CoinbasePrefixTooLong(9)),
            "a 9-byte coinbase_prefix must be rejected"
        );
    }

    #[test]
    fn rejects_a_prefix_without_a_height_push() {
        let result = parse_with_prefix(vec![]);

        assert_eq!(
            result,
            Err(binary_sv2::Error::InvalidCoinbasePrefixHeightPush),
            "an empty coinbase_prefix must be rejected"
        );
    }

    #[test]
    fn rejects_a_height_push_that_does_not_fit_the_prefix() {
        // the push claims 82 bytes of height but the prefix only carries one more byte
        let result = parse_with_prefix(vec![82, 0]);

        assert_eq!(
            result,
            Err(binary_sv2::Error::InvalidCoinbasePrefixHeightPush),
            "a height push longer than the prefix must be rejected"
        );
    }
}
