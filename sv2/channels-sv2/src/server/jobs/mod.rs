//! Mining job construction - Mining Server Abstraction.
//!
//! This module provides submodules and traits for representing, constructing, and managing
//! Extended and Standard mining jobs in Stratum V2 (SV2) mining servers.
//!
//! ## Responsibilities
//!
//! - **Error Handling**: See [`error`] submodule for job-related error types.
//! - **Extended Jobs**: See [`extended`] submodule for SV2 extended job implementation.
//! - **Standard Jobs**: See [`standard`] submodule for SV2 standard job implementation.
//! - **Job Factories**: See [`factory`] for job creation logic and unique job ID assignment.
//! - **Job Storage**: See [`job_store`] for job lifecycle management and storage abstractions.
//! - **Job Origin Tracking**: Tracks job origin (template or custom job message).
//! - **Job Trait**: Unified trait for all mining job types, supporting activation and job ID
//!   retrieval.
//!
//! ## Usage
//!
//! Use these abstractions for implementing mining server job management and SV2 protocol logic.

pub mod either_job;
pub mod error;
pub mod factory;
pub mod job_store;

use binary_sv2::{Seq0255, Sv2Option, U256};
use bitcoin::TxOut;
use mining_sv2::SetCustomMiningJob;
use template_distribution_sv2::NewTemplate;

#[derive(Clone, Debug, PartialEq)]
pub enum JobOrigin<'a> {
    NewTemplate(NewTemplate<'a>),
    SetCustomMiningJob(SetCustomMiningJob<'a>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum JobMessage<'a> {
    NewExtendedMiningJob(mining_sv2::NewExtendedMiningJob<'a>),
    NewMiningJob(mining_sv2::NewMiningJob<'a>),
}

impl<'a> JobMessage<'a> {
    pub fn get_job_id(&self) -> u32 {
        match self {
            JobMessage::NewExtendedMiningJob(msg) => msg.job_id,
            JobMessage::NewMiningJob(msg) => msg.job_id,
        }
    }
    pub fn get_min_ntime(&self) -> Sv2Option<'a, u32> {
        match self {
            JobMessage::NewExtendedMiningJob(msg) => msg.min_ntime.clone(),
            JobMessage::NewMiningJob(msg) => msg.min_ntime.clone(),
        }
    }
    fn get_version(&self) -> u32 {
        match self {
            JobMessage::NewExtendedMiningJob(msg) => msg.version,
            JobMessage::NewMiningJob(msg) => msg.version,
        }
    }
    fn version_rolling_allowed(&self) -> bool {
        match self {
            JobMessage::NewExtendedMiningJob(msg) => msg.version_rolling_allowed,
            JobMessage::NewMiningJob(_msg) => false,
        }
    }
    pub fn get_merkle_root(&self) -> &U256<'a> {
        match self {
            JobMessage::NewExtendedMiningJob(_msg) => {
                panic!("Merkle root is not directly available for NewExtendedMiningJob")
            }
            JobMessage::NewMiningJob(msg) => &msg.merkle_root,
        }
    }
    pub fn get_merkle_path(&self) -> &Seq0255<'a, U256<'a>> {
        match self {
            JobMessage::NewExtendedMiningJob(msg) => &msg.merkle_path,
            JobMessage::NewMiningJob(_msg) => {
                panic!("Merkle path is not available for NewMiningJob")
            }
        }
    }
    /// Returns the coinbase transaction without for this job without BIP141 data.
    pub fn get_coinbase_tx_prefix_without_bip141(&self) -> Vec<u8> {
        match self {
            JobMessage::NewExtendedMiningJob(msg) => msg.coinbase_tx_prefix.inner_as_ref().to_vec(),
            JobMessage::NewMiningJob(_msg) => {
                panic!("coinbase_tx_prefix_without_bip141 is not available for NewMiningJob")
            }
        }
    }

    /// Returns the coinbase transaction suffix for this job without BIP141 data.
    pub fn get_coinbase_tx_suffix_without_bip141(&self) -> Vec<u8> {
        match self {
            JobMessage::NewExtendedMiningJob(msg) => msg.coinbase_tx_suffix.inner_as_ref().to_vec(),
            JobMessage::NewMiningJob(_msg) => {
                panic!("coinbase_tx_suffix_without_bip141 is not available for NewMiningJob")
            }
        }
    }
    pub fn activate(&mut self, min_ntime: u32) {
        match self {
            JobMessage::NewExtendedMiningJob(msg) => {
                msg.min_ntime = Sv2Option::new(Some(min_ntime));
            }
            JobMessage::NewMiningJob(msg) => {
                msg.min_ntime = Sv2Option::new(Some(min_ntime));
            }
        }
    }
}

pub trait Job<'a> {
    fn get_job_id(&self) -> u32;
    fn get_template_id(&self) -> u64 {
        match self.get_origin() {
            JobOrigin::NewTemplate(template) => template.template_id,
            JobOrigin::SetCustomMiningJob(_) => 0,
        }
    }
    fn get_origin(&self) -> &JobOrigin<'a>;
    fn get_extranonce_prefix(&self) -> &Vec<u8>;
    fn get_coinbase_outputs(&self) -> &Vec<TxOut>;
    fn get_job_message(&self) -> &JobMessage<'a>;
    fn get_min_ntime(&self) -> Sv2Option<'a, u32>;
    fn get_version(&self) -> u32;
    fn version_rolling_allowed(&self) -> bool;
    fn get_merkle_root(&self, full_extranonce: Option<&[u8]>) -> Option<U256<'a>>;
    /// Returns the merkle path for this job.
    fn get_merkle_path(&self) -> &Seq0255<'a, U256<'a>>;
    fn get_coinbase_tx_prefix_with_bip141(&self) -> Vec<u8>;
    fn get_coinbase_tx_suffix_with_bip141(&self) -> Vec<u8>;
    /// Returns the coinbase transaction without for this job without BIP141 data.
    fn get_coinbase_tx_prefix_without_bip141(&self) -> Vec<u8>;
    fn get_coinbase_tx_suffix_without_bip141(&self) -> Vec<u8>;
    fn is_future(&self) -> bool;
    fn activate(&mut self, min_ntime: u32);
}
