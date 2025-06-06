use crate::{
    channels::server::jobs::JobOrigin, template_distribution_sv2::NewTemplate,
    utils::deserialize_outputs,
};
use binary_sv2::{Seq0255, Sv2Option, B064K, U256};
use mining_sv2::{NewExtendedMiningJob, SetCustomMiningJob};
use stratum_common::bitcoin::transaction::TxOut;

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

impl<'a> ExtendedJob<'a> {
    /// Creates a new job from a template.
    ///
    /// `additional_coinbase_outputs` are added to the coinbase outputs coming from the template.
    pub fn from_template(
        template: NewTemplate<'a>,
        extranonce_prefix: Vec<u8>,
        additional_coinbase_outputs: Vec<TxOut>,
        job_message: NewExtendedMiningJob<'a>,
    ) -> Self {
        let mut coinbase_outputs = vec![];
        coinbase_outputs.extend(additional_coinbase_outputs);
        coinbase_outputs.extend(deserialize_outputs(
            template.coinbase_tx_outputs.inner_as_ref().to_vec(),
        ));

        Self {
            origin: JobOrigin::NewTemplate(template),
            extranonce_prefix,
            coinbase_outputs,
            job_message,
        }
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
    /// - creating a non-future extended job from a non-future template
    /// - activating a future extended job (that was created from a future template)
    pub fn into_custom_job(self) -> SetCustomMiningJob<'a> {
        // we need to wait for the outcome of the discussions around
        // https://github.com/stratum-mining/sv2-spec/issues/133
        // before implementing this
        todo!()
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
