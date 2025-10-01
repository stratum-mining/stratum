use alloc::vec::Vec;
use binary_sv2::{binary_codec_sv2, Deserialize, Seq0255, Serialize, Sv2Option, B064K, U256};
use core::{convert::TryInto, fmt};

/// Message used by an upstream to provide an updated mining job to downstream.
///
/// This is used for Standard Channels only.
///
/// Note that Standard Jobs distrbuted through this message are restricted to a fixed Merkle Root,
/// and the only rollable bits are `version`, `nonce`, and `nTime` fields of the block header.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct NewMiningJob<'decoder> {
    /// Channel identifier for the channel that this job is valid for.
    ///
    /// This must be a Standard Channel.
    pub channel_id: u32,
    /// Upstream’s identification of the mining job.
    ///
    /// This identifier must be provided to the upstream when shares are submitted.
    pub job_id: u32,
    /// Smallest `nTime` value available for hashing for the new mining job.
    ///
    /// An empty value indicates this is a future job and will be ready to mine on once a
    /// [`SetNewPrevHash`] message is received with a matching `job_id`.
    /// [`SetNewPrevHash`] message will also provide `prev_hash` and `min_ntime`.
    ///
    /// Otherwise, if [`NewMiningJob::min_ntime`] value is set, the downstream must start mining on
    /// it immediately. In this case, the new mining job uses the `prev_hash` from the last
    /// received [`SetNewPrevHash`] message.
    ///
    /// [`SetNewPrevHash`]: crate::SetNewPrevHash
    pub min_ntime: Sv2Option<'decoder, u32>,
    /// Version field that reflects the current network consensus.
    ///
    /// As specified in [BIP320](https://github.com/bitcoin/bips/blob/master/bip-0320.mediawiki),
    /// the general purpose bits can be freely manipulated by the downstream node.
    ///
    /// The downstream node must not rely on the upstream node to set the
    /// [BIP320](https://github.com/bitcoin/bips/blob/master/bip-0320.mediawiki) bits to any
    /// particular value.
    pub version: u32,
    /// Merkle root field as used in the bitcoin block header.
    ///
    /// Note that this field is fixed and cannot be modified by the downstream node.
    pub merkle_root: U256<'decoder>,
}

impl fmt::Display for NewMiningJob<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "NewMiningJob(channel_id: {}, job_id: {}, min_ntime: {}, version: 0x{:08x}, merkle_root: {})",
            self.channel_id, self.job_id, self.min_ntime, self.version, self.merkle_root
        )
    }
}

impl NewMiningJob<'_> {
    pub fn is_future(&self) -> bool {
        self.min_ntime.clone().into_inner().is_none()
    }
    pub fn set_future(&mut self) {
        self.min_ntime = Sv2Option::new(None);
    }
    pub fn set_no_future(&mut self, min_ntime: u32) {
        self.min_ntime = Sv2Option::new(Some(min_ntime));
    }
}

/// Message used by an upstream to provide an updated mining job to the downstream through
/// Extended or Group Channel only.
///
/// An Extended Job allows rolling Merkle Roots, giving extensive control over the search space so
/// that they can implement various advanced use cases such as: translation between Stratum V1 and
/// V2 protocols, difficulty aggregation and search space splitting.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct NewExtendedMiningJob<'decoder> {
    /// Identifier of the Extended Mining Channel that this job is valid for.
    ///
    /// For a Group Channel, the message is broadcasted to all standard channels belonging to the
    /// group.
    pub channel_id: u32,
    /// Upstream’s identification of the mining job.
    ///
    /// This identifier must be provided to the upstream when shares are submitted later in the
    /// mining process.
    pub job_id: u32,
    /// Smallest `nTime` value available for hashing for the new mining job.
    ///
    /// An empty value indicates this is a future job and will be ready to mine on once a
    /// [`SetNewPrevHash`] message is received with a matching `job_id`.
    /// [`SetNewPrevHash`] message will also provide `prev_hash` and `min_ntime`.
    ///
    /// Otherwise, if [`NewMiningJob::min_ntime`] value is set, the downstream must start mining on
    /// it immediately. In this case, the new mining job uses the `prev_hash` from the last
    /// received [`SetNewPrevHash`] message.
    ///
    /// [`SetNewPrevHash`]: crate::SetNewPrevHash
    pub min_ntime: Sv2Option<'decoder, u32>,
    /// Version field that reflects the current network consensus.
    ///
    /// As specified in [BIP320](https://github.com/bitcoin/bips/blob/master/bip-0320.mediawiki),
    /// the general purpose bits can be freely manipulated by the downstream node.
    ///
    /// The downstream node must not rely on the upstream node to set the
    /// [BIP320](https://github.com/bitcoin/bips/blob/master/bip-0320.mediawiki) bits to any
    /// particular value.
    pub version: u32,
    /// If set to `true`, the general purpose bits of [`NewExtendedMiningJob::version`] (as
    /// specified in BIP320) can be freely manipulated by the downstream node.
    ///
    /// If set to `false`, the downstream node must use [`NewExtendedMiningJob::version`] as it is
    /// defined by this message.
    pub version_rolling_allowed: bool,
    /// Merkle path hashes ordered from deepest.
    pub merkle_path: Seq0255<'decoder, U256<'decoder>>,
    /// Prefix part of the coinbase transaction.
    pub coinbase_tx_prefix: B064K<'decoder>,
    /// Suffix part of the coinbase transaction.
    pub coinbase_tx_suffix: B064K<'decoder>,
}

