#![no_std]

//! # Job Negotiation Protocol
//!
//! This protocol runs between the Job Negotiator and Pool and can be
//! provided as a trusted 3rd party service for mining farms.
//!
//! Protocol flow:
//!

extern crate alloc;
mod allocate_mining_job_token;
mod commit_mining_job;

pub use allocate_mining_job_token::{AllocateMiningJobToken, AllocateMiningJobTokenSuccess};
pub use commit_mining_job::{CommitMiningJob, CommitMiningJobSuccess};
