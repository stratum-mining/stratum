//! Abstractions for job storage and lifecycle management in SV2 mining channels.
//!
//! This module provides the [`JobStore`] trait and a default implementation for
//! tracking mining job states (future, active, past, stale) for SV2 Extended and Standard channels.
//!
//! ## Responsibilities
//!
//! - **Job Storage**: Manages collections of jobs indexed by job ID and template ID.
//! - **Job Activation**: Handles transitions between future, active, past, and stale jobs.
//! - **Template Mapping**: Tracks mappings from template IDs to job IDs for future jobs.
//! - **Lifecycle Management**: Ensures correct state transitions when activating jobs or updating
//!   chain tips.
//!
//! ## Usage
//!
//! Use the [`JobStore`] trait for custom job store implementations, or the [`DefaultJobStore`]
//! for standard job lifecycle management in mining channel abstractions.

use std::{
    collections::HashMap,
    fmt::Debug,
    ops::{Deref, DerefMut},
    rc::Rc,
    sync::Arc,
};

use bitcoin::TxOut;

use super::Job;
use crate::server::jobs::{either_job::EitherJob, JobOrigin};

/// Trait for job lifecycle management in mining channels.
///
/// Types implementing `JobStore` must support tracking and transitioning jobs through various
/// states (future, active, past, stale), and provide access to job collections and mappings.
///
///  All getter methods return owned/cloned values to allow implementations to store jobs behind
/// thread-safe types like `Arc<Mutex<T>>`.
pub trait JobStore<'a, JobIn, JobOut>
where
    JobIn: Job<'a>,
    JobOut: Job<'a>,
{
    /// Adds a future job associated with a template ID.
    /// Returns the new job's ID.
    fn add_future_job(&mut self, template_id: u64, job: JobIn) -> u32;

    /// Adds an active job, moving the previous active job (if any) to past jobs.
    fn add_active_job(&mut self, job: JobIn);

    /// Activates a future job given by template ID and header timestamp.
    /// Returns `true` if successful, `false` if not found.
    fn activate_future_job(&mut self, template_id: u64, prev_hash_header_timestamp: u32) -> bool;

    /// Marks all past jobs as stale, so that shares can be rejected with the appropriate error
    /// code
    fn mark_past_jobs_as_stale(&mut self);

    /// Returns the job ID for a future job from a template ID, if any.
    fn get_future_job_id_from_template_id(&self, template_id: u64) -> Option<u32>;

    /// Returns an owned copy of the currently active job, if any.
    fn get_active_job(&self) -> Option<JobOut>;

    /// Returns true if there are any future jobs, false otherwise.
    fn has_future_jobs(&self) -> bool;

    /// Returns a reference to a future job from its job ID, if any.
    fn peek_future_job(&self, job_id: u32) -> Option<&JobIn>;

    /// Removes an owned copy of a future job from its job ID, if any.
    fn remove_future_job(&mut self, job_id: u32) -> Option<EitherJob<'a>>;

    /// Returns true if there are any past jobs, false otherwise.
    fn has_past_jobs(&self) -> bool;

    /// Returns an owned copy of a past job from its job ID, if any.
    fn get_past_job(&self, job_id: u32) -> Option<JobOut>;

    /// Returns true if there are any stale jobs, false otherwise.
    fn has_stale_jobs(&self) -> bool;

    /// Returns an owned copy of a stale job from its job ID, if any.
    fn get_stale_job(&self, job_id: u32) -> Option<JobOut>;
}

/// Default implementation of [`JobStore`] for tracking mining job states in SV2 channels.
///
/// Maintains collections for future, active, past, and stale jobs, and tracks template-to-job ID
/// mappings for future job activation.
#[derive(Default, Debug)]
pub struct DefaultJobStore<'a> {
    future_template_to_job_id: HashMap<u64, u32>,
    // Future jobs are indexed with job_id (u32)
    future_jobs: HashMap<u32, EitherJob<'a>>,
    active_job: Option<EitherJob<'a>>,
    // Past jobs are indexed with job_id (u32)
    past_jobs: HashMap<u32, EitherJob<'a>>,
    // Stale jobs are indexed with job_id (u32)
    stale_jobs: HashMap<u32, EitherJob<'a>>,
    _phantom: std::marker::PhantomData<&'a EitherJob<'a>>,
}

impl<'a> DefaultJobStore<'a> {
    /// Creates a new empty job store.
    pub fn new() -> Self {
        Self::default()
    }
}

impl<'a> JobStore<'a, EitherJob<'a>, EitherJob<'a>> for DefaultJobStore<'a> {
    fn add_future_job(&mut self, template_id: u64, new_job: EitherJob<'a>) -> u32 {
        let new_job_id = new_job.get_job_id();
        self.future_jobs.insert(new_job_id, new_job);
        self.future_template_to_job_id
            .insert(template_id, new_job_id);
        new_job_id
    }

    fn add_active_job(&mut self, job: EitherJob<'a>) {
        // Move currently active job to past jobs (so it can be marked as stale)
        if let Some(active_job) = self.active_job.take() {
            self.past_jobs.insert(active_job.get_job_id(), active_job);
        }
        // Set the new active job
        self.active_job = Some(job);
    }

    fn activate_future_job(&mut self, template_id: u64, prev_hash_header_timestamp: u32) -> bool {
        let mut future_job =
            if let Some(job_id) = self.future_template_to_job_id.remove(&template_id) {
                if let Some(job) = self.future_jobs.remove(&job_id) {
                    job
                } else {
                    return false;
                }
            } else {
                return false;
            };

        // Move currently active job to past jobs (so it can be marked as stale)
        if let Some(active_job) = self.active_job.take() {
            self.past_jobs.insert(active_job.get_job_id(), active_job);
        }

        // Activate the future job
        future_job.activate(prev_hash_header_timestamp);
        self.active_job = Some(future_job);
        self.future_jobs.clear();
        self.future_template_to_job_id.clear();

        self.mark_past_jobs_as_stale();

        true
    }

    fn mark_past_jobs_as_stale(&mut self) {
        // Transfer past jobs to stale jobs collection and reset past jobs to empty
        self.stale_jobs = std::mem::take(&mut self.past_jobs);
    }

    fn get_future_job_id_from_template_id(&self, template_id: u64) -> Option<u32> {
        self.future_template_to_job_id.get(&template_id).cloned()
    }

    fn get_active_job(&self) -> Option<EitherJob<'a>> {
        self.active_job.clone()
    }

    fn has_future_jobs(&self) -> bool {
        !self.future_jobs.is_empty()
    }

    fn peek_future_job(&self, job_id: u32) -> Option<&EitherJob<'a>> {
        self.future_jobs.get(&job_id)
    }

    fn remove_future_job(&mut self, job_id: u32) -> Option<EitherJob<'a>> {
        self.future_jobs.remove(&job_id)
    }

    fn has_past_jobs(&self) -> bool {
        !self.past_jobs.is_empty()
    }

    fn get_past_job(&self, job_id: u32) -> Option<EitherJob<'a>> {
        self.past_jobs.get(&job_id).cloned()
    }

    fn has_stale_jobs(&self) -> bool {
        !self.stale_jobs.is_empty()
    }

    fn get_stale_job(&self, job_id: u32) -> Option<EitherJob<'a>> {
        self.stale_jobs.get(&job_id).cloned()
    }
}

#[cfg(test)]
mod test {
    use std::{collections::HashMap, sync::Arc};

    use binary_sv2::{Seq0255, Sv2Option, U256};
    use bitcoin::TxOut;

    use crate::server::jobs::{
        either_job::EitherJob, job_store::JobStore, Job, JobMessage, JobOrigin,
    };

