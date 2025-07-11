use std::{collections::HashMap, fmt::Debug};

use super::Job;

pub trait JobStore<T: Job>: Send + Sync + Debug {
    fn add_future_job(&mut self, template_id: u64, job: T) -> u32;
    fn add_active_job(&mut self, job: T);
    fn activate_future_job(&mut self, template_id: u64, prev_hash_header_timestamp: u32) -> bool;
    fn set_active_job(&mut self, job: T);
    fn get_future_template_to_job_id(&self) -> &HashMap<u64, u32>;
    fn get_active_job(&self) -> Option<&T>;
    fn get_future_jobs(&self) -> &HashMap<u32, T>;
    fn get_past_jobs(&self) -> &HashMap<u32, T>;
    fn get_stale_jobs(&self) -> &HashMap<u32, T>;
}

#[derive(Debug)]
pub struct DefaultJobStore<T: Job + Clone> {
    future_template_to_job_id: HashMap<u64, u32>,
    // future jobs are indexed with job_id (u32)
    future_jobs: HashMap<u32, T>,
    active_job: Option<T>,
    // past jobs are indexed with job_id (u32)
    past_jobs: HashMap<u32, T>,
    // stale jobs are indexed with job_id (u32)
    stale_jobs: HashMap<u32, T>,
}

impl<T: Job + Clone> DefaultJobStore<T> {
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
        // move currently active job to past jobs (so it can be marked as stale)
        if let Some(active_job) = self.active_job.take() {
            self.past_jobs.insert(active_job.get_job_id(), active_job);
        }
        // set the new active job
        self.active_job = Some(job);
    }

    fn set_active_job(&mut self, job: T) {
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

        // move currently active job to past jobs (so it can be marked as stale)
        if let Some(active_job) = self.active_job.take() {
            self.past_jobs.insert(active_job.get_job_id(), active_job);
        }

        // activate the future job
        future_job.activate(prev_hash_header_timestamp);
        self.active_job = Some(future_job);
        self.future_jobs.clear();
        self.future_template_to_job_id.clear();
        // mark all past jobs as stale, so that shares can be rejected with the appropriate error
        // code
        self.stale_jobs = self.past_jobs.clone();

        // clear past jobs, as we're no longer going to validate shares for them
        self.past_jobs.clear();
        true
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
