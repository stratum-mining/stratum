use stratum_common::roles_logic_sv2::{
    codec_sv2::binary_sv2::Str0255,
    handlers_sv2::{
        HandleMiningMessagesFromClientAsync, HandlerError as Error, SupportedChannelTypes,
    },
    mining_sv2::*,
};
use tracing::info;

use crate::channel_manager::ChannelManager;

impl HandleMiningMessagesFromClientAsync for ChannelManager {
    fn get_channel_type_for_client(&self) -> SupportedChannelTypes {
        SupportedChannelTypes::Extended
    }
    fn is_work_selection_enabled_for_client(&self) -> bool {
        false
    }
    fn is_client_authorized(&self, user_identity: &Str0255) -> Result<bool, Error> {
        Ok(true)
    }
    async fn handle_close_channel(&mut self, msg: CloseChannel<'_>) -> Result<(), Error> {
        info!("Received handle_close_channel from Downstream");
        Ok(())
    }

    async fn handle_open_standard_mining_channel(
        &mut self,
        msg: OpenStandardMiningChannel<'_>,
    ) -> Result<(), Error> {
        info!("Received handle_open_standard_mining_channel from Downstream");
        Ok(())
    }

    async fn handle_open_extended_mining_channel(
        &mut self,
        msg: OpenExtendedMiningChannel<'_>,
    ) -> Result<(), Error> {
        info!("Received handle_open_extended_mining_channel from Downstream");
        Ok(())
    }

    async fn handle_update_channel(&mut self, msg: UpdateChannel<'_>) -> Result<(), Error> {
        info!("Received handle_update_channel from Downstream");
        Ok(())
    }

    async fn handle_submit_shares_standard(
        &mut self,
        msg: SubmitSharesStandard,
    ) -> Result<(), Error> {
        info!("Received handle_submit_shares_standard from Downstream");
        Ok(())
    }

    async fn handle_submit_shares_extended(
        &mut self,
        msg: SubmitSharesExtended<'_>,
    ) -> Result<(), Error> {
        info!("Received handle_submit_shares_extended from Downstream");
        Ok(())
    }

    async fn handle_set_custom_mining_job(
        &mut self,
        msg: SetCustomMiningJob<'_>,
    ) -> Result<(), Error> {
        info!("Received handle_set_custom_mining_job from Downstream");
        Ok(())
    }
}
