//! # Sv2 Extended Channel - Mining Client Abstraction
//!
//! This module provides an abstraction over the state of an [Sv2](https://stratumprotocol.org/specification)
//! **Extended Channel** within a mining client.

extern crate alloc;
use super::HashMap;
use crate::{
    bip141::try_strip_bip141,
    chain_tip::ChainTip,
    client::{
        error::ExtendedChannelError,
        share_accounting::{ShareAccounting, ShareValidationError, ShareValidationResult},
    },
    merkle_root::merkle_root_from_path,
    target::{bytes_to_hex, target_to_difficulty, u256_to_block_hash},
};
use alloc::{format, string::String, vec, vec::Vec};
use binary_sv2::{self, Sv2Option};
use bitcoin::{
    absolute::LockTime,
    blockdata::block::{Header, Version as BlockVersion},
    consensus::{serialize, Decodable},
    hashes::sha256d::Hash,
    transaction::Version,
    CompactTarget, OutPoint, Sequence, Target as BitcoinTarget, Transaction, TxIn, TxOut, Witness,
};
use mining_sv2::{
    NewExtendedMiningJob, SetCustomMiningJob, SetCustomMiningJobSuccess,
    SetNewPrevHash as SetNewPrevHashMp, SubmitSharesExtended, Target, MAX_EXTRANONCE_LEN,
};
use tracing::debug;

/// A type alias representing an extended mining job tied to a specific `extranonce_prefix`.
///
/// Extended jobs allow Merkle root rolling, providing broader control over the search space.
/// Each job includes:
/// - A [`NewExtendedMiningJob`] message
/// - The `extranonce_prefix` in use when the job was created
pub type ExtendedJob<'a> = (NewExtendedMiningJob<'a>, Vec<u8>);

/// Mining Client abstraction for the state management of an Sv2 Extended Channel.
///
/// This struct encapsulates all channel-specific state for a mining client, including:
/// - The channel's unique `channel_id`.
/// - The channel's `user_identity` as seen by upstream.
/// - The channel's unique `extranonce_prefix`.
/// - The size of the rollable portion of the extranonce.
/// - The channel's current target.
/// - The channel's nominal hashrate.
/// - Whether version rolling is supported (see [BIP 320](https://github.com/bitcoin/bips/blob/master/bip-0320.mediawiki)).
/// - Future jobs (indexed by `job_id`) to be activated by a [`SetNewPrevHash`](SetNewPrevHashMp)
///   message.
/// - The currently active job.
/// - Past jobs (previously active under the current chain tip, indexed by `job_id`).
/// - Stale jobs (previously active and past jobs under the previous chain tip, indexed by
///   `job_id`).
/// - Share accounting for the channel (as tracked by the client).
/// - The channel's current chain tip.
#[derive(Clone, Debug)]
pub struct ExtendedChannel<'a> {
    channel_id: u32,
    user_identity: String,
    extranonce_prefix: Vec<u8>,
    rollable_extranonce_size: u16,
    target: Target, // todo: try to use Target from rust-bitcoin
    nominal_hashrate: f32,
    version_rolling: bool,
    // future jobs are indexed with job_id (u32)
    future_jobs: HashMap<u32, ExtendedJob<'a>>,
    active_job: Option<ExtendedJob<'a>>,
    // past jobs are indexed with job_id (u32)
    past_jobs: HashMap<u32, ExtendedJob<'a>>,
    // stale jobs are indexed with job_id (u32)
    stale_jobs: HashMap<u32, ExtendedJob<'a>>,
    share_accounting: ShareAccounting,
    chain_tip: Option<ChainTip>,
}

impl<'a> ExtendedChannel<'a> {
    /// Constructs a new [`ExtendedChannel`].
    pub fn new(
        channel_id: u32,
        user_identity: String,
        extranonce_prefix: Vec<u8>,
        target: Target,
        nominal_hashrate: f32,
        version_rolling: bool,
        rollable_extranonce_size: u16,
    ) -> Self {
        Self {
            channel_id,
            user_identity,
            extranonce_prefix,
            rollable_extranonce_size,
            target,
            nominal_hashrate,
            version_rolling,
            future_jobs: HashMap::new(),
            active_job: None,
            past_jobs: HashMap::new(),
            stale_jobs: HashMap::new(),
            share_accounting: ShareAccounting::new(),
            chain_tip: None,
        }
    }

