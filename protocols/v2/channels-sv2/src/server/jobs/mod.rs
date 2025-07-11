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

pub trait Job: Send + Sync {
    fn get_job_id(&self) -> u32;
    fn activate(&mut self, prev_hash_header_timestamp: u32);
}
