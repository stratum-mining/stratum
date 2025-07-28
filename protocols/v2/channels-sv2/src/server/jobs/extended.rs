use super::Job;
use crate::{
    chain_tip::ChainTip,
    merkle_root::merkle_root_from_path,
    server::jobs::{error::ExtendedJobError, standard::StandardJob, JobOrigin},
    template::deserialize_template_outputs,
};
use binary_sv2::{Seq0255, Sv2Option, B0255, B064K, U256};
use bitcoin::{
    consensus::{deserialize, serialize},
    transaction::{Transaction, TxOut},
};
use mining_sv2::{NewExtendedMiningJob, NewMiningJob, SetCustomMiningJob, FULL_EXTRANONCE_LEN};
use std::convert::TryInto;
use template_distribution_sv2::NewTemplate;

/// Abstraction of an extended mining job with:
/// - the `NewTemplate` OR `SetCustomMiningJob` message that originated it
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
#[derive(Debug, Clone)]
pub struct ExtendedJob<'a> {
    origin: JobOrigin<'a>,
    extranonce_prefix: Vec<u8>,
    coinbase_outputs: Vec<TxOut>,
    coinbase_tx_prefix_with_bip141: Vec<u8>,
    coinbase_tx_suffix_with_bip141: Vec<u8>,
    job_message: NewExtendedMiningJob<'a>,
}

impl Job for ExtendedJob<'_> {
    fn get_job_id(&self) -> u32 {
        self.job_message.job_id
    }

    fn activate(&mut self, min_ntime: u32) {
        self.activate(min_ntime);
    }
}

impl<'a> ExtendedJob<'a> {
    /// Creates a new job from a template.
    ///
    /// `additional_coinbase_outputs` are added to the coinbase outputs coming from the template.
    pub fn from_template(
        template: NewTemplate<'a>,
        extranonce_prefix: Vec<u8>,
        additional_coinbase_outputs: Vec<TxOut>,
        coinbase_tx_prefix: Vec<u8>,
        coinbase_tx_suffix: Vec<u8>,
        job_message: NewExtendedMiningJob<'a>,
    ) -> Result<Self, ExtendedJobError> {
        let template_coinbase_outputs = deserialize_template_outputs(
            template.coinbase_tx_outputs.to_vec(),
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
    /// Used for jobs originating from [`SetCustomMiningJob`] messages.
    pub fn from_custom_job(
        custom_job: SetCustomMiningJob<'a>,
        extranonce_prefix: Vec<u8>,
        coinbase_outputs: Vec<TxOut>,
        coinbase_tx_prefix: Vec<u8>,
        coinbase_tx_suffix: Vec<u8>,
        job_message: NewExtendedMiningJob<'a>,
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

    /// Converts the `ExtendedJob` into a `SetCustomMiningJob` message.
    ///
    /// To be used by a Sv2 Job Declaration Client after:
    /// - a non-future `ExtendedJob` was created from a non-future `NewTemplate`
    /// - a future `ExtendedJob` was activated into a non-future `ExtendedJob`
    ///
    /// In other words, a future `ExtendedJob` cannot be converted into a `SetCustomMiningJob`.
    pub fn into_custom_job(
        self,
        request_id: u32,
        token: B0255<'a>,
        chain_tip: ChainTip,
    ) -> Result<SetCustomMiningJob<'a>, ExtendedJobError> {
        let coinbase_tx_prefix = self.get_coinbase_tx_prefix_with_bip141();
        let coinbase_tx_suffix = self.get_coinbase_tx_suffix_with_bip141();

        let mut serialized_coinbase: Vec<u8> = vec![];
        serialized_coinbase.extend(coinbase_tx_prefix.clone());
        serialized_coinbase.extend(vec![0; FULL_EXTRANONCE_LEN]);
        serialized_coinbase.extend(coinbase_tx_suffix.clone());

        let deserialized_coinbase: Transaction = deserialize(&serialized_coinbase)
            .map_err(|_| ExtendedJobError::FailedToDeserializeCoinbase)?;

        if deserialized_coinbase.input.len() != 1 {
            return Err(ExtendedJobError::CoinbaseInputCountMismatch);
        }

        let min_ntime = if let Some(job_min_ntime) = self.job_message.min_ntime.clone().into_inner()
        {
            // job min_ntime must be coherent with the provided chain tip
            // because chain_tip is where prev_hash and nbits are coming from
            if job_min_ntime < chain_tip.min_ntime() {
                return Err(ExtendedJobError::InvalidMinNTime);
            } else {
                job_min_ntime
            }
        } else {
            // future jobs are not allowed to be converted into `SetCustomMiningJob` messages
            return Err(ExtendedJobError::FutureJobNotAllowed);
        };

        let prev_hash = chain_tip.prev_hash();
        let nbits = chain_tip.nbits();

        let coinbase_prefix_start_index = 4 // tx version
            + 2 // segwit bytes
            + 1 // number of inputs
            + 32 // prev OutPoint
            + 4 // index
            + 1; // bytes in script
        let coinbase_prefix: B0255<'a> = coinbase_tx_prefix[coinbase_prefix_start_index..]
            .to_vec()
            .try_into()
            .map_err(|_| ExtendedJobError::FailedToSerializeCoinbasePrefix)?;
        let coinbase_tx_version = deserialized_coinbase.version.0 as u32;
        let coinbase_tx_locktime = deserialized_coinbase.lock_time.to_consensus_u32();
        let coinbase_tx_input_n_sequence = deserialized_coinbase.input[0].sequence.0 as u32;

        let serialized_outputs = serialize(&deserialized_coinbase.output);

        let coinbase_tx_outputs: B064K<'a> = serialized_outputs
            .try_into()
            .map_err(|_| ExtendedJobError::FailedToSerializeCoinbaseOutputs)?;

        Ok(SetCustomMiningJob {
            channel_id: self.job_message.channel_id,
            request_id,
            token,
            version: self.get_version(),
            prev_hash,
            min_ntime,
            nbits,
            coinbase_tx_version,
            coinbase_prefix,
            coinbase_tx_input_n_sequence,
            coinbase_tx_outputs,
            coinbase_tx_locktime,
            merkle_path: self.get_merkle_path().clone(),
        })
    }

    /// Converts the `ExtendedJob` into a `StandardJob`.
    ///
    /// Only possible if the job was created from a `NewTemplate`.
    /// Jobs created from `SetCustomMiningJob` cannot be converted
    pub fn into_standard_job(
        self,
        channel_id: u32,
        extranonce_prefix: Vec<u8>,
    ) -> Result<StandardJob<'a>, ExtendedJobError> {
        // here we can only convert extended jobs that were created from a template
        let template = match self.get_origin() {
            JobOrigin::NewTemplate(template) => template,
            JobOrigin::SetCustomMiningJob(_) => {
                return Err(ExtendedJobError::FailedToConvertToStandardJob);
            }
        };

        let merkle_root = merkle_root_from_path(
            &self.get_coinbase_tx_prefix_without_bip141(),
            &self.get_coinbase_tx_suffix_without_bip141(),
            &extranonce_prefix,
            &self.get_merkle_path().inner_as_ref(),
        )
        .ok_or(ExtendedJobError::FailedToCalculateMerkleRoot)?
        .try_into()
        .map_err(|_| ExtendedJobError::FailedToCalculateMerkleRoot)?;

        let standard_job_message = NewMiningJob {
            channel_id,
            job_id: self.get_job_id(),
            merkle_root,
            version: self.get_version(),
            min_ntime: self.get_min_ntime(),
        };

        let standard_job = StandardJob::from_template(
            template.clone(),
            extranonce_prefix,
            self.get_coinbase_outputs().clone(),
            standard_job_message,
        )
        .map_err(|_| ExtendedJobError::FailedToConvertToStandardJob)?;

        Ok(standard_job)
    }

    /// Returns the job ID for this job.
    pub fn get_job_id(&self) -> u32 {
        self.job_message.job_id
    }

    /// Returns the origin message for this job (template or custom job).
    pub fn get_origin(&self) -> &JobOrigin<'a> {
        &self.origin
    }

