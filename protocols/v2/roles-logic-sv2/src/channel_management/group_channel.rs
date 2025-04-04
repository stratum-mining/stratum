use crate::{
    channel_management::standard_channel::StandardChannel, mining_sv2::NewExtendedMiningJob,
};
use std::collections::HashMap;
pub struct GroupChannel {
    channel_id: u32,
    channels: HashMap<u32, StandardChannel>,
    current_job: Option<NewExtendedMiningJob<'static>>,
    future_job: Option<NewExtendedMiningJob<'static>>,
    past_jobs: Vec<NewExtendedMiningJob<'static>>,
}

impl GroupChannel {
    async fn get_channels(&self) -> Vec<StandardChannel> {
        todo!()
    }
    async fn get_channel(&self, channel_id: u32) -> StandardChannel {
        todo!()
    }
    async fn add_channel(&mut self, channel: StandardChannel) {
        todo!()
    }
    async fn remove_channel(&mut self, channel: StandardChannel) {
        todo!()
    }
}