    /// Returns the unique `channel_id` of this channel.
    pub fn get_channel_id(&self) -> u32 {
        self.channel_id
    }

    /// Returns the `user_identity` used by the upstream node to identify this client.
    pub fn get_user_identity(&self) -> &String {
        &self.user_identity
    }

    /// Returns the bytes representing the first part of the `extranonce`.
    pub fn get_extranonce_prefix(&self) -> &Vec<u8> {
        &self.extranonce_prefix
    }

    /// Returns `true` if the channel supports version rolling as per [BIP 320](https://github.com/bitcoin/bips/blob/master/bip-0320.mediawiki).
    pub fn is_version_rolling(&self) -> bool {
        self.version_rolling
    }

    /// Returns a reference to the current [`ChainTip`], if any.
    pub fn get_chain_tip(&self) -> Option<&ChainTip> {
        self.chain_tip.as_ref()
    }

    /// Sets the [`ChainTip`].
    pub fn set_chain_tip(&mut self, chain_tip: ChainTip) {
        self.chain_tip = Some(chain_tip);
    }

    /// Sets a new extranonce prefix for the channel.
    ///
    /// After this change, all new jobs will use the new extranonce prefix.
    /// Jobs created before this call retain the previous extranonce prefix,
    /// and share validation is performed accordingly.
    ///
    /// Returns an error if the new prefix violates the minimum rollable extranonce size established
    /// at channel creation.
    pub fn set_extranonce_prefix(
        &mut self,
        new_extranonce_prefix: Vec<u8>,
    ) -> Result<(), ExtendedChannelError> {
        let new_rollable_extranonce_size =
            MAX_EXTRANONCE_LEN as u16 - new_extranonce_prefix.len() as u16;

        // we return an error if the new extranonce_prefix would violate
        // min_rollable_extranonce_size that was already established with the client when the
        // channel was created
        if new_rollable_extranonce_size < self.rollable_extranonce_size {
            return Err(ExtendedChannelError::NewExtranoncePrefixTooLarge);
        }

        self.extranonce_prefix = new_extranonce_prefix;
        self.rollable_extranonce_size = new_rollable_extranonce_size;

        Ok(())
    }

    /// Returns the available size, in bytes, of the rollable portion of the extranonce.
    pub fn get_rollable_extranonce_size(&self) -> u16 {
        self.rollable_extranonce_size
    }

    /// Returns a reference to the current [`Target`] for this channel.
    pub fn get_target(&self) -> &Target {
        &self.target
    }

    /// Sets a new [`Target`] for the channel.
    pub fn set_target(&mut self, new_target: Target) {
        self.target = new_target;
    }

    /// Returns the cumulative nominal hashrate for the channel, in h/s.
    pub fn get_nominal_hashrate(&self) -> f32 {
        self.nominal_hashrate
    }

