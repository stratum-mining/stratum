#[cfg(not(feature = "with_serde"))]
use alloc::vec::Vec;
#[cfg(not(feature = "with_serde"))]
use binary_sv2::binary_codec_sv2;
use binary_sv2::{Deserialize, Seq0255, Serialize, Sv2Option, B032, B064K, U256};
#[cfg(not(feature = "with_serde"))]
use core::convert::TryInto;

/// # NewMiningJob (Server -> Client)
///
/// The server provides an updated mining job to the client through a standard channel. This MUST be the first message after the channel has been successfully opened. This first message will have min_ntime unset (future job).
/// If the `min_ntime` field is set, the client MUST start to mine on the new job immediately after receiving this message, and use the value for the initial nTime.

#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct NewMiningJob<'decoder> {
    /// Channel identifier, this must be a standard channel.
    pub channel_id: u32,
    /// Server’s identification of the mining job. This identifier must be provided
    /// to the server when shares are submitted later in the mining process.
    pub job_id: u32,
    /// Smallest nTime value available for hashing for the new mining job. An empty value indicates
    /// this is a future job to be activated once a SetNewPrevHash message is received with a
    /// matching job_id. This SetNewPrevHash message provides the new prev_hash and min_ntime.
    /// If the min_ntime value is set, this mining job is active and miner must start mining on it
    /// immediately. In this case, the new mining job uses the prev_hash from the last received SetNewPrevHash message.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub min_ntime: Sv2Option<'decoder, u32>,
    /// Valid version field that reflects the current network consensus. The
    /// general purpose bits (as specified in BIP320) can be freely manipulated
    /// by the downstream node. The downstream node MUST NOT rely on the
    /// upstream node to set the BIP320 bits to any particular value.
    pub version: u32,
    /// Merkle root field as used in the bitcoin block header.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub merkle_root: B032<'decoder>,
}

impl<'d> NewMiningJob<'d> {
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

/// NewExtendedMiningJob (Server -> Client)
///
/// (Extended and group channels only)
/// For an *extended channel*: The whole search space of the job is owned by the specified
/// channel. If the future_job field is set to *False,* the client MUST start to mine on the new job as
/// soon as possible after receiving this message.
/// For a *group channel*: This is a broadcast variant of NewMiningJob message with the
/// merkle_root field replaced by merkle_path and coinbase TX prefix and suffix, for further traffic
/// optimization. The Merkle root is then defined deterministically for each channel by the
/// common merkle_path and unique extranonce_prefix serialized into the coinbase. The full
/// coinbase is then constructed as follows: *coinbase_tx_prefix* + *extranonce_prefix* + *coinbase_tx_suffix*.
/// The proxy MAY transform this multicast variant for downstream standard channels into
/// NewMiningJob messages by computing the derived Merkle root for them. A proxy MUST
/// translate the message for all downstream channels belonging to the group which don’t signal
/// that they accept extended mining jobs in the SetupConnection message (intended and
/// expected behaviour for end mining devices).
///
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct NewExtendedMiningJob<'decoder> {
    /// For a group channel, the message is broadcasted to all standard
    /// channels belonging to the group. Otherwise, it is addressed to
    /// the specified extended channel.
    pub channel_id: u32,
    /// Server’s identification of the mining job.
    pub job_id: u32,
    /// Smallest nTime value available for hashing for the new mining job. An empty value indicates
    /// this is a future job to be activated once a SetNewPrevHash message is received with a
    /// matching job_id. This SetNewPrevHash message provides the new prev_hash and min_ntime.
    /// If the min_ntime value is set, this mining job is active and miner must start mining on it
    /// immediately. In this case, the new mining job uses the prev_hash from the last received SetNewPrevHash message.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub min_ntime: Sv2Option<'decoder, u32>,
    /// Valid version field that reflects the current network consensus.
    pub version: u32,
    /// If set to True, the general purpose bits of version (as specified in
    /// BIP320) can be freely manipulated by the downstream node.
    /// The downstream node MUST NOT rely on the upstream node to
    /// set the BIP320 bits to any particular value.
    /// If set to False, the downstream node MUST use version as it is
    /// defined by this message.
    pub version_rolling_allowed: bool,
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    /// Merkle path hashes ordered from deepest.
    pub merkle_path: Seq0255<'decoder, U256<'decoder>>,
    /// Prefix part of the coinbase transaction.
    /// The full coinbase is constructed by inserting one of the following:
    /// * For a *standard channel*: extranonce_prefix
    /// * For an *extended channel*: extranonce_prefix + extranonce (=N bytes), where N is the
    ///   negotiated extranonce space for the channel (OpenMiningChannel.Success.extranonce_size)
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub coinbase_tx_prefix: B064K<'decoder>,
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    /// Suffix part of the coinbase transaction.
    pub coinbase_tx_suffix: B064K<'decoder>,
}

