use crate::{
    channels::server::jobs::{error::StandardJobError, Job},
    utils::deserialize_template_outputs,
};
use bitcoin::transaction::TxOut;
use codec_sv2::binary_sv2::{Sv2Option, U256};
use mining_sv2::NewMiningJob;
use template_distribution_sv2::NewTemplate;

/// Abstraction of a standard mining job with:
/// - the `NewTemplate` message that originated it
/// - the extranonce prefix associated with the channel at the time of job creation
/// - all coinbase outputs (spendable + unspendable) associated with the job
/// - the `NewMiningJob` message to be sent across the wire
#[derive(Debug, Clone)]
pub struct StandardJob<'a> {
    template: NewTemplate<'a>,
    extranonce_prefix: Vec<u8>,
    coinbase_outputs: Vec<TxOut>,
    job_message: NewMiningJob<'a>,
}

impl Job for StandardJob<'_> {
    fn get_job_id(&self) -> u32 {
        self.job_message.job_id
    }

    fn activate(&mut self, min_ntime: u32) {
        self.activate(min_ntime);
    }
}

impl<'a> StandardJob<'a> {
    pub fn from_template(
        template: NewTemplate<'a>,
        extranonce_prefix: Vec<u8>,
        additional_coinbase_outputs: Vec<TxOut>,
        job_message: NewMiningJob<'a>,
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
            template,
            extranonce_prefix,
            coinbase_outputs,
            job_message,
        })
    }

    pub fn get_job_id(&self) -> u32 {
        self.job_message.job_id
    }

    pub fn get_coinbase_outputs(&self) -> &Vec<TxOut> {
        &self.coinbase_outputs
    }

    pub fn get_extranonce_prefix(&self) -> &Vec<u8> {
        &self.extranonce_prefix
    }

    pub fn get_job_message(&self) -> &NewMiningJob<'a> {
        &self.job_message
    }

    pub fn get_template(&self) -> &NewTemplate<'a> {
        &self.template
    }

    pub fn get_merkle_root(&self) -> &U256<'a> {
        &self.job_message.merkle_root
    }

    pub fn is_future(&self) -> bool {
        self.job_message.min_ntime.clone().into_inner().is_none()
    }

    pub fn activate(&mut self, min_ntime: u32) {
        self.job_message.min_ntime = Sv2Option::new(Some(min_ntime));
    }
}
