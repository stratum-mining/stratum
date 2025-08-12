use stratum_common::roles_logic_sv2::{
    handlers_sv2::{
        HandleMiningMessagesFromClientAsync, HandlerError as Error, SupportedChannelTypes,
    },
    mining_sv2::*,
};
use tracing::info;

use crate::channel_manager::ChannelManager;

impl HandleMiningMessagesFromClientAsync for ChannelManager {
    fn get_channel_type_client(&self) -> SupportedChannelTypes {
        SupportedChannelTypes::Extended
    }
    fn is_work_selection_enabled_client(&self) -> bool {
        false
    }

    async fn handle_close_channel(&mut self, msg: CloseChannel<'_>) -> Result<(), Error> {
        info!("Received {msg:#?}");
        Ok(())
    }

    async fn handle_open_standard_mining_channel(
        &mut self,
        msg: OpenStandardMiningChannel<'_>,
    ) -> Result<(), Error> {
        info!("Received {msg:#?}");
        Ok(())
    }

    async fn handle_open_extended_mining_channel(
        &mut self,
        msg: OpenExtendedMiningChannel<'_>,
    ) -> Result<(), Error> {
        info!("Received {msg:#?}");
        Ok(())
    }

    async fn handle_update_channel(&mut self, msg: UpdateChannel<'_>) -> Result<(), Error> {
        info!("Received {msg:#?}");
        Ok(())
    }

    async fn handle_submit_shares_standard(
        &mut self,
        msg: SubmitSharesStandard,
    ) -> Result<(), Error> {
        info!("Received {msg:#?}");
        Ok(())
    }

    async fn handle_submit_shares_extended(
        &mut self,
        msg: SubmitSharesExtended<'_>,
    ) -> Result<(), Error> {
        info!("Received {msg:#?}");
        Ok(())
    }

    async fn handle_set_custom_mining_job(
        &mut self,
        msg: SetCustomMiningJob<'_>,
    ) -> Result<(), Error> {
        info!("Received {msg:#?}");
        Ok(())
    }
}
