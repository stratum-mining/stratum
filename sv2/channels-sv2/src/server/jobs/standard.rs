//! Abstraction of a standard mining job for SV2 mining servers.
//!
//! This module provides the [`StandardJob`] struct, which encapsulates all the state and
//! protocol-relevant data for a standard mining job as handled by a mining server.
//!
//! ## Responsibilities
//!
//! - **Origin Tracking**: Captures the originating `NewTemplate` message and extranonce prefix at
//!   creation time.
//! - **Coinbase Outputs Management**: Combines spendable and unspendable coinbase outputs from the
//!   template and additional outputs.
//! - **Wire-format Message**: Stores the protocol wire-format `NewMiningJob` message for downstream
//!   communication.
//! - **Lifecycle Management**: Supports activation and state transitions of jobs, including
//!   future/non-future status.
//!
//! ## Usage
//!
//! Use this struct when creating, activating, or managing standard mining jobs in SV2-compliant
//! mining servers.

use crate::{
    merkle_root::merkle_root_from_path,
    outputs::deserialize_template_outputs,
    server::jobs::{error::StandardJobError, extended::ExtendedJob, Job, JobOrigin},
};
use binary_sv2::{Sv2OptionOwned, U256Owned};
use bitcoin::transaction::TxOut;
use mining_sv2::NewMiningJobOwned;
use template_distribution_sv2::NewTemplateOwned;

/// Abstraction of a standard mining job with:
/// - the `NewTemplate` message that originated it
/// - the extranonce prefix associated with the channel at the time of job creation
/// - all coinbase outputs (spendable + unspendable) associated with the job
/// - the `NewMiningJob` message to be sent across the wire
#[derive(Debug, Clone)]
pub struct StandardJob {
    template: NewTemplateOwned,
    extranonce_prefix: Vec<u8>,
    coinbase_outputs: Vec<TxOut>,
    job_message: NewMiningJobOwned,
}

impl Job for StandardJob {
    /// Returns the job ID for this job.
    fn get_job_id(&self) -> u32 {
        self.job_message.job_id
    }

    /// Returns the extranonce prefix this job was created under.
    fn get_extranonce_prefix(&self) -> &[u8] {
        &self.extranonce_prefix
    }

    /// Activates the job by setting the `ntime_start` field.
    fn activate(&mut self, ntime_start: u32) {
        self.activate(ntime_start);
    }
}

impl StandardJob {
    /// Creates a new standard job from a template.
    ///
    /// Combines coinbase outputs from the template and any additional outputs.
    /// Returns an error if coinbase outputs cannot be deserialized.
    pub fn from_template(
        template: NewTemplateOwned,
        extranonce_prefix: Vec<u8>,
        additional_coinbase_outputs: Vec<TxOut>,
        job_message: NewMiningJobOwned,
    ) -> Result<Self, StandardJobError> {
        let template_coinbase_outputs = deserialize_template_outputs(
            template.coinbase_tx_outputs.to_owned_bytes(),
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
    /// Creates a standard job out of the extended job broadcast to a group channel.
    ///
    /// The standard job shares the extended job's coinbase: its merkle root is derived from the
    /// extended job's coinbase prefix and suffix around `extranonce_prefix`, and it carries the
    /// extended job's coinbase outputs as they are.
    ///
    /// Group channel jobs always originate from a `NewTemplate`, which is also what a
    /// [`StandardJob`] is anchored to (its block-found coinbase and `template_id` are read from
    /// it). An extended job originating from `SetCustomMiningJob` belongs to a single extended
    /// channel, never to a group channel, so passing one here is refused with
    /// [`StandardJobError::InvalidJobOrigin`].
    pub(crate) fn from_group_extended_job(
        extended_job: &ExtendedJob,
        channel_id: u32,
        extranonce_prefix: Vec<u8>,
    ) -> Result<Self, StandardJobError> {
        let template = match extended_job.get_origin() {
            JobOrigin::NewTemplate(template) => template,
            JobOrigin::SetCustomMiningJob(_) => return Err(StandardJobError::InvalidJobOrigin),
        };

        let merkle_root = merkle_root_from_path(
            &extended_job.get_coinbase_tx_prefix_without_bip141(),
            &extended_job.get_coinbase_tx_suffix_without_bip141(),
            &extranonce_prefix,
            extended_job.get_merkle_path().as_slice(),
        )
        .ok_or(StandardJobError::FailedToCalculateMerkleRoot)?
        .into();

        let job_message = NewMiningJobOwned {
            channel_id,
            job_id: extended_job.get_job_id(),
            merkle_root,
            version: extended_job.get_version(),
            ntime_start: Sv2OptionOwned::new(extended_job.get_ntime_start()),
        };

        Ok(Self {
            template: template.clone(),
            extranonce_prefix,
            coinbase_outputs: extended_job.get_coinbase_outputs().to_vec(),
            job_message,
        })
    }
    /// Returns the job ID for this job.
    pub fn get_job_id(&self) -> u32 {
        self.job_message.job_id
    }
    /// Returns all coinbase outputs (spendable and unspendable) for this job.
    pub fn get_coinbase_outputs(&self) -> &[TxOut] {
        &self.coinbase_outputs
    }
    /// Returns the extranonce prefix used for this job.
    pub fn get_extranonce_prefix(&self) -> &[u8] {
        &self.extranonce_prefix
    }
    /// Returns the `NewMiningJob` message for this job.
    pub fn get_job_message(&self) -> &NewMiningJobOwned {
        &self.job_message
    }
    /// Returns the originating `NewTemplate` message for this job.
    pub fn get_template(&self) -> &NewTemplateOwned {
        &self.template
    }
    /// Returns the merkle root for this job.
    pub fn get_merkle_root(&self) -> &U256Owned {
        &self.job_message.merkle_root
    }
    /// Returns the block version for this job.
    pub fn get_version(&self) -> u32 {
        self.job_message.version
    }
    /// Returns the `ntime_start` for this job (if set).
    pub fn get_ntime_start(&self) -> Option<u32> {
        self.job_message.ntime_start.as_ref().copied()
    }
    /// Returns true if the job is a future job (not yet activated).
    pub fn is_future(&self) -> bool {
        self.get_ntime_start().is_none()
    }
    /// Activates the job by setting the `ntime_start` field.
    ///
    /// Should be called when activating future jobs.
    pub fn activate(&mut self, ntime_start: u32) {
        self.job_message.ntime_start = Sv2OptionOwned::new(Some(ntime_start));
    }
}
