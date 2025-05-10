use crate::channels::server::jobs::JobOrigin;
use binary_sv2::{Sv2Option, U256};
use mining_sv2::NewMiningJob;
use stratum_common::bitcoin::transaction::TxOut;

/// Abstraction of a standard mining job with:
/// - the `NewTemplate` message that originated it
/// - the extranonce prefix associated with the channel at the time of job creation
/// - all coinbase outputs (spendable + unspendable) associated with the job
/// - the `NewMiningJob` message to be sent across the wire
#[derive(Debug, Clone)]
pub struct StandardJob<'a> {
    origin: JobOrigin<'a>,
    extranonce_prefix: Vec<u8>,
    coinbase_outputs: Vec<TxOut>,
    job_message: NewMiningJob<'a>,
}

impl<'a> StandardJob<'a> {
    pub fn new(
        origin: JobOrigin<'a>,
        extranonce_prefix: Vec<u8>,
        coinbase_outputs: Vec<TxOut>,
        job_message: NewMiningJob<'a>,
    ) -> Self {
        Self {
            origin,
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

    pub fn get_origin(&self) -> &JobOrigin<'a> {
        &self.origin
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
