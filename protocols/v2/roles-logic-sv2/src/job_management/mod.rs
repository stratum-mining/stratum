pub mod job_factory;

use crate::mining_sv2::{NewExtendedMiningJob, NewMiningJob};
use binary_sv2::{binary_codec_sv2::Sv2Option, Seq0255, B064K, U256};
use std::convert::TryInto;

#[derive(Debug, Clone)]
pub struct ExtendedJob<'a> {
    template_id: Option<u64>, // None if custom job
    job_message: NewExtendedMiningJob<'a>,
}

impl<'a> ExtendedJob<'a> {
    pub fn new(template_id: Option<u64>, job_message: NewExtendedMiningJob<'a>) -> Self {
        Self {
            template_id,
            job_message,
        }
    }

    pub fn get_template_id(&self) -> Option<u64> {
        self.template_id
    }

    pub fn get_job_id(&self) -> u32 {
        self.job_message.job_id
    }

    pub fn get_coinbase_tx_prefix(&self) -> B064K<'a> {
        self.job_message.coinbase_tx_prefix.clone()
    }

    pub fn get_coinbase_tx_suffix(&self) -> B064K<'a> {
        self.job_message.coinbase_tx_suffix.clone()
    }

    pub fn get_merkle_path(&self) -> Seq0255<'a, U256<'a>> {
        self.job_message.merkle_path.clone()
    }

    pub fn get_version_rolling_allowed(&self) -> bool {
        self.job_message.version_rolling_allowed
    }

    pub fn get_version(&self) -> u32 {
        self.job_message.version
    }

    pub fn get_min_ntime(&self) -> Sv2Option<'a, u32> {
        self.job_message.min_ntime.clone()
    }

    pub fn get_job_message(&self) -> NewExtendedMiningJob<'a> {
        self.job_message.clone()
    }
}

#[derive(Debug, Clone)]
pub struct StandardJob<'a> {
    template_id: u64,
    job_message: NewMiningJob<'a>,
    coinbase: Vec<u8>,
}

impl<'a> StandardJob<'a> {
    pub fn new(template_id: u64, job_message: NewMiningJob<'a>, coinbase: Vec<u8>) -> Self {
        Self {
            template_id,
            job_message,
            coinbase,
        }
    }

    pub fn get_job_id(&self) -> u32 {
        self.job_message.job_id
    }

    pub fn get_merkle_root(&self) -> [u8; 32] {
        self.job_message
            .merkle_root
            .inner_as_ref()
            .try_into()
            .expect("merkle root must be 32 bytes")
    }

    pub fn get_job_message(&self) -> NewMiningJob<'a> {
        self.job_message.clone()
    }

    pub fn get_template_id(&self) -> u64 {
        self.template_id
    }

    pub fn get_coinbase(&self) -> Vec<u8> {
        self.coinbase.clone()
    }
}