    /// Returns a reference to the currently active job, if any.
    pub fn get_active_job(&self) -> Option<&ExtendedJob<'a>> {
        self.active_job.as_ref()
    }

    /// Returns a reference to all future jobs for this channel.
    pub fn get_future_jobs(&self) -> &HashMap<u32, ExtendedJob<'a>> {
        &self.future_jobs
    }

    /// Returns a reference to all past jobs for this channel.
    pub fn get_past_jobs(&self) -> &HashMap<u32, ExtendedJob<'a>> {
        &self.past_jobs
    }

    /// Returns a reference to all stale jobs for this channel.
    pub fn get_stale_jobs(&self) -> &HashMap<u32, ExtendedJob<'a>> {
        &self.stale_jobs
    }

    /// Returns a reference to the [`ShareAccounting`] for this channel.
    pub fn get_share_accounting(&self) -> &ShareAccounting {
        &self.share_accounting
    }

    /// Handles a [`NewExtendedMiningJob`] message received from upstream.
    ///
    /// - If [`NewExtendedMiningJob::min_ntime`] is empty, the job is considered a future job and
    ///   added to the future jobs list (see [`get_future_jobs`](ExtendedChannel::get_future_jobs)).
    /// - Otherwise, the job is activated and previous active job moves to the past jobs list.
    pub fn on_new_extended_mining_job(
        &mut self,
        mut new_extended_mining_job: NewExtendedMiningJob<'a>,
    ) -> Result<(), ExtendedChannelError> {
        // try to strip bip141 bytes from coinbase_tx_prefix and coinbase_tx_suffix, if they are
        // present
        let new_extended_mining_job = match try_strip_bip141(
            new_extended_mining_job.coinbase_tx_prefix.inner_as_ref(),
            new_extended_mining_job.coinbase_tx_suffix.inner_as_ref(),
        )
        .map_err(ExtendedChannelError::FailedToTryToStripBip141)?
        {
            Some((coinbase_tx_prefix_stripped_bip141, coinbase_tx_suffix_stripped_bip141)) => {
                new_extended_mining_job.coinbase_tx_prefix = coinbase_tx_prefix_stripped_bip141
                    .try_into()
                    .map_err(|_| ExtendedChannelError::FailedToSerializeToB064K)?;
                new_extended_mining_job.coinbase_tx_suffix = coinbase_tx_suffix_stripped_bip141
                    .try_into()
                    .map_err(|_| ExtendedChannelError::FailedToSerializeToB064K)?;
                new_extended_mining_job
            }
            None => new_extended_mining_job,
        };

        match new_extended_mining_job.min_ntime.clone().into_inner() {
            Some(_min_ntime) => {
                if let Some(active_job) = self.active_job.clone() {
                    self.past_jobs.insert(active_job.0.job_id, active_job);
                }
                self.active_job = Some((new_extended_mining_job, self.extranonce_prefix.clone()));
            }
            None => {
                self.future_jobs.insert(
                    new_extended_mining_job.job_id,
                    (new_extended_mining_job, self.extranonce_prefix.clone()),
                );
            }
        }

        Ok(())
    }

    /// Handles a `SetCustomMiningJobSuccess` message from upstream.
    /// Requires the corresponding `SetCustomMiningJob`.
    ///
    /// To be used by a Sv2 Job Declarator Client
    pub fn on_set_custom_mining_job_success(
        &mut self,
        set_custom_mining_job: SetCustomMiningJob<'a>,
        set_custom_mining_job_success: SetCustomMiningJobSuccess,
    ) -> Result<(), ExtendedChannelError> {
        if set_custom_mining_job.channel_id != set_custom_mining_job_success.channel_id
            || set_custom_mining_job.channel_id != self.channel_id
        {
            return Err(ExtendedChannelError::ChannelIdMismatch);
        }

        if set_custom_mining_job.request_id != set_custom_mining_job_success.request_id {
            return Err(ExtendedChannelError::RequestIdMismatch);
        }

        let Some(chain_tip) = self.chain_tip.clone() else {
            return Err(ExtendedChannelError::NoChainTip);
        };

        if set_custom_mining_job.min_ntime != chain_tip.min_ntime()
            || set_custom_mining_job.prev_hash != chain_tip.prev_hash()
            || set_custom_mining_job.nbits != chain_tip.nbits()
        {
            return Err(ExtendedChannelError::ChainTipMismatch);
        }

        let deserialized_outputs = Vec::<TxOut>::consensus_decode(
            &mut set_custom_mining_job
                .coinbase_tx_outputs
                .inner_as_ref()
                .to_vec()
                .as_slice(),
        )
        .map_err(|_| ExtendedChannelError::FailedToDeserializeCoinbaseOutputs)?;

        let mut script_sig = vec![];
        script_sig.extend_from_slice(set_custom_mining_job.coinbase_prefix.inner_as_ref());
        script_sig.extend_from_slice(&[0; MAX_EXTRANONCE_LEN]);

        let tx_in = TxIn {
            previous_output: OutPoint::null(),
            script_sig: script_sig.into(),
            sequence: Sequence(set_custom_mining_job.coinbase_tx_input_n_sequence),
            witness: Witness::from(vec![vec![0; 32]]), /* note: 32 bytes of zeros is only safe to
                                                        * assume now, this could change in future
                                                        * soft forks */
        };

        let coinbase = Transaction {
            version: Version::non_standard(set_custom_mining_job.coinbase_tx_version as i32),
            lock_time: LockTime::from_consensus(set_custom_mining_job.coinbase_tx_locktime),
            input: vec![tx_in],
            output: deserialized_outputs,
        };

        let serialized_coinbase = serialize(&coinbase);

        let prefix_index = 4 // tx version
            + 2 // segwit
            + 1 // number of inputs
            + 32 // prev OutPoint
            + 4 // index
            + 1 // bytes in script
            + set_custom_mining_job.coinbase_prefix.inner_as_ref().len();

        let coinbase_tx_prefix = serialized_coinbase[0..prefix_index].to_vec();

        let suffix_index = prefix_index + MAX_EXTRANONCE_LEN;

        let coinbase_tx_suffix = serialized_coinbase[suffix_index..].to_vec();

        // strip bip141 bytes from coinbase_tx_prefix and coinbase_tx_suffix
        let (coinbase_tx_prefix_stripped_bip141, coinbase_tx_suffix_stripped_bip141) =
            try_strip_bip141(&coinbase_tx_prefix, &coinbase_tx_suffix)
                .map_err(ExtendedChannelError::FailedToTryToStripBip141)?
                .ok_or(ExtendedChannelError::FailedToStripBip141)?;

        let new_extended_mining_job = NewExtendedMiningJob {
            channel_id: set_custom_mining_job.channel_id,
            job_id: set_custom_mining_job_success.job_id,
            min_ntime: Sv2Option::new(Some(set_custom_mining_job.min_ntime)),
            version: set_custom_mining_job.version,
            version_rolling_allowed: self.version_rolling,
            coinbase_tx_prefix: coinbase_tx_prefix_stripped_bip141
                .try_into()
                .map_err(|_| ExtendedChannelError::FailedToSerializeToB064K)?,
            coinbase_tx_suffix: coinbase_tx_suffix_stripped_bip141
                .try_into()
                .map_err(|_| ExtendedChannelError::FailedToSerializeToB064K)?,
            merkle_path: set_custom_mining_job.merkle_path,
        };

        if let Some(active_job) = self.active_job.clone() {
            self.past_jobs.insert(active_job.0.job_id, active_job);
        }
        self.active_job = Some((new_extended_mining_job, self.extranonce_prefix.clone()));

        Ok(())
    }

    /// Handles a [`ChainTip`] update.
    ///
    /// To be used by a Sv2 Job Declarator Client, which should never receive a
    /// [`SetNewPrevHash`](SetNewPrevHashMp) (Mining Protocol) message, or will most likely
    /// ignore it if it does.
    ///
    /// So a [`SetNewPrevHash`](template_distribution_sv2::SetNewPrevHash) (Template Distribution
    /// Protocol) message should be converted into a [`ChainTip`] and passed to this function.
    pub fn on_chain_tip_update(&mut self, chain_tip: ChainTip) -> Result<(), ExtendedChannelError> {
        self.chain_tip = Some(chain_tip);

        // all other future jobs are now useless
        self.future_jobs.clear();

        // mark all past jobs as stale, so that shares are not propagated
        self.stale_jobs = self.past_jobs.clone();

        // clear past jobs, as we're no longer going to propagate shares for them
        self.past_jobs.clear();

        // clear seen shares, as shares for past chain tip will be rejected as stale
        self.share_accounting.flush_seen_shares();

        Ok(())
    }

    /// Handles a [`SetNewPrevHash`](SetNewPrevHashMp) message from upstream.
    ///
    /// - If the referenced `job_id` is not a future job, returns an error.
    /// - If it is a future job, activates it as the current job.
    /// - Marks all past jobs as stale and clears them.
    /// - Clears all seen shares as shares for the previous chain tip will be rejected as stale.
    /// - Updates the chain tip for the channel.
    pub fn on_set_new_prev_hash(
        &mut self,
        set_new_prev_hash: SetNewPrevHashMp<'a>,
    ) -> Result<(), ExtendedChannelError> {
        match self.future_jobs.remove(&set_new_prev_hash.job_id) {
            Some(mut activated_job) => {
                activated_job.0.min_ntime = Sv2Option::new(Some(set_new_prev_hash.min_ntime));
                self.active_job = Some(activated_job);
            }
            None => {
                return Err(ExtendedChannelError::JobIdNotFound);
            }
        }

        // all other future jobs are now useless
        self.future_jobs.clear();

        // mark all past jobs as stale, so that shares are not propagated
        self.stale_jobs = self.past_jobs.clone();

        // clear past jobs, as we're no longer going to propagate shares for them
        self.past_jobs.clear();

        // clear seen shares, as shares for past chain tip will be rejected as stale
        self.share_accounting.flush_seen_shares();

        self.chain_tip = Some(set_new_prev_hash.into());

        Ok(())
    }

    /// Validates a share prior to submission upstream.
    ///
    /// Updates channel state with the share validation result:
    /// - Prevents propagation of stale, duplicate, or low-difficulty shares.
    /// - Indicates whether a block was found from the share.
    /// - Maintains local share accounting for later reconciliation with upstream acknowledgements.
    pub fn validate_share(
        &mut self,
        share: SubmitSharesExtended,
    ) -> Result<ShareValidationResult, ShareValidationError> {
        let job_id = share.job_id;

        // check if job_id is active job
        let is_active_job = self
            .active_job
            .as_ref()
            .is_some_and(|job| job.0.job_id == job_id);

        // check if job_id is past job
        let is_past_job = self.past_jobs.contains_key(&job_id);

        // check if job_id is stale job
        let is_stale_job = self.stale_jobs.contains_key(&job_id);

        if is_stale_job {
            return Err(ShareValidationError::Stale);
        }

        let job = if is_active_job {
            self.active_job.as_ref().expect("active job must exist")
        } else if is_past_job {
            self.past_jobs.get(&job_id).expect("past job must exist")
        } else {
            return Err(ShareValidationError::InvalidJobId);
        };

        let mut full_extranonce = vec![];
        full_extranonce.extend_from_slice(job.1.as_slice());
        full_extranonce.extend_from_slice(share.extranonce.inner_as_ref());

        // calculate the merkle root from:
        // - job coinbase_tx_prefix
        // - full extranonce
        // - job coinbase_tx_suffix
        // - job merkle_path
        let merkle_root: [u8; 32] = merkle_root_from_path(
            job.0.coinbase_tx_prefix.inner_as_ref(),
            job.0.coinbase_tx_suffix.inner_as_ref(),
            full_extranonce.as_ref(),
            &job.0.merkle_path.inner_as_ref(),
        )
        .ok_or(ShareValidationError::Invalid)?
        .try_into()
        .expect("merkle root must be 32 bytes");

        let chain_tip = self
            .chain_tip
            .as_ref()
            .ok_or(ShareValidationError::NoChainTip)?;

        let prev_hash = chain_tip.prev_hash();
        let nbits: CompactTarget = CompactTarget::from_consensus(chain_tip.nbits());

        // validate when version rolling is not allowed
        if !job.0.version_rolling_allowed {
            // If version rolling is not allowed, ensure bits 13-28 are 0
            // This is done by checking if the version & 0x1fffe000 == 0
            // ref: https://github.com/bitcoin/bips/blob/master/bip-0320.mediawiki
            if (share.version & 0x1fffe000) != 0 {
                return Err(ShareValidationError::VersionRollingNotAllowed);
            }
        }

        // create the header for validation
        let header = Header {
            version: BlockVersion::from_consensus(share.version as i32),
            prev_blockhash: u256_to_block_hash(prev_hash.clone()),
            merkle_root: (*Hash::from_bytes_ref(&merkle_root)).into(),
            time: share.ntime,
            bits: nbits,
            nonce: share.nonce,
        };

        // convert the header hash to a target type for easy comparison
        let hash = header.block_hash();
        let raw_hash: [u8; 32] = *hash.to_raw_hash().as_ref();
        let hash_as_target: Target = raw_hash.into();
        let hash_as_diff = target_to_difficulty(hash_as_target.clone());

        let network_target = BitcoinTarget::from_compact(nbits);

        // print hash_as_target and self.target as human readable hex
        let hash_as_u256: binary_sv2::U256 = hash_as_target.clone().into();
        let mut hash_bytes = hash_as_u256.to_vec();
        hash_bytes.reverse(); // Convert to big-endian for display
        let target_u256: binary_sv2::U256 = self.target.clone().into();
        let mut target_bytes = target_u256.to_vec();
        target_bytes.reverse(); // Convert to big-endian for display

        debug!(
            "share validation \nshare:\t\t{}\nchannel target:\t{}\nnetwork target:\t{}",
            bytes_to_hex(&hash_bytes),
            bytes_to_hex(&target_bytes),
            format!("{:x}", network_target)
        );

        // check if a block was found
        if network_target.is_met_by(hash) {
            self.share_accounting.update_share_accounting(
                target_to_difficulty(self.target.clone()) as u64,
                share.sequence_number,
                hash.to_raw_hash(),
            );
            return Ok(ShareValidationResult::BlockFound);
        }

        // check if the share hash meets the channel target
        if hash_as_target < self.target {
            if self.share_accounting.is_share_seen(hash.to_raw_hash()) {
                return Err(ShareValidationError::DuplicateShare);
            }

            self.share_accounting.update_share_accounting(
                target_to_difficulty(self.target.clone()) as u64,
                share.sequence_number,
                hash.to_raw_hash(),
            );

            // update the best diff
            self.share_accounting.update_best_diff(hash_as_diff);

            return Ok(ShareValidationResult::Valid);
        }

        Err(ShareValidationError::DoesNotMeetTarget)
    }
}

