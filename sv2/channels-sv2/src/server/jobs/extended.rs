use super::Job;
use crate::{
    outputs::deserialize_template_outputs,
    server::jobs::{error::ExtendedJobError, JobOrigin},
};
use binary_sv2::{Seq0255Owned, Sv2OptionOwned, U256Owned};
use bitcoin::transaction::TxOut;
use mining_sv2::{NewExtendedMiningJobOwned, SetCustomMiningJobOwned};
use template_distribution_sv2::NewTemplateOwned;

/// Abstraction of an extended mining job with:
/// - the [`JobOrigin`] it was built from
/// - the extranonce prefix associated with the channel at the time of job creation
/// - all coinbase outputs (spendable + unspendable) associated with the job
/// - the `NewExtendedMiningJob` message to be sent across the wire
///
/// Please note that `coinbase_tx_prefix` and `coinbase_tx_suffix` are stored in memory with bip141
/// data (marker, flag and witness). That makes it easy to reconstruct the segwit coinbase while
/// trying to propagate a block.
///
/// However, the `coinbase_tx_prefix` and `coinbase_tx_suffix` contained in the
/// `NewExtendedMiningJob` message to be sent across the wire DO NOT contain bip141 data.
/// That makes it easy to calculate the coinbase `txid` (instead of `wtxid`) for merkle root
/// calculation.
///
/// A [`PrebuiltJob`](super::PrebuiltJob) is the exception: the crate did not build its
/// coinbase, so both are its bytes as given, see
/// [`from_prebuilt_job`](Self::from_prebuilt_job).
#[derive(Debug, Clone)]
pub struct ExtendedJob {
    origin: JobOrigin,
    extranonce_prefix: Vec<u8>,
    coinbase_outputs: Vec<TxOut>,
    coinbase_tx_prefix_with_bip141: Vec<u8>,
    coinbase_tx_suffix_with_bip141: Vec<u8>,
    job_message: NewExtendedMiningJobOwned,
}

impl Job for ExtendedJob {
    fn get_job_id(&self) -> u32 {
        self.job_message.job_id
    }

    fn get_extranonce_prefix(&self) -> &[u8] {
        &self.extranonce_prefix
    }

    fn activate(&mut self, ntime_start: u32) {
        self.activate(ntime_start);
    }
}

impl ExtendedJob {
    /// Creates a new job from a template.
    ///
    /// `additional_coinbase_outputs` are added to the coinbase outputs coming from the template.
    pub fn from_template(
        template: NewTemplateOwned,
        extranonce_prefix: Vec<u8>,
        additional_coinbase_outputs: Vec<TxOut>,
        coinbase_tx_prefix: Vec<u8>,
        coinbase_tx_suffix: Vec<u8>,
        job_message: NewExtendedMiningJobOwned,
    ) -> Result<Self, ExtendedJobError> {
        let template_coinbase_outputs = deserialize_template_outputs(
            template.coinbase_tx_outputs.to_owned_bytes(),
            template.coinbase_tx_outputs_count,
        )
        .map_err(|_| ExtendedJobError::FailedToDeserializeCoinbaseOutputs)?;

        let mut coinbase_outputs = vec![];
        coinbase_outputs.extend(additional_coinbase_outputs);
        coinbase_outputs.extend(template_coinbase_outputs);

        Ok(Self {
            origin: JobOrigin::NewTemplate(template),
            extranonce_prefix,
            coinbase_outputs,
            coinbase_tx_prefix_with_bip141: coinbase_tx_prefix,
            coinbase_tx_suffix_with_bip141: coinbase_tx_suffix,
            job_message,
        })
    }
    /// Creates a new extended job from a custom mining job message.
    ///
    /// Used for jobs originating from [`SetCustomMiningJob`](mining_sv2::SetCustomMiningJob) messages.
    pub fn from_custom_job(
        custom_job: SetCustomMiningJobOwned,
        extranonce_prefix: Vec<u8>,
        coinbase_outputs: Vec<TxOut>,
        coinbase_tx_prefix: Vec<u8>,
        coinbase_tx_suffix: Vec<u8>,
        job_message: NewExtendedMiningJobOwned,
    ) -> Self {
        Self {
            origin: JobOrigin::SetCustomMiningJob(custom_job),
            extranonce_prefix,
            coinbase_outputs,
            coinbase_tx_prefix_with_bip141: coinbase_tx_prefix,
            coinbase_tx_suffix_with_bip141: coinbase_tx_suffix,
            job_message,
        }
    }

