#![allow(warnings)]
use crate::config::JobDeclaratorClientConfig;

mod channel_manager;
pub mod config;
mod downstream;
pub mod error;
mod job_declarator;
mod task_manager;
mod template_receiver;
mod upstream;

pub struct JobDeclaratorClient;

impl JobDeclaratorClient {
    pub fn new(config: JobDeclaratorClientConfig) -> Self {
        todo!()
    }

    pub async fn start(&self) {}
}