#[cfg(test)]
mod tests {
    use crate::client::{
        extended::ExtendedChannel,
        share_accounting::{ShareValidationError, ShareValidationResult},
    };
    use binary_sv2::Sv2Option;
    use mining_sv2::{
        NewExtendedMiningJob, SetNewPrevHash as SetNewPrevHashMp, SubmitSharesExtended,
        MAX_EXTRANONCE_LEN,
    };
    use std::convert::TryInto;

    #[test]
    fn test_future_job_activation_flow() {
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let target = [0xff; 32].into();
        let nominal_hashrate = 1.0;
        let version_rolling = true;
        let rollable_extranonce_size = (MAX_EXTRANONCE_LEN - extranonce_prefix.len()) as u16;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            extranonce_prefix.clone(),
            target,
            nominal_hashrate,
            version_rolling,
            rollable_extranonce_size,
        );

        let future_job = NewExtendedMiningJob {
            channel_id: 1,
            job_id: 1,
            min_ntime: Sv2Option::new(None),
            version: 536870912,
            version_rolling_allowed: true,
            coinbase_tx_prefix: vec![
                2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 34, 82, 0,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_suffix: vec![
                255, 255, 255, 255, 2, 0, 242, 5, 42, 1, 0, 0, 0, 22, 0, 20, 235, 225, 183, 220,
                194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194, 8, 252, 0, 0, 0,
                0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209, 222,
                253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180, 139,
                235, 216, 54, 151, 78, 140, 249, 0, 0, 0, 0,
            ]
            .try_into()
            .unwrap(),
            merkle_path: vec![].try_into().unwrap(),
        };

        channel
            .on_new_extended_mining_job(future_job.clone())
            .unwrap();

        assert_eq!(channel.get_future_jobs().len(), 1);
        assert_eq!(channel.get_active_job(), None);
        assert_eq!(channel.get_past_jobs().len(), 0);

        let ntime: u32 = 1746839905;
        let set_new_prev_hash = SetNewPrevHashMp {
            channel_id,
            job_id: future_job.job_id,
            prev_hash: [
                200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144,
                205, 88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
            ]
            .into(),
            nbits: 503543726,
            min_ntime: ntime,
        };

        channel.on_set_new_prev_hash(set_new_prev_hash).unwrap();

        assert!(channel.get_future_jobs().is_empty());

        let mut previously_future_job = future_job.clone();
        previously_future_job.min_ntime = Sv2Option::new(Some(ntime));

        assert_eq!(
            channel.get_active_job(),
            Some(&(previously_future_job, extranonce_prefix))
        );
    }

