use crate::config::PoolConfig;

pub mod config;


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