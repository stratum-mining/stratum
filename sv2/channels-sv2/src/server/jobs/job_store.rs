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

use std::{collections::HashMap, fmt::Debug};

use super::Job;

/// Trait for job lifecycle management in mining channels.
///
/// Types implementing `JobStore` must support tracking and transitioning jobs through various
/// states (future, active, past, stale), and provide access to job collections and mappings.
pub trait JobStore<T: Job>: Send + Sync + Debug {
    /// Adds a future job associated with a template ID.
    /// Returns the new job's ID.
    fn add_future_job(&mut self, template_id: u64, job: T) -> u32;

    /// Adds an active job, moving the previous active job (if any) to past jobs.
    fn add_active_job(&mut self, job: T);

    /// Activates a future job given by template ID and header timestamp.
    /// Returns `true` if successful, `false` if not found.
    fn activate_future_job(&mut self, template_id: u64, prev_hash_header_timestamp: u32) -> bool;

    /// Marks all past jobs as stale, so that shares can be rejected with the appropriate error
    /// code
    fn mark_past_jobs_as_stale(&mut self);

    /// Returns the mapping from future template IDs to job IDs.
    fn get_future_template_to_job_id(&self) -> &HashMap<u64, u32>;

    /// Returns the currently active job, if any.
    fn get_active_job(&self) -> Option<&T>;

    /// Returns all future jobs, indexed by job ID.
    fn get_future_jobs(&self) -> &HashMap<u32, T>;

    /// Returns all past jobs (previously active jobs), indexed by job ID.
    fn get_past_jobs(&self) -> &HashMap<u32, T>;

    /// Returns all stale jobs (jobs from previous chain tip), indexed by job ID.
    fn get_stale_jobs(&self) -> &HashMap<u32, T>;
}

/// Default implementation of [`JobStore`] for tracking mining job states in SV2 channels.
///
/// Maintains collections for future, active, past, and stale jobs, and tracks template-to-job ID
/// mappings for future job activation.
#[derive(Debug)]
pub struct DefaultJobStore<T: Job + Clone> {
    future_template_to_job_id: HashMap<u64, u32>,
    // Future jobs are indexed with job_id (u32)
    future_jobs: HashMap<u32, T>,
    active_job: Option<T>,
    // Past jobs are indexed with job_id (u32)
    past_jobs: HashMap<u32, T>,
    // Stale jobs are indexed with job_id (u32)
    stale_jobs: HashMap<u32, T>,
}

impl<T: Job + Clone> DefaultJobStore<T> {
    /// Creates a new empty job store.
    pub fn new() -> Self {
        Self {
            future_template_to_job_id: HashMap::new(),
            future_jobs: HashMap::new(),
            active_job: None,
            past_jobs: HashMap::new(),
            stale_jobs: HashMap::new(),
        }
    }
}

impl<T: Job + Clone> Default for DefaultJobStore<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Job + Clone + Debug> JobStore<T> for DefaultJobStore<T> {
    fn add_future_job(&mut self, template_id: u64, new_job: T) -> u32 {
        let new_job_id = new_job.get_job_id();
        self.future_jobs.insert(new_job_id, new_job);
        self.future_template_to_job_id
            .insert(template_id, new_job_id);
        new_job_id
    }

    fn add_active_job(&mut self, job: T) {
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
        // Mark all past jobs as stale, so that shares can be rejected with the appropriate error
        // code
        self.stale_jobs = self.past_jobs.clone();

        // Clear past jobs, as we're no longer going to validate shares for them
        self.past_jobs.clear();
    }

    fn get_future_template_to_job_id(&self) -> &HashMap<u64, u32> {
        &self.future_template_to_job_id
    }

    fn get_active_job(&self) -> Option<&T> {
        self.active_job.as_ref()
    }

    fn get_future_jobs(&self) -> &HashMap<u32, T> {
        &self.future_jobs
    }

    fn get_past_jobs(&self) -> &HashMap<u32, T> {
        &self.past_jobs
    }

    fn get_stale_jobs(&self) -> &HashMap<u32, T> {
        &self.stale_jobs
    }
}
