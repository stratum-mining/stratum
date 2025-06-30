use super::Job;
use crate::{
    channels::{
        chain_tip::ChainTip,
        server::jobs::{error::ExtendedJobError, JobOrigin},
    },
    template_distribution_sv2::NewTemplate,
    utils::deserialize_template_outputs,
};
use bitcoin::{
    consensus::{deserialize, serialize},
    transaction::{Transaction, TxOut},
};
use codec_sv2::binary_sv2::{Seq0255, Sv2Option, B0255, B064K, U256};
use mining_sv2::{NewExtendedMiningJob, SetCustomMiningJob, MAX_EXTRANONCE_LEN};
use std::convert::TryInto;

/// Abstraction of an extended mining job with:
/// - the `NewTemplate` OR `SetCustomMiningJob` message that originated it
/// - the extranonce prefix associated with the channel at the time of job creation
/// - all coinbase outputs (spendable + unspendable) associated with the job
/// - the `NewExtendedMiningJob` message to be sent across the wire
#[derive(Debug, Clone)]
pub struct ExtendedJob<'a> {
    origin: JobOrigin<'a>,
    extranonce_prefix: Vec<u8>,
    coinbase_outputs: Vec<TxOut>,
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
            job_message,
        })
    }

    pub fn from_custom_job(
        custom_job: SetCustomMiningJob<'a>,
        extranonce_prefix: Vec<u8>,
        coinbase_outputs: Vec<TxOut>,
        job_message: NewExtendedMiningJob<'a>,
    ) -> Self {
        Self {
            origin: JobOrigin::SetCustomMiningJob(custom_job),
            extranonce_prefix,
            coinbase_outputs,
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
        let coinbase_tx_prefix = self.get_coinbase_tx_prefix().inner_as_ref();
        let coinbase_tx_suffix = self.get_coinbase_tx_suffix().inner_as_ref();

        let mut serialized_coinbase: Vec<u8> = vec![];
        serialized_coinbase.extend(coinbase_tx_prefix);
        serialized_coinbase.extend(vec![0; MAX_EXTRANONCE_LEN]);
        serialized_coinbase.extend(coinbase_tx_suffix);

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

    pub fn get_job_id(&self) -> u32 {
        self.job_message.job_id
    }

    pub fn get_origin(&self) -> &JobOrigin<'a> {
        &self.origin
    }

    pub fn get_coinbase_tx_prefix(&self) -> &B064K<'a> {
        &self.job_message.coinbase_tx_prefix
    }

    pub fn get_coinbase_tx_suffix(&self) -> &B064K<'a> {
        &self.job_message.coinbase_tx_suffix
    }

    pub fn get_extranonce_prefix(&self) -> &Vec<u8> {
        &self.extranonce_prefix
    }

    pub fn get_coinbase_outputs(&self) -> &Vec<TxOut> {
        &self.coinbase_outputs
    }

    pub fn get_job_message(&self) -> &NewExtendedMiningJob<'a> {
        &self.job_message
    }

    pub fn get_merkle_path(&self) -> &Seq0255<'a, U256<'a>> {
        &self.job_message.merkle_path
    }

    pub fn get_min_ntime(&self) -> Sv2Option<'a, u32> {
        self.job_message.min_ntime.clone()
    }

    pub fn get_version(&self) -> u32 {
        self.job_message.version
    }

    pub fn version_rolling_allowed(&self) -> bool {
        self.job_message.version_rolling_allowed
    }

    /// Activates the job, setting the `min_ntime` field of the `NewExtendedMiningJob` message.
    ///
    /// To be used while activating future jobs upon updating channel `ChainTip` state.
    pub fn activate(&mut self, min_ntime: u32) {
        self.job_message.min_ntime = Sv2Option::new(Some(min_ntime));
    }
}
