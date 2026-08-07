//! Internal job storage and lifecycle management for server-side SV2 channels.
//!
//! ## Responsibilities
//!
//! - **Job Storage**: Manages collections of jobs indexed by job ID and template ID.
//! - **Job Activation**: Handles transitions between future, active, past, and stale jobs.
//! - **Template Mapping**: Tracks mappings from template IDs to job IDs for future jobs.
//! - **Lifecycle Management**: Ensures correct state transitions when activating jobs or updating
//!   chain tips.
//! - **Retired Extranonce Prefixes**: Holds on to extranonce prefixes that were rotated out of the
//!   channel while jobs created under them can still accept shares, so that their allocator slots
//!   are not handed to another channel too early.
use std::collections::HashMap;

use super::Job;
use crate::extranonce_manager::ExtranoncePrefix;

/// Internal implementation for tracking mining job states in SV2 server channels.
///
/// Maintains collections for future, active, past, and stale jobs, and tracks template-to-job ID
/// mappings for future job activation.
#[derive(Debug)]
pub(crate) struct JobStore<T: Job> {
    future_template_to_job_id: HashMap<u64, u32>,
    // Future jobs are indexed with job_id (u32)
    future_jobs: HashMap<u32, T>,
    active_job: Option<T>,
    // Past jobs are indexed with job_id (u32)
    past_jobs: HashMap<u32, T>,
    // Stale jobs are indexed with job_id (u32)
    stale_jobs: HashMap<u32, T>,
    // Extranonce prefixes rotated out of the channel that are still referenced by at least one job
    // that can accept shares. Holding the object here keeps its allocator slot reserved; dropping
    // it releases the slot.
    retired_extranonce_prefixes: Vec<ExtranoncePrefix>,
}

impl<T: Job> JobStore<T> {
    /// Creates a new empty job store.
    pub fn new() -> Self {
        Self {
            future_template_to_job_id: HashMap::new(),
            future_jobs: HashMap::new(),
            active_job: None,
            past_jobs: HashMap::new(),
            stale_jobs: HashMap::new(),
            retired_extranonce_prefixes: Vec::new(),
        }
    }
}

impl<T: Job> Default for JobStore<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Job> JobStore<T> {
    /// Adds a future job associated with a template ID.
    ///
    /// If the template ID was already mapped to a future job, that job is dropped, since it could
    /// never be activated again (activation resolves jobs through this mapping).
    ///
    /// Returns the new job's ID.
    pub fn add_future_job(&mut self, template_id: u64, new_job: T) -> u32 {
        let new_job_id = new_job.get_job_id();
        if let Some(old_job_id) = self
            .future_template_to_job_id
            .insert(template_id, new_job_id)
        {
            self.future_jobs.remove(&old_job_id);
        }
        self.future_jobs.insert(new_job_id, new_job);
        new_job_id
    }

    /// Adds an active job, moving the previous active job (if any) to past jobs.
    pub fn add_active_job(&mut self, job: T) {
        // Move currently active job to past jobs (so it can be marked as stale)
        if let Some(active_job) = self.active_job.take() {
            self.past_jobs.insert(active_job.get_job_id(), active_job);
        }
        // Set the new active job
        self.active_job = Some(job);
    }

    /// Activates a future job given by template ID and header timestamp.
    /// Returns `true` if successful, `false` if not found.
    pub fn activate_future_job(
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
        self.active_job = Some(future_job);
        self.future_jobs.clear();
        self.future_template_to_job_id.clear();

        self.mark_past_jobs_as_stale();

        true
    }

    /// Moves the active job (if any) into past jobs.
    pub fn deactivate_job(&mut self) {
        if let Some(active_job) = self.active_job.take() {
            self.past_jobs.insert(active_job.get_job_id(), active_job);
        }
    }

    /// Marks all past jobs as stale so shares can be rejected with the proper error code.
    pub fn mark_past_jobs_as_stale(&mut self) {
        // Transfer past jobs to stale jobs collection and reset past jobs to empty
        self.stale_jobs = std::mem::take(&mut self.past_jobs);
        // jobs that just went stale can no longer accept shares, so any retired extranonce prefix
        // they were the last reference to is now releasable
        self.prune_retired_extranonce_prefixes();
    }