    /// Returns the coinbase transaction without for this job without BIP141 data.
    pub fn get_coinbase_tx_prefix_without_bip141(&self) -> Vec<u8> {
        self.job_message.coinbase_tx_prefix.inner_as_ref().to_vec()
    }

    /// Returns the coinbase transaction suffix for this job without BIP141 data.
    pub fn get_coinbase_tx_suffix_without_bip141(&self) -> Vec<u8> {
        self.job_message.coinbase_tx_suffix.inner_as_ref().to_vec()
    }

    /// Returns the extranonce prefix used for this job.
    pub fn get_extranonce_prefix(&self) -> &Vec<u8> {
        &self.extranonce_prefix
    }
    /// Returns all coinbase outputs for this job.
    pub fn get_coinbase_outputs(&self) -> &Vec<TxOut> {
        &self.coinbase_outputs
    }
    /// Returns the [`NewExtendedMiningJob`] message for this job.
    pub fn get_job_message(&self) -> &NewExtendedMiningJob<'a> {
        &self.job_message
    }
    /// Returns the merkle path for this job.
    pub fn get_merkle_path(&self) -> &Seq0255<'a, U256<'a>> {
        &self.job_message.merkle_path
    }
    /// Returns the minimum ntime for this job (if set).
    pub fn get_min_ntime(&self) -> Sv2Option<'a, u32> {
        self.job_message.min_ntime.clone()
    }
    /// Returns the block version for this job.
    pub fn get_version(&self) -> u32 {
        self.job_message.version
    }
    /// Returns true if version rolling is allowed for this job.
    pub fn version_rolling_allowed(&self) -> bool {
        self.job_message.version_rolling_allowed
    }

    pub fn get_coinbase_tx_prefix_with_bip141(&self) -> Vec<u8> {
        self.coinbase_tx_prefix_with_bip141.clone()
    }

    pub fn get_coinbase_tx_suffix_with_bip141(&self) -> Vec<u8> {
        self.coinbase_tx_suffix_with_bip141.clone()
    }

    /// Activates the job, setting the `min_ntime` field of the `NewExtendedMiningJob` message.
    ///
    /// To be used while activating future jobs upon updating channel `ChainTip` state.
    pub fn activate(&mut self, min_ntime: u32) {
        self.job_message.min_ntime = Sv2Option::new(Some(min_ntime));
    }
}
