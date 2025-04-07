use crate::utils::Id as IdFactory;
use std::sync::{Arc, RwLock};

#[derive(Clone, Debug)]
pub struct ChannelIdFactory {
    channel_id_factory: Arc<RwLock<IdFactory>>,
}

impl ChannelIdFactory {
    pub fn new() -> Self {
        Self {
            channel_id_factory: Arc::new(RwLock::new(IdFactory::new())),
        }
    }

    pub fn next(&self) -> Result<u32, ChannelIdFactoryError> {
        let mut channel_id_factory_guard = self
            .channel_id_factory
            .write()
            .map_err(|_| ChannelIdFactoryError::PoisonError)?;
        let channel_id = channel_id_factory_guard.next();
        Ok(channel_id)
    }
}

pub enum ChannelIdFactoryError {
    PoisonError,
}
