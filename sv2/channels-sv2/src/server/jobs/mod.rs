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
//! - **Job Storage**: Uses an internal store for server-side job lifecycle management.
//! - **Job Origin Tracking**: Tracks job origin (template, custom job message or prebuilt job).
//! - **Prebuilt Jobs**: See [`PrebuiltJob`] for a job whose coinbase the channel's job factory
//!   does not assemble.
//! - **Job Trait**: Unified trait for all mining job types, supporting activation and job ID
//!   retrieval.
//!
//! ## Usage
//!
//! Use these abstractions for implementing mining server job management and SV2 protocol logic.

pub mod error;
pub mod extended;
pub mod factory;
pub(crate) mod job_store;
pub mod standard;

use binary_sv2::{B064KOwned, Seq0255Owned, U256Owned};
use mining_sv2::SetCustomMiningJobOwned;
use template_distribution_sv2::NewTemplateOwned;

/// The message a server job was built from.
#[derive(Clone, Debug, PartialEq)]
pub enum JobOrigin {
    NewTemplate(NewTemplateOwned),
    SetCustomMiningJob(SetCustomMiningJobOwned),
    /// A [`PrebuiltJob`], whose bytes the job's `NewExtendedMiningJob` message carries as
    /// given.
    Prebuilt,
}

impl JobOrigin {
    /// Returns the `template_id` of the template the job was built from, if it was built from
    /// one.
    pub fn template_id(&self) -> Option<u64> {
        match self {
            JobOrigin::NewTemplate(template) => Some(template.template_id),
            JobOrigin::SetCustomMiningJob(_) | JobOrigin::Prebuilt => None,
        }
    }
}

/// A job whose coinbase and merkle path the channel's
/// [`JobFactory`](factory::JobFactory) does not assemble: the application hands them over as
/// the fields a [`NewExtendedMiningJob`](mining_sv2::NewExtendedMiningJob) carries (Sv2 Mining
/// Protocol §5.3.16), and the channel serves them as given. See
/// [`GroupChannel::on_prebuilt_job`](crate::server::group::GroupChannel::on_prebuilt_job),
/// [`StandardChannel::on_prebuilt_job`](crate::server::standard::StandardChannel::on_prebuilt_job)
/// and
/// [`ExtendedChannel::on_prebuilt_job`](crate::server::extended::ExtendedChannel::on_prebuilt_job).
///
/// The factory only adds what the message takes from the channel: the job id, the `ntime_start`
/// and the version-rolling policy. Nothing is added to the bytes (no pool or miner tag), parsed
/// out of them (no coinbase outputs) or reassembled:
///
/// - The extranonce slot the bytes leave between prefix and suffix is taken to be the channel's
///   full extranonce size. The crate verifies it only as far as the bytes still deserialize as a
///   coinbase around a slot of that size (the check below): where the slot sits or what it means
///   is not checked.
/// - The bytes around a zero-filled slot must deserialize as a coinbase transaction, which is
///   how the crate computes a merkle root from them (see
///   [`merkle_root_from_path`](crate::merkle_root::merkle_root_from_path)). Otherwise the job is
///   refused with
///   [`JobFactoryError::InvalidPrebuiltCoinbase`](error::JobFactoryError::InvalidPrebuiltCoinbase)
///   and the channel is left unchanged.
/// - The coinbase of a found block is reported as
///   `coinbase_tx_prefix || extranonce || coinbase_tx_suffix`, so for a SegWit coinbase the
///   application restores the BIP141 fields it stripped.
///
/// `on_prebuilt_job` serves it either as a future job (§5.1.3), with `ntime_start` unset, to be
/// activated by the channel's `activate_future_job` with the chain tip it is for, or as a job
/// active under the channel's current chain tip, with the tip's `ntime_start`.
#[derive(Clone, Debug)]
pub struct PrebuiltJob {
    /// Block header version, see `NewExtendedMiningJob.version`.
    pub version: u32,
    /// Coinbase bytes before the extranonce, see `NewExtendedMiningJob.coinbase_tx_prefix`.
    /// Stripped of BIP141 fields if the coinbase is a SegWit transaction, as §5.3.16 requires.
    pub coinbase_tx_prefix: B064KOwned,
    /// Coinbase bytes after the extranonce, see `NewExtendedMiningJob.coinbase_tx_suffix`,
    /// stripped likewise.
    pub coinbase_tx_suffix: B064KOwned,
    /// Merkle path hashes ordered from deepest, see `NewExtendedMiningJob.merkle_path`.
    pub merkle_path: Seq0255Owned<U256Owned>,
}

/// Trait for mining job types in SV2 mining servers.
///
/// Types implementing `Job` must provide a unique job ID and support activation upon chain tip
/// update.
pub trait Job: Send + Sync {
    /// Returns the unique job ID for this job.
    fn get_job_id(&self) -> u32;

    /// Returns the extranonce prefix bytes this job was created under.
    ///
    /// Used by the job store to keep a rotated-out extranonce prefix (and therefore its
    /// allocator slot) reserved for as long as any job created under it can still accept shares.
    fn get_extranonce_prefix(&self) -> &[u8];

    /// Activates the job for a new chain tip or prev_hash header timestamp.
    fn activate(&mut self, prev_hash_ntime_start: u32);
}
