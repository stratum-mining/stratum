use crate::utils::deserialize_outputs;
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

impl<'a> StandardJob<'a> {
    pub fn from_template(
        template: NewTemplate<'a>,
        extranonce_prefix: Vec<u8>,
        additional_coinbase_outputs: Vec<TxOut>,
        job_message: NewMiningJob<'a>,
    ) -> Self {
        let mut coinbase_outputs = vec![];
        coinbase_outputs.extend(additional_coinbase_outputs);
        coinbase_outputs.extend(deserialize_outputs(
            template.coinbase_tx_outputs.inner_as_ref().to_vec(),
        ));
        Self {
            template,
            extranonce_prefix,
            coinbase_outputs,
            job_message,
        }
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