    /// Creates a new extended job from a [`PrebuiltJob`](super::PrebuiltJob).
    ///
    /// `job_message` carries the prebuilt bytes as given. They are also what the coinbase
    /// of a found block is assembled from, so
    /// [`get_coinbase_tx_prefix_with_bip141`](Self::get_coinbase_tx_prefix_with_bip141) and
    /// [`get_coinbase_tx_suffix_with_bip141`](Self::get_coinbase_tx_suffix_with_bip141) return
    /// them unchanged: the crate adds no BIP141 data to bytes it did not build. The coinbase
    /// outputs are left empty, as the bytes are not parsed.
    pub fn from_prebuilt_job(
        extranonce_prefix: Vec<u8>,
        job_message: NewExtendedMiningJobOwned,
    ) -> Self {
        Self {
            origin: JobOrigin::Prebuilt,
            extranonce_prefix,
            coinbase_outputs: vec![],
            coinbase_tx_prefix_with_bip141: job_message.coinbase_tx_prefix.to_owned_bytes(),
            coinbase_tx_suffix_with_bip141: job_message.coinbase_tx_suffix.to_owned_bytes(),
            job_message,
        }
    }

    /// Returns the job ID for this job.
    pub fn get_job_id(&self) -> u32 {
        self.job_message.job_id
    }

    /// Returns the origin of this job.
    pub fn get_origin(&self) -> &JobOrigin {
        &self.origin
    }

    /// Returns the coinbase transaction without for this job without BIP141 data.
    pub fn get_coinbase_tx_prefix_without_bip141(&self) -> Vec<u8> {
        self.job_message.coinbase_tx_prefix.to_owned_bytes()
    }

    /// Returns the coinbase transaction suffix for this job without BIP141 data.
    pub fn get_coinbase_tx_suffix_without_bip141(&self) -> Vec<u8> {
        self.job_message.coinbase_tx_suffix.to_owned_bytes()
    }

    /// Sets the extranonce prefix for this job.
    ///
    /// Meant to be used when a job was originally created for a group channel, but then is assigned to a specific extended channel.
    pub fn set_extranonce_prefix(&mut self, extranonce_prefix: Vec<u8>) {
        self.extranonce_prefix = extranonce_prefix;
    }

    /// Returns the extranonce prefix used for this job.
    pub fn get_extranonce_prefix(&self) -> &[u8] {
        &self.extranonce_prefix
    }
    /// Returns all coinbase outputs for this job. Empty for a prebuilt job, whose bytes are not
    /// parsed.
    pub fn get_coinbase_outputs(&self) -> &[TxOut] {
        &self.coinbase_outputs
    }
    /// Returns the [`NewExtendedMiningJob`](mining_sv2::NewExtendedMiningJob) message for this job.
    pub fn get_job_message(&self) -> &NewExtendedMiningJobOwned {
        &self.job_message
    }
    /// Returns the merkle path for this job.
    pub fn get_merkle_path(&self) -> &Seq0255Owned<U256Owned> {
        &self.job_message.merkle_path
    }
    /// Returns the `ntime_start` for this job (if set).
    pub fn get_ntime_start(&self) -> Option<u32> {
        self.job_message.ntime_start.as_ref().copied()
    }
    /// Returns the block version for this job.
    pub fn get_version(&self) -> u32 {
        self.job_message.version
    }
    /// Returns true if version rolling is allowed for this job.
    pub fn version_rolling_allowed(&self) -> bool {
        self.job_message.version_rolling_allowed
    }

    /// Returns the coinbase bytes before the extranonce from which the coinbase of a found block
    /// is assembled: with BIP141 data for a job the crate built, the prebuilt bytes as given
    /// otherwise (see [`from_prebuilt_job`](Self::from_prebuilt_job)).
    pub fn get_coinbase_tx_prefix_with_bip141(&self) -> Vec<u8> {
        self.coinbase_tx_prefix_with_bip141.clone()
    }

    /// Returns the coinbase bytes after the extranonce, see
    /// [`get_coinbase_tx_prefix_with_bip141`](Self::get_coinbase_tx_prefix_with_bip141).
    pub fn get_coinbase_tx_suffix_with_bip141(&self) -> Vec<u8> {
        self.coinbase_tx_suffix_with_bip141.clone()
    }

    pub fn is_future(&self) -> bool {
        self.job_message.is_future()
    }

    /// Activates the job, setting the `ntime_start` field of the `NewExtendedMiningJob` message.
    ///
    /// To be used while activating future jobs upon updating channel `ChainTip` state.
    pub fn activate(&mut self, ntime_start: u32) {
        self.job_message.ntime_start = Sv2OptionOwned::new(Some(ntime_start));
    }
}