    #[test]
    fn test_past_jobs_flow() {
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let target = [0xff; 32].into();
        let nominal_hashrate = 1.0;
        let version_rolling = true;
        let rollable_extranonce_size = (MAX_EXTRANONCE_LEN - extranonce_prefix.len()) as u16;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            extranonce_prefix.clone(),
            target,
            nominal_hashrate,
            version_rolling,
            rollable_extranonce_size,
        );

        let ntime: u32 = 1746839905;
        let active_job = NewExtendedMiningJob {
            channel_id: 1,
            job_id: 1,
            min_ntime: Sv2Option::new(Some(ntime)),
            version: 536870912,
            version_rolling_allowed: true,
            coinbase_tx_prefix: vec![
                2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 34, 82, 0,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_suffix: vec![
                255, 255, 255, 255, 2, 0, 242, 5, 42, 1, 0, 0, 0, 22, 0, 20, 235, 225, 183, 220,
                194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194, 8, 252, 0, 0, 0,
                0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209, 222,
                253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180, 139,
                235, 216, 54, 151, 78, 140, 249, 0, 0, 0, 0,
            ]
            .try_into()
            .unwrap(),
            merkle_path: vec![].try_into().unwrap(),
        };

        channel
            .on_new_extended_mining_job(active_job.clone())
            .unwrap();

        assert_eq!(channel.get_future_jobs().len(), 0);
        assert_eq!(
            channel.get_active_job(),
            Some(&(active_job.clone(), extranonce_prefix.clone()))
        );
        assert_eq!(channel.get_past_jobs().len(), 0);

        let mut new_active_job = active_job.clone();
        new_active_job.job_id = 2;
        channel
            .on_new_extended_mining_job(new_active_job.clone())
            .unwrap();

        assert_eq!(channel.get_future_jobs().len(), 0);
        assert_eq!(
            channel.get_active_job(),
            Some(&(new_active_job, extranonce_prefix))
        );
        assert_eq!(channel.get_past_jobs().len(), 1);
    }

