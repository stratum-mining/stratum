use alloc::vec::Vec;
use binary_sv2::{Deserialize, Seq0255, Serialize, Sv2Option, Sv2OptionOwned, B064K, U256};
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
    /// As specified in [BIP323](https://github.com/bitcoin/bips/blob/master/bip-0323.mediawiki),
    /// the general purpose bits can be freely manipulated by the downstream node.
    ///
    /// The downstream node must not rely on the upstream node to set the
    /// [BIP323](https://github.com/bitcoin/bips/blob/master/bip-0323.mediawiki) bits to any
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
            self.channel_id,
            self.job_id,
            self.min_ntime,
            self.version,
            self.merkle_root
        )
    }
}

impl fmt::Display for NewMiningJobOwned {
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

impl NewMiningJobOwned {
    pub fn is_future(&self) -> bool {
        self.min_ntime.clone().into_inner().is_none()
    }

    pub fn set_future(&mut self) {
        self.min_ntime = Sv2OptionOwned::new(None);
    }

    pub fn set_no_future(&mut self, min_ntime: u32) {
        self.min_ntime = Sv2OptionOwned::new(Some(min_ntime));
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
    /// As specified in [BIP323](https://github.com/bitcoin/bips/blob/master/bip-0323.mediawiki),
    /// the general purpose bits can be freely manipulated by the downstream node.
    ///
    /// The downstream node must not rely on the upstream node to set the
    /// [BIP323](https://github.com/bitcoin/bips/blob/master/bip-0323.mediawiki) bits to any
    /// particular value.
    pub version: u32,
    /// If set to `true`, the general purpose bits of [`NewExtendedMiningJob::version`] (as
    /// specified in BIP323) can be freely manipulated by the downstream node.
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
            "NewExtendedMiningJob(channel_id: {}, job_id: {}, min_ntime: {}, version: 0x{:08x}, version_rolling_allowed: {}, merkle_path: {}, coinbase_tx_prefix: {}, coinbase_tx_suffix: {}",
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

impl fmt::Display for NewExtendedMiningJobOwned {
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

impl NewExtendedMiningJobOwned {
    pub fn is_future(&self) -> bool {
        self.min_ntime.clone().into_inner().is_none()
    }

    pub fn set_future(&mut self) {
        self.min_ntime = Sv2OptionOwned::new(None);
    }

    pub fn set_no_future(&mut self, min_ntime: u32) {
        self.min_ntime = Sv2OptionOwned::new(Some(min_ntime));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    use core::convert::TryFrom;

    fn from_arbitrary_vec_to_array(vec: Vec<u8>) -> [u8; 32] {
        let mut result = [0_u8; 32];
        let start = 32_usize.saturating_sub(vec.len());
        let copy_len = vec.len().min(32);
        result[start..start + copy_len].copy_from_slice(&vec[..copy_len]);
        result
    }

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
        let merkle_path_arrays = helpers::scan_to_u256_arrays(&merkle_path);
        let merkle_path = helpers::u256_sequence(&merkle_path_arrays);
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
        let owned_nmj = nemj.as_owned();
        owned_nmj.channel_id == nemj.channel_id
            && owned_nmj.job_id == nemj.job_id
            && owned_nmj.min_ntime == nemj.min_ntime.clone().into_owned()
            && owned_nmj.version == nemj.version
            && owned_nmj.version_rolling_allowed == nemj.version_rolling_allowed
            && owned_nmj.merkle_path == merkle_path.into_owned()
            && owned_nmj.coinbase_tx_prefix == coinbase_tx_prefix.into_owned()
            && owned_nmj.coinbase_tx_suffix == coinbase_tx_suffix.into_owned()
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
            merkle_root: U256::try_from(&merkle_root[..]).expect("U256 is exactly 32 bytes"),
        };
        let owned_nmj = nmj.clone().as_owned();
        owned_nmj.channel_id == nmj.channel_id
            && owned_nmj.job_id == nmj.job_id
            && owned_nmj.min_ntime == nmj.min_ntime.clone().into_owned()
            && owned_nmj.version == nmj.version
            && owned_nmj.merkle_root == nmj.merkle_root.into_owned()
    }

    pub mod helpers {
        use super::*;

        /// Pads `bytes` into 32-byte chunks owned by the caller, so borrowed
        /// `U256` fixtures can point into them without leaking.
        pub fn scan_to_u256_arrays(bytes: &[u8]) -> Vec<[u8; 32]> {
            bytes
                .chunks(32)
                .map(|chunk| from_arbitrary_vec_to_array(chunk.to_vec()))
                .collect()
        }

        pub fn u256_sequence(arrays: &[[u8; 32]]) -> Seq0255<U256> {
            let inner: Vec<U256> = arrays.iter().map(U256::from).collect();
            Seq0255::new(inner).expect("Could not convert bytes to SEQ0255<U256>")
        }

        pub fn bytes_to_b064k(bytes: &[u8]) -> B064K {
            B064K::try_from(bytes).expect("Failed to convert to B064K")
        }
    }
}
