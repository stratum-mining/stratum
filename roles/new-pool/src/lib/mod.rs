use crate::config::PoolConfig;

pub mod config;
pub mod error;
pub mod template_receiver;
pub mod downstream;
pub mod channel_manager;

#[derive(Debug, Clone)]
pub struct PoolSv2 {
    config: PoolConfig,
}

impl PoolSv2 {
    pub fn new(config: PoolConfig) -> Self {
        PoolSv2 { config }
    }

    pub async fn start(&self) {}
}
