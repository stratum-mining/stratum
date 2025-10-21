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

pub mod error;
pub mod extended;
pub mod factory;
pub mod job_store;
pub mod standard;

use mining_sv2::SetCustomMiningJob;
use template_distribution_sv2::NewTemplate;

#[derive(Clone, Debug, PartialEq)]
pub enum JobOrigin<'a> {
    NewTemplate(NewTemplate<'a>),
    SetCustomMiningJob(SetCustomMiningJob<'a>),
}

/// Trait for mining job types in SV2 mining servers.
///
/// Types implementing `Job` must provide a unique job ID and support activation upon chain tip
/// update.
pub trait Job: Send + Sync {
    /// Returns the unique job ID for this job.
    fn get_job_id(&self) -> u32;

    /// Activates the job for a new chain tip or prev_hash header timestamp.
    fn activate(&mut self, prev_hash_header_timestamp: u32);
}