    /// Takes ownership of an extranonce prefix that is no longer the channel's current one,
    /// releasing it only once no job created under it can accept shares anymore.
    ///
    /// Jobs only carry a copy of the extranonce prefix bytes they were created under, and stay
    /// valid across a prefix rotation. Dropping the prefix object right away would return its
    /// slot to the allocator while those jobs still validate shares under those bytes, allowing
    /// the same extranonce space to be handed to a second live channel.
    pub fn retire_extranonce_prefix(&mut self, extranonce_prefix: ExtranoncePrefix) {
        if self.is_extranonce_prefix_in_use(extranonce_prefix.as_bytes()) {
            self.retired_extranonce_prefixes.push(extranonce_prefix);
        }
        // otherwise it drops here, releasing its slot right away
    }

    /// Drops every retired extranonce prefix that no future, active or past job still references.
    /// Dropping releases the prefix's slot back to its allocator.
    ///
    /// Stale jobs are deliberately not consulted: shares against them are rejected as stale, so
    /// they can no longer be credited under the old prefix bytes.
    fn prune_retired_extranonce_prefixes(&mut self) {
        // bind the job collections separately, so that the closure borrows them instead of
        // borrowing all of `self` (which `retain` needs mutably)
        let future_jobs = &self.future_jobs;
        let active_job = &self.active_job;
        let past_jobs = &self.past_jobs;

        self.retired_extranonce_prefixes.retain(|prefix| {
            future_jobs
                .values()
                .chain(active_job.iter())
                .chain(past_jobs.values())
                .any(|job| job.get_extranonce_prefix() == prefix.as_bytes())
        });
    }

    /// Whether any future, active or past job was created under `extranonce_prefix`.
    fn is_extranonce_prefix_in_use(&self, extranonce_prefix: &[u8]) -> bool {
        self.future_jobs
            .values()
            .chain(self.active_job.iter())
            .chain(self.past_jobs.values())
            .any(|job| job.get_extranonce_prefix() == extranonce_prefix)
    }

    /// Returns the job ID for a future job from a template ID, if any.
    pub fn get_future_job_id_from_template_id(&self, template_id: u64) -> Option<u32> {
        self.future_template_to_job_id.get(&template_id).cloned()
    }

    /// Returns a reference to the currently active job, if any.
    pub fn get_active_job(&self) -> Option<&T> {
        self.active_job.as_ref()
    }

    /// Returns true if there are any future jobs, false otherwise.
    pub fn has_future_jobs(&self) -> bool {
        !self.future_jobs.is_empty()
    }

    /// Returns a reference to a future job from its job ID, if any.
    pub fn get_future_job(&self, job_id: u32) -> Option<&T> {
        self.future_jobs.get(&job_id)
    }

    /// Returns a reference to a past job from its job ID, if any.
    pub fn get_past_job(&self, job_id: u32) -> Option<&T> {
        self.past_jobs.get(&job_id)
    }

    /// Returns a reference to a stale job from its job ID, if any.
    pub fn get_stale_job(&self, job_id: u32) -> Option<&T> {
        self.stale_jobs.get(&job_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyJob {
        job_id: u32,
    }

    impl Job for DummyJob {
        fn get_job_id(&self) -> u32 {
            self.job_id
        }

        fn get_extranonce_prefix(&self) -> &[u8] {
            &[]
        }

        fn activate(&mut self, _prev_hash_header_timestamp: u32) {}
    }

    #[test]
    fn reused_template_id_evicts_superseded_future_job() {
        let mut store = JobStore::new();

        let old_job_id = store.add_future_job(1, DummyJob { job_id: 10 });
        let new_job_id = store.add_future_job(1, DummyJob { job_id: 11 });

        // the superseded job could never be activated again, so it must not be retained
        assert!(store.get_future_job(old_job_id).is_none());
        assert!(store.get_future_job(new_job_id).is_some());
        assert_eq!(
            store.get_future_job_id_from_template_id(1),
            Some(new_job_id)
        );
    }
}