impl fmt::Display for NewExtendedMiningJob<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "NewExtendedMiningJob(channel_id: {}, job_id: {}, min_ntime: {}, version: 0x{:08x}, version_rolling_allowed: {}, merkle_path: {}, coinbase_tx_prefix: {}, coinbase_tx_suffix: {})",
            self.channel_id,
            self.job_id,
            self.min_ntime,
            self.version,
            self.version_rolling_allowed,
            self.merkle_path,
            self.coinbase_tx_prefix,
            self.coinbase_tx_suffix
        )
    }
}

impl NewExtendedMiningJob<'_> {
    pub fn is_future(&self) -> bool {
        self.min_ntime.clone().into_inner().is_none()
    }
    pub fn set_future(&mut self) {
        self.min_ntime = Sv2Option::new(None);
    }
    pub fn set_no_future(&mut self, min_ntime: u32) {
        self.min_ntime = Sv2Option::new(Some(min_ntime));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::from_arbitrary_vec_to_array;
    use core::convert::TryFrom;

    #[quickcheck_macros::quickcheck]
    #[allow(clippy::too_many_arguments)]
    fn test_new_extended_mining_job(
        channel_id: u32,
        job_id: u32,
        min_ntime: Option<u32>,
        version: u32,
        version_rolling_allowed: bool,
        merkle_path: Vec<u8>,
        coinbase_tx_prefix: Vec<u8>,
        coinbase_tx_suffix: Vec<u8>,
    ) -> bool {
        let merkle_path = helpers::scan_to_u256_sequence(&merkle_path);
        let coinbase_tx_prefix = helpers::bytes_to_b064k(&coinbase_tx_prefix);
        let coinbase_tx_suffix = helpers::bytes_to_b064k(&coinbase_tx_suffix);
        let nemj = NewExtendedMiningJob {
            channel_id,
            job_id,
            min_ntime: Sv2Option::new(min_ntime),
            version,
            version_rolling_allowed,
            merkle_path: merkle_path.clone(),
            coinbase_tx_prefix: coinbase_tx_prefix.clone(),
            coinbase_tx_suffix: coinbase_tx_suffix.clone(),
        };
        let static_nmj = nemj.as_static();
        static_nmj.channel_id == nemj.channel_id
            && static_nmj.job_id == nemj.job_id
            && static_nmj.min_ntime == nemj.min_ntime
            && static_nmj.version == nemj.version
            && static_nmj.version_rolling_allowed == nemj.version_rolling_allowed
            && static_nmj.merkle_path == merkle_path
            && static_nmj.coinbase_tx_prefix == coinbase_tx_prefix
            && static_nmj.coinbase_tx_suffix == coinbase_tx_suffix
    }

    #[quickcheck_macros::quickcheck]
    fn test_new_mining_job(
        channel_id: u32,
        job_id: u32,
        min_ntime: Option<u32>,
        version: u32,
        merkle_root: Vec<u8>,
    ) -> bool {
        let merkle_root = from_arbitrary_vec_to_array(merkle_root);
        let nmj = NewMiningJob {
            channel_id,
            job_id,
            min_ntime: Sv2Option::new(min_ntime),
            version,
            merkle_root: U256::try_from(merkle_root.to_vec())
                .expect("NewMiningJob: failed to convert merkle_root to B032"),
        };
        let static_nmj = nmj.clone().as_static();
        static_nmj.channel_id == nmj.channel_id
            && static_nmj.job_id == nmj.job_id
            && static_nmj.min_ntime == nmj.min_ntime
            && static_nmj.version == nmj.version
            && static_nmj.merkle_root == nmj.merkle_root
    }

    pub mod helpers {
        use super::*;
        use alloc::borrow::ToOwned;

        pub fn scan_to_u256_sequence(bytes: &[u8]) -> Seq0255<U256> {
            let inner: Vec<U256> = bytes
                .chunks(32)
                .map(|chunk| {
                    let data = from_arbitrary_vec_to_array(chunk.to_vec());
                    U256::from(data)
                })
                .collect();
            Seq0255::new(inner).expect("Could not convert bytes to SEQ0255<U256")
        }

        pub fn bytes_to_b064k(bytes: &[u8]) -> B064K {
            B064K::try_from(bytes.to_owned()).expect("Failed to convert to B064K")
        }
    }
}
