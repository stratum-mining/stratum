#![allow(warnings)]
use async_channel::unbounded;
use stratum_common::roles_logic_sv2::parsers_sv2::TemplateDistribution;

use crate::{config::PoolConfig, error::PoolResult};

pub mod channel_manager;
pub mod config;
pub mod downstream;
pub mod error;
pub mod status;
pub mod task_manager;
pub mod template_receiver;
pub mod utils;

#[derive(Debug, Clone)]
pub struct PoolSv2 {
    config: PoolConfig,
}

impl PoolSv2 {
    pub fn new(config: PoolConfig) -> Self {
        PoolSv2 { config }
    }

    pub async fn start(&self) -> PoolResult<()> {
        /// Define channels
        let (
            template_receiver_to_channel_manager_sender,
            template_receiver_to_channel_manager_receiver,
        ) = unbounded::<TemplateDistribution>();

        let (
            channel_manager_to_template_receiver_sender,
            channel_manager_to_template_receiver_receiver,
        ) = unbounded::<TemplateDistribution>();

        todo!()
    }
}