    #[test]
    fn test_share_validation_block_found() {
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let target = [0xff; 32].into();
        let nominal_hashrate = 1.0;
        let version_rolling = true;
        let rollable_extranonce_size = (MAX_EXTRANONCE_LEN - extranonce_prefix.len()) as u16;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            extranonce_prefix.clone(),
            target,
            nominal_hashrate,
            version_rolling,
            rollable_extranonce_size,
        );

        let future_job = NewExtendedMiningJob {
            channel_id: 1,
            job_id: 1,
            min_ntime: Sv2Option::new(None),
            version: 536870912,
            version_rolling_allowed: true,
            coinbase_tx_prefix: vec![
                2, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 34, 82, 0,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_suffix: vec![
                255, 255, 255, 255, 2, 0, 242, 5, 42, 1, 0, 0, 0, 22, 0, 20, 235, 225, 183, 220,
                194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194, 8, 252, 0, 0, 0,
                0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209, 222,
                253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180, 139,
                235, 216, 54, 151, 78, 140, 249, 1, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ]
            .try_into()
            .unwrap(),
            merkle_path: vec![].try_into().unwrap(),
        };

        channel
            .on_new_extended_mining_job(future_job.clone())
            .unwrap();

        // network target: 7fffff0000000000000000000000000000000000000000000000000000000000
        let nbits = 545259519;
        let prev_hash = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ];
        let ntime: u32 = 1746839905;
        let set_new_prev_hash = SetNewPrevHashMp {
            channel_id,
            job_id: future_job.job_id,
            prev_hash: prev_hash.into(),
            nbits,
            min_ntime: ntime,
        };