impl<'d> NewExtendedMiningJob<'d> {
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

#[cfg(feature = "with_serde")]
use binary_sv2::GetSize;
#[cfg(feature = "with_serde")]
impl<'d> GetSize for NewExtendedMiningJob<'d> {
    fn get_size(&self) -> usize {
        self.channel_id.get_size()
            + self.job_id.get_size()
            + self.min_ntime.get_size()
            + self.version.get_size()
            + self.version_rolling_allowed.get_size()
            + self.merkle_path.get_size()
            + self.coinbase_tx_prefix.get_size()
            + self.coinbase_tx_suffix.get_size()
    }
}
#[cfg(feature = "with_serde")]
impl<'d> GetSize for NewMiningJob<'d> {
    fn get_size(&self) -> usize {
        self.channel_id.get_size()
            + self.job_id.get_size()
            + self.min_ntime.get_size()
            + self.version.get_size()
            + self.merkle_root.get_size()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::from_arbitrary_vec_to_array;
    use core::convert::TryFrom;
    use quickcheck_macros;

    #[quickcheck_macros::quickcheck]
    fn test_new_extended_mining_job(
        channel_id: u32,
        job_id: u32,
        min_ntime: Option<u32>,
        version: u32,
        version_rolling_allowed: bool,
        merkle_path: Vec<u8>,
        mut coinbase_tx_prefix: Vec<u8>,
        mut coinbase_tx_suffix: Vec<u8>,
    ) -> bool {
        let merkle_path = helpers::scan_to_u256_sequence(&merkle_path);
        let coinbase_tx_prefix = helpers::bytes_to_b064k(&mut coinbase_tx_prefix);
        let coinbase_tx_suffix = helpers::bytes_to_b064k(&mut coinbase_tx_suffix);
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
            merkle_root: B032::try_from(merkle_root.to_vec())
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

        pub fn scan_to_u256_sequence(bytes: &Vec<u8>) -> Seq0255<U256> {
            let inner: Vec<U256> = bytes
                .chunks(32)
                .map(|chunk| {
                    let data = from_arbitrary_vec_to_array(chunk.to_vec());
                    return U256::from(data);
                })
                .collect();
            Seq0255::new(inner).expect("Could not convert bytes to SEQ0255<U256")
        }

        pub fn bytes_to_b064k(bytes: &Vec<u8>) -> B064K {
            B064K::try_from(bytes.clone()).expect("Failed to convert to B064K")
        }
    }
}
#[cfg(feature = "with_serde")]
impl<'a> NewExtendedMiningJob<'a> {
    pub fn into_static(self) -> NewExtendedMiningJob<'static> {
        panic!("This function shouldn't be called by the Messaege Generator");
    }
    pub fn as_static(&self) -> NewExtendedMiningJob<'static> {
        panic!("This function shouldn't be called by the Messaege Generator");
    }
}
#[cfg(feature = "with_serde")]
impl<'a> NewMiningJob<'a> {
    pub fn into_static(self) -> NewMiningJob<'static> {
        panic!("This function shouldn't be called by the Messaege Generator");
    }
    pub fn as_static(&self) -> NewMiningJob<'static> {
        panic!("This function shouldn't be called by the Messaege Generator");
    }
}
