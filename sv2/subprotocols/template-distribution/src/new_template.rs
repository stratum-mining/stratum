use alloc::{fmt, vec::Vec};
use binary_sv2::{binary_codec_sv2, Deserialize, Seq0255, Serialize, B0255, B064K, U256};
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
    /// [BIP320](https://github.com/bitcoin/bips/blob/master/bip-0320.mediawiki), can be freely
    /// manipulated by the downstream node.
    ///
    /// The downstream **must not** rely on the upstream to set the
    /// [BIP320](https://github.com/bitcoin/bips/blob/master/bip-0320.mediawiki) bits to any
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
            "NewTemplate(template_id: {}, future_template: {}, version: {}, coinbase_tx_version: {}, \
             coinbase_prefix: {}, coinbase_tx_input_sequence: {}, coinbase_tx_value_remaining: {}, \
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

#[cfg(feature = "prop_test")]
use quickcheck::{Arbitrary, Gen};

#[cfg(feature = "prop_test")]
use alloc::vec;

#[cfg(feature = "prop_test")]
impl Arbitrary for NewTemplate<'static> {
    fn arbitrary(g: &mut Gen) -> NewTemplate<'static> {
        let coinbase_tx_version = (u32::arbitrary(g) % 2) + 1;
        let mut coinbase_prefix = vec::Vec::new();
        let coinbase_prefix_len = match coinbase_tx_version {
            1 => u8::arbitrary(g) as usize,
            2 => u8::arbitrary(g).checked_add(4).unwrap_or(4) as usize,
            _ => panic!(),
        };
        for _ in 0..coinbase_prefix_len {
            coinbase_prefix.push(u8::arbitrary(g))
        }
        let coinbase_prefix: binary_sv2::B0255 = coinbase_prefix.try_into().unwrap();

        // TODO uncomment when node provided outputs are supported
        //let mut coinbase_tx_outputs = vec::Vec::new();
        //let coinbase_tx_outputs_len = u16::arbitrary(g) as usize;
        //for _ in 0..coinbase_tx_outputs_len {
        //    coinbase_tx_outputs.push(u8::arbitrary(g))
        //};
        //coinbase_tx_outputs.resize(coinbase_tx_outputs.len() - coinbase_tx_outputs.len() % 36,0);
        //let coinbase_tx_outputs: binary_sv2::B064K = coinbase_tx_outputs.try_into().unwrap();

        let mut merkle_path = vec::Vec::new();
        let merkle_path_len = u8::arbitrary(g);
        for _ in 0..merkle_path_len {
            let mut path = Vec::new();
            for _ in 0..32 {
                path.push(u8::arbitrary(g));
            }
            let path: binary_sv2::U256 = path.try_into().unwrap();
            merkle_path.push(path);
        }
        let merkle_path: binary_sv2::Seq0255<binary_sv2::U256> = merkle_path.into();

        NewTemplate {
            template_id: u64::arbitrary(g) % u64::MAX,
            future_template: bool::arbitrary(g),
            version: u32::arbitrary(g),
            coinbase_tx_version,
            coinbase_prefix,
            coinbase_tx_input_sequence: u32::arbitrary(g),
            coinbase_tx_value_remaining: u64::arbitrary(g),
            // the belows should be used when node provided outputs are enabled
            //coinbase_tx_outputs_count: coinbase_tx_outputs.len().checked_div(36).unwrap_or(0) as
            // u32, coinbase_tx_outputs,
            coinbase_tx_outputs_count: 0,
            coinbase_tx_outputs: Vec::new().try_into().unwrap(),
            coinbase_tx_locktime: u32::arbitrary(g),
            merkle_path,
        }
    }
}