        channel.on_set_new_prev_hash(set_new_prev_hash).unwrap();

        // this share has hash 38b6a7d5b2cae08bc6c8b4b4fc13ff129ae0a07309240108f46ddf48c498b120
        // which satisfies network target
        // 7fffff0000000000000000000000000000000000000000000000000000000000
        let share_valid_block = SubmitSharesExtended {
            channel_id,
            sequence_number: 0,
            job_id: 1,
            nonce: 741057,
            ntime: 1745596971,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0].try_into().unwrap(),
        };

        let res = channel.validate_share(share_valid_block);

        assert!(matches!(res, Ok(ShareValidationResult::BlockFound)));
    }

    #[test]
    fn test_share_validation_does_not_meet_target() {
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        // channel target: 0000ffff00000000000000000000000000000000000000000000000000000000
        let target = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0x00, 0x00,
        ]
        .into();
        let nominal_hashrate = 1.0;
        let version_rolling = true;
        let rollable_extranonce_size = (MAX_EXTRANONCE_LEN - extranonce_prefix.len()) as u16;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            extranonce_prefix.clone(),
            target,
            nominal_hashrate,
            version_rolling,
            rollable_extranonce_size,
        );

        let future_job = NewExtendedMiningJob {
            channel_id: 1,
            job_id: 1,
            min_ntime: Sv2Option::new(None),
            version: 536870912,
            version_rolling_allowed: true,
            coinbase_tx_prefix: vec![
                2, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 34, 82, 0,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_suffix: vec![
                255, 255, 255, 255, 2, 0, 242, 5, 42, 1, 0, 0, 0, 22, 0, 20, 235, 225, 183, 220,
                194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194, 8, 252, 0, 0, 0,
                0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209, 222,
                253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180, 139,
                235, 216, 54, 151, 78, 140, 249, 1, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ]
            .try_into()
            .unwrap(),
            merkle_path: vec![].try_into().unwrap(),
        };

        channel
            .on_new_extended_mining_job(future_job.clone())
            .unwrap();

        // network target: 000000000000d7c0000000000000000000000000000000000000000000000000
        let nbits = 453040064;
        let prev_hash = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ];
        let ntime: u32 = 1746839905;
        let set_new_prev_hash = SetNewPrevHashMp {
            channel_id,
            job_id: future_job.job_id,
            prev_hash: prev_hash.into(),
            nbits,
            min_ntime: ntime,
        };

        channel.on_set_new_prev_hash(set_new_prev_hash).unwrap();

        // this share has hash bc1f25bceec05b1cc60fd0f0a3ede685efbb00d2a7d39c879d2c187b2af3538d
        // which does not meet the channel target
        // 0000ffff00000000000000000000000000000000000000000000000000000000
        let share_low_diff = SubmitSharesExtended {
            channel_id,
            sequence_number: 0,
            job_id: 1,
            nonce: 741057,
            ntime: 1745596971,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0].try_into().unwrap(),
        };

        let res = channel.validate_share(share_low_diff);

        assert!(matches!(
            res.unwrap_err(),
            ShareValidationError::DoesNotMeetTarget
        ));
    }

    #[test]
    fn test_share_validation_valid_share() {
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        // channel target: 0000ffff00000000000000000000000000000000000000000000000000000000
        let target = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0x00, 0x00,
        ]
        .into();
        let nominal_hashrate = 1.0;
        let version_rolling = true;
        let rollable_extranonce_size = (MAX_EXTRANONCE_LEN - extranonce_prefix.len()) as u16;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            extranonce_prefix.clone(),
            target,
            nominal_hashrate,
            version_rolling,
            rollable_extranonce_size,
        );

        let future_job = NewExtendedMiningJob {
            channel_id: 1,
            job_id: 1,
            min_ntime: Sv2Option::new(None),
            version: 536870912,
            version_rolling_allowed: true,
            coinbase_tx_prefix: vec![
                2, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 34, 82, 0,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_suffix: vec![
                255, 255, 255, 255, 2, 0, 242, 5, 42, 1, 0, 0, 0, 22, 0, 20, 235, 225, 183, 220,
                194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194, 8, 252, 0, 0, 0,
                0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209, 222,
                253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180, 139,
                235, 216, 54, 151, 78, 140, 249, 1, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ]
            .try_into()
            .unwrap(),
            merkle_path: vec![].try_into().unwrap(),
        };

        channel
            .on_new_extended_mining_job(future_job.clone())
            .unwrap();

        // network target: 000000000000d7c0000000000000000000000000000000000000000000000000
        let nbits: u32 = 453040064;
        let ntime: u32 = 1746839905;
        let prev_hash = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ];
        let set_new_prev_hash = SetNewPrevHashMp {
            channel_id,
            job_id: future_job.job_id,
            prev_hash: prev_hash.into(),
            nbits,
            min_ntime: ntime,
        };

        channel.on_set_new_prev_hash(set_new_prev_hash).unwrap();

        // this share has hash 0000d769e5ab58b7309b7507834cb0bc60749315c93015e8bba97b9752ced5b7
        // which does meet the channel target
        // 0000ffff00000000000000000000000000000000000000000000000000000000
        let valid_share = SubmitSharesExtended {
            channel_id,
            sequence_number: 0,
            job_id: 1,
            nonce: 2426,
            ntime: 1745596971,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0].try_into().unwrap(),
        };

        let res = channel.validate_share(valid_share);

        assert!(matches!(res, Ok(ShareValidationResult::Valid)));

        // try to cheat by re-submitting the same share
        // with a different sequence number
        let repeated_share = SubmitSharesExtended {
            channel_id,
            sequence_number: 1,
            job_id: 1,
            nonce: 2426,
            ntime: 1745596971,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0].try_into().unwrap(),
        };

        let res = channel.validate_share(repeated_share);

        assert!(matches!(
            res.unwrap_err(),
            ShareValidationError::DuplicateShare
        ));
    }
}
