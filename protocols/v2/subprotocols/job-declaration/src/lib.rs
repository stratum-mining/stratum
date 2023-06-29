#![no_std]

//! # Job Declaration Protocol
//!
//! This protocol runs between the Job Declarator and Pool and can be
//! provided as a trusted 3rd party service for mining farms.
//!
//! Protocol flow:
//!

extern crate alloc;
mod allocate_mining_job_token;
mod commit_mining_job;
mod identify_transactions;
mod provide_missing_transactions;

pub use allocate_mining_job_token::{AllocateMiningJobToken, AllocateMiningJobTokenSuccess};
pub use commit_mining_job::{CommitMiningJob, CommitMiningJobError, CommitMiningJobSuccess};
pub use identify_transactions::{IdentifyTransactions, IdentifyTransactionsSuccess};
pub use provide_missing_transactions::{
    ProvideMissingTransactions, ProvideMissingTransactionsSuccess,
};
