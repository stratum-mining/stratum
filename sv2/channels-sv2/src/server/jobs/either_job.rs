use crate::{
    merkle_root::merkle_root_from_path,
    outputs::deserialize_template_outputs,
    server::jobs::{
        error::{ExtendedJobError, StandardJobError},
        Job, JobMessage, JobOrigin,
    },
};
use binary_sv2::{Seq0255, Sv2Option, U256};
use bitcoin::transaction::TxOut;
use mining_sv2::{NewExtendedMiningJob, NewMiningJob, SetCustomMiningJob};
use std::{convert::TryInto, ops::Deref};
use template_distribution_sv2::NewTemplate;

#[derive(Debug, Clone, PartialEq)]
pub enum EitherJobVariant {
    Extended,
    Standard,
}

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
#[derive(Debug, Clone, PartialEq)]
pub struct EitherJob<'a> {
    origin: JobOrigin<'a>,
    extranonce_prefix: Vec<u8>,
    coinbase_outputs: Vec<TxOut>,
    // StandardJob specific field
    merkle_root: Option<U256<'a>>,
    // ExtendedJob specific fields
    coinbase_tx_prefix_with_bip141: Option<Vec<u8>>,
    coinbase_tx_suffix_with_bip141: Option<Vec<u8>>,
    job_message: JobMessage<'a>,
    variant: EitherJobVariant,
}

impl<'a> Job<'a> for EitherJob<'a> {
    fn get_job_id(&self) -> u32 {
        self.job_message.get_job_id()
    }
    fn get_origin(&self) -> &JobOrigin<'a> {
        &self.origin
    }
    fn get_extranonce_prefix(&self) -> &Vec<u8> {
        &self.extranonce_prefix
    }
    fn get_coinbase_outputs(&self) -> &Vec<TxOut> {
        &self.coinbase_outputs
    }
    fn get_job_message(&self) -> &JobMessage<'a> {
        &self.job_message
    }
    fn get_min_ntime(&self) -> Sv2Option<'a, u32> {
        self.job_message.get_min_ntime()
    }
    fn get_version(&self) -> u32 {
        self.job_message.get_version()
    }
    fn version_rolling_allowed(&self) -> bool {
        self.job_message.version_rolling_allowed()
    }
    fn get_merkle_root(&self) -> Option<&U256<'a>> {
        self.merkle_root.as_ref()
    }
    /// Returns the merkle path for this job.
    fn get_merkle_path(&self) -> &Seq0255<'a, U256<'a>> {
        &self.job_message.get_merkle_path()
    }
    fn get_coinbase_tx_prefix_with_bip141(&self) -> Option<Vec<u8>> {
        self.coinbase_tx_prefix_with_bip141.clone()
    }
    fn get_coinbase_tx_suffix_with_bip141(&self) -> Option<Vec<u8>> {
        self.coinbase_tx_suffix_with_bip141.clone()
    }
    /// Returns the coinbase transaction without for this job without BIP141 data.
    fn get_coinbase_tx_prefix_without_bip141(&self) -> Vec<u8> {
        self.job_message.get_coinbase_tx_prefix_without_bip141()
    }
    /// Returns the coinbase transaction suffix for this job without BIP141 data.
    fn get_coinbase_tx_suffix_without_bip141(&self) -> Vec<u8> {
        self.job_message.get_coinbase_tx_suffix_without_bip141()
    }
    fn is_future(&self) -> bool {
        self.job_message.get_min_ntime().0.is_empty()
    }
    fn activate(&mut self, min_ntime: u32) {
        self.job_message.activate(min_ntime);
    }
}

impl<'a> EitherJob<'a> {
    /// Creates a new standard job from a template.
    ///
    /// Combines coinbase outputs from the template and any additional outputs.
    /// Returns an error if coinbase outputs cannot be deserialized.
    pub fn standard_from_template(
        template: NewTemplate<'a>,
        extranonce_prefix: Vec<u8>,
        additional_coinbase_outputs: Vec<TxOut>,
        job_message: JobMessage<'a>,
    ) -> Result<Self, StandardJobError> {
        let template_coinbase_outputs = deserialize_template_outputs(
            template.coinbase_tx_outputs.to_vec(),
            template.coinbase_tx_outputs_count,
        )
        .map_err(|_| StandardJobError::FailedToDeserializeCoinbaseOutputs)?;

        let mut coinbase_outputs = vec![];
        coinbase_outputs.extend(additional_coinbase_outputs);
        coinbase_outputs.extend(template_coinbase_outputs);

        Ok(Self {
            origin: JobOrigin::NewTemplate(template),
            extranonce_prefix,
            coinbase_outputs,
            merkle_root: None,
            coinbase_tx_prefix_with_bip141: None,
            coinbase_tx_suffix_with_bip141: None,
            job_message,
            variant: EitherJobVariant::Standard,
        })
    }

    pub fn extended_from_template(
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
            merkle_root: None,
            coinbase_tx_prefix_with_bip141: Some(coinbase_tx_prefix),
            coinbase_tx_suffix_with_bip141: Some(coinbase_tx_suffix),
            job_message: JobMessage::NewExtendedMiningJob(job_message),
            variant: EitherJobVariant::Extended,
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
        job_message: JobMessage<'a>,
    ) -> Self {
        Self {
            origin: JobOrigin::SetCustomMiningJob(custom_job),
            extranonce_prefix,
            coinbase_outputs,
            merkle_root: None,
            coinbase_tx_prefix_with_bip141: Some(coinbase_tx_prefix),
            coinbase_tx_suffix_with_bip141: Some(coinbase_tx_suffix),
            job_message,
            variant: EitherJobVariant::Extended,
        }
    }

    /// Converts the `ExtendedJob` into a `StandardJob`.
    ///
    /// Only possible if the job was created from a `NewTemplate`.
    /// Jobs created from `SetCustomMiningJob` cannot be converted
    pub fn into_standard_job(
        self,
        channel_id: u32,
        extranonce_prefix: Vec<u8>,
    ) -> Result<Self, ExtendedJobError> {
        // here we can only convert extended jobs that were created from a template
        let template = match self.get_origin() {
            JobOrigin::NewTemplate(template) => template,
            JobOrigin::SetCustomMiningJob(_) => {
                return Err(ExtendedJobError::FailedToConvertToStandardJob);
            }
        };

        let merkle_root: U256<'a> = merkle_root_from_path(
            &&self.get_coinbase_tx_prefix_without_bip141(),
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
            merkle_root: merkle_root.clone(),
            version: self.get_version(),
            min_ntime: self.get_min_ntime(),
        };

        let mut standard_job = EitherJob::standard_from_template(
            template.clone(),
            extranonce_prefix,
            self.get_coinbase_outputs().clone(),
            JobMessage::NewMiningJob(standard_job_message),
        )
        .map_err(|_| ExtendedJobError::FailedToConvertToStandardJob)?;
        standard_job.variant = EitherJobVariant::Standard;
        Ok(standard_job)
    }
}