    #[derive(Clone, Debug, PartialEq)]
    struct SharedJob<'a>(Arc<EitherJob<'a>>);
    impl<'a> Job<'a> for SharedJob<'a> {
        fn get_job_id(&self) -> u32 {
            self.0.get_job_id()
        }
        fn get_origin(&self) -> &JobOrigin<'a> {
            self.0.get_origin()
        }
        fn get_extranonce_prefix(&self) -> &Vec<u8> {
            self.0.get_extranonce_prefix()
        }
        fn get_coinbase_outputs(&self) -> &Vec<TxOut> {
            self.0.get_coinbase_outputs()
        }
        fn get_job_message(&self) -> &JobMessage<'a> {
            self.0.get_job_message()
        }
        fn get_min_ntime(&self) -> Sv2Option<'a, u32> {
            self.0.get_min_ntime()
        }
        fn get_version(&self) -> u32 {
            self.0.get_version()
        }
        fn version_rolling_allowed(&self) -> bool {
            self.0.version_rolling_allowed()
        }
        fn get_merkle_root(&self) -> Option<&U256<'a>> {
            self.0.get_merkle_root()
        }
        fn get_merkle_path(&self) -> &Seq0255<'a, U256<'a>> {
            self.0.get_merkle_path()
        }
        fn get_coinbase_tx_prefix_with_bip141(&self) -> Option<Vec<u8>> {
            self.0.get_coinbase_tx_prefix_with_bip141()
        }
        fn get_coinbase_tx_suffix_with_bip141(&self) -> Option<Vec<u8>> {
            self.0.get_coinbase_tx_suffix_with_bip141()
        }
        fn get_coinbase_tx_prefix_without_bip141(&self) -> Vec<u8> {
            self.0.get_coinbase_tx_prefix_without_bip141()
        }
        fn get_coinbase_tx_suffix_without_bip141(&self) -> Vec<u8> {
            self.0.get_coinbase_tx_suffix_without_bip141()
        }
        fn is_future(&self) -> bool {
            self.0.is_future()
        }
        fn activate(&mut self, min_ntime: u32) {
            Arc::get_mut(&mut self.0)
                .expect("Cannot activate a shared job while it is shared")
                .activate(min_ntime);
        }
    }

    /// Shared implementation of [`JobStore`] used for tracking mining job states in SV2 channels.
    /// This implementation is thread-safe and holds jobs behind `Arc` pointers.
    ///
    /// Maintains collections for future, active, past, and stale jobs, and tracks template-to-job ID
    /// mappings for future job activation.
    #[derive(Default, Debug)]
    pub struct SharedJobStore<'a> {
        future_template_to_job_id: HashMap<u64, u32>,
        // Future jobs are indexed with job_id (u32)
        future_jobs: HashMap<u32, EitherJob<'a>>,
        active_job: Option<SharedJob<'a>>,
        // Past jobs are indexed with job_id (u32)
        past_jobs: HashMap<u32, SharedJob<'a>>,
        // Stale jobs are indexed with job_id (u32)
        stale_jobs: HashMap<u32, SharedJob<'a>>,
        _phantom: std::marker::PhantomData<&'a EitherJob<'a>>,
    }

    impl<'a> SharedJobStore<'a> {
        /// Creates a new empty job store.
        pub fn new() -> Self {
            Self::default()
        }
    }

    impl<'a> JobStore<'a, EitherJob<'a>, SharedJob<'a>> for SharedJobStore<'a> {
        fn add_future_job(&mut self, template_id: u64, new_job: EitherJob<'a>) -> u32 {
            let new_job_id = new_job.get_job_id();
            self.future_jobs.insert(new_job_id, new_job);
            self.future_template_to_job_id
                .insert(template_id, new_job_id);
            new_job_id
        }

        fn add_active_job(&mut self, job: EitherJob<'a>) {
            // Move currently active job to past jobs (so it can be marked as stale)
            if let Some(active_job) = self.active_job.take() {
                self.past_jobs.insert(active_job.get_job_id(), active_job);
            }
            // Set the new active job
            self.active_job = Some(SharedJob(Arc::new(job)));
        }

        fn activate_future_job(
            &mut self,
            template_id: u64,
            prev_hash_header_timestamp: u32,
        ) -> bool {
            let mut future_job =
                if let Some(job_id) = self.future_template_to_job_id.remove(&template_id) {
                    if let Some(job) = self.future_jobs.remove(&job_id) {
                        job
                    } else {
                        return false;
                    }
                } else {
                    return false;
                };

            // Move currently active job to past jobs (so it can be marked as stale)
            if let Some(active_job) = self.active_job.take() {
                self.past_jobs.insert(active_job.get_job_id(), active_job);
            }

            // Activate the future job
            future_job.activate(prev_hash_header_timestamp);
            self.active_job = Some(SharedJob(Arc::new(future_job)));
            self.future_jobs.clear();
            self.future_template_to_job_id.clear();

            self.mark_past_jobs_as_stale();

            true
        }

        fn mark_past_jobs_as_stale(&mut self) {
            // Transfer past jobs to stale jobs collection and reset past jobs to empty
            self.stale_jobs = std::mem::take(&mut self.past_jobs);
        }

        fn get_future_job_id_from_template_id(&self, template_id: u64) -> Option<u32> {
            self.future_template_to_job_id.get(&template_id).cloned()
        }

        fn get_active_job(&self) -> Option<SharedJob<'a>> {
            self.active_job.clone()
        }

        fn has_future_jobs(&self) -> bool {
            !self.future_jobs.is_empty()
        }

        fn peek_future_job(&self, job_id: u32) -> Option<&EitherJob<'a>> {
            self.future_jobs.get(&job_id)
        }

        fn remove_future_job(&mut self, job_id: u32) -> Option<EitherJob<'a>> {
            self.future_jobs.remove(&job_id)
        }

        fn has_past_jobs(&self) -> bool {
            !self.past_jobs.is_empty()
        }

        fn get_past_job(&self, job_id: u32) -> Option<SharedJob<'a>> {
            self.past_jobs.get(&job_id).cloned()
        }

        fn has_stale_jobs(&self) -> bool {
            !self.stale_jobs.is_empty()
        }

        fn get_stale_job(&self, job_id: u32) -> Option<SharedJob<'a>> {
            self.stale_jobs.get(&job_id).cloned()
        }
    }
}
