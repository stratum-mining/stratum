use crate::{
    channel_management::extended_channel::ExtendedChannel,
    mining_sv2::{OpenExtendedMiningChannel, SubmitSharesExtended, UpdateChannel},
    template_distribution_sv2::{NewTemplate, SetNewPrevHash as SetNewPrevHashTDP},
    parsers::Mining,
};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

pub struct ExtendedChannelFactory {
    extended_channels: Arc<RwLock<HashMap<u32, Arc<RwLock<ExtendedChannel>>>>>,
}

impl ExtendedChannelFactory {
    /// on successful execution, returns Ok(u32) with the id of the new standard channel
    async fn on_open_extended_mining_channel(
        &mut self,
        m: OpenExtendedMiningChannel<'static>,
    ) -> Result<u32, ExtendedChannelFactoryError> {
        todo!()
    }

    /// on successful execution, returns Ok(())
    async fn on_update_channel(
        &mut self,
        m: UpdateChannel<'static>,
    ) -> Result<(), ExtendedChannelFactoryError> {
        todo!()
    }

    /// returns Ok(false) if the shares were accepted, but the batch is still incomplete
    /// returns Ok(true) if the shares were accepted and the batch is complete
    /// returns Err(ExtendedChannelFactoryError::ShareSubmissionError(e)) if the share was rejected
    async fn on_submit_shares_extended(
        &mut self,
        m: SubmitSharesExtended<'static>,
    ) -> Result<bool, ExtendedChannelFactoryError> {
        todo!()
    }

    /// on successful execution, returns Ok(())
    async fn on_set_new_prev_hash_tdp(
        &mut self,
        m: SetNewPrevHashTDP<'static>,
    ) -> Result<(), ExtendedChannelFactoryError> {
        todo!()
    }

    /// on successful execution, returns Ok(())
    async fn on_new_template(
        &mut self,
        m: NewTemplate<'static>,
    ) -> Result<(), ExtendedChannelFactoryError> {
        todo!()
    }
}

pub enum ExtendedChannelFactoryError {
    ShareSubmissionError(String),
}
