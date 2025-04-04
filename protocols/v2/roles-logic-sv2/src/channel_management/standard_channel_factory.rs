use crate::{
    channel_management::{group_channel::GroupChannel, standard_channel::StandardChannel},
    mining_sv2::{OpenStandardMiningChannel, SubmitSharesStandard, UpdateChannel},
    template_distribution_sv2::{NewTemplate, SetNewPrevHash as SetNewPrevHashTDP},
    parsers::Mining,
};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};
pub struct StandardChannelFactory {
    group_channels: Arc<RwLock<HashMap<u32, Arc<RwLock<GroupChannel>>>>>,
    standard_channels: Arc<RwLock<HashMap<u32, Arc<RwLock<StandardChannel>>>>>,
}

impl StandardChannelFactory {
    /// on successful execution, returns Ok(u32) with the id of the new standard channel
    pub async fn on_open_standard_mining_channel(
        &mut self,
        m: OpenStandardMiningChannel<'static>,
    ) -> Result<u32, StandardChannelFactoryError> {
        todo!()
    }

    /// on successful execution, returns Ok(())
    pub async fn on_update_channel(
        &mut self,
        m: UpdateChannel<'static>,
    ) -> Result<(), StandardChannelFactoryError> {
        todo!()
    }

    /// returns Ok(false) if the shares were accepted, but the batch is still incomplete
    /// returns Ok(true) if the shares were accepted and the batch is complete
    /// returns Err(StandardChannelFactoryError::ShareSubmissionError(e)) if the share was rejected
    pub async fn on_submit_shares_standard(
        &mut self,
        m: SubmitSharesStandard,
    ) -> Result<bool, StandardChannelFactoryError> {
        todo!()
    }

    /// on successful execution, returns Ok(())
    pub async fn on_set_new_prev_hash_tdp(
        &mut self,
        m: SetNewPrevHashTDP<'static>,
    ) -> Result<(), StandardChannelFactoryError> {
        todo!()
    }

    /// on successful execution, returns Ok(())
    pub async fn on_new_template(
        &mut self,
        m: NewTemplate<'static>,
    ) -> Result<(), StandardChannelFactoryError> {
        todo!()
    }
}

#[derive(Debug)]
pub enum StandardChannelFactoryError {
    ShareSubmissionError(String),
}
