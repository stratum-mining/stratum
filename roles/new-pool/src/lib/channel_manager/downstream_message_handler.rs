use stratum_common::roles_logic_sv2::{
    self, Vardiff, VardiffState,
    bitcoin::Amount,
    channels_sv2::{
        client,
        server::{
            error::{ExtendedChannelError, StandardChannelError},
            extended::ExtendedChannel,
            group::GroupChannel,
            jobs::job_store::DefaultJobStore,
            share_accounting::{ShareValidationError, ShareValidationResult},
            standard::StandardChannel,
        },
    },
    codec_sv2::binary_sv2::Str0255,
    handlers_sv2::{HandleMiningMessagesFromClientAsync, SupportedChannelTypes},
    job_declaration_sv2::PushSolution,
    mining_sv2::*,
    parsers_sv2::{AnyMessage, JobDeclaration, Mining, TemplateDistribution},
    template_distribution_sv2::SubmitSolution,
};
use tracing::{debug, error, info, warn};

use crate::{
    channel_manager::{ChannelManager, ChannelManagerChannel},
    error::PoolError,
    utils::{StdFrame, deserialize_coinbase_outputs},
};

impl HandleMiningMessagesFromClientAsync for ChannelManager {
    type Error = PoolError;

    fn get_channel_type_for_client(&self) -> SupportedChannelTypes {
        SupportedChannelTypes::GroupAndExtended
    }
    fn is_work_selection_enabled_for_client(&self) -> bool {
        false
    }
    fn is_client_authorized(&self, _user_identity: &Str0255) -> Result<bool, Self::Error> {
        Ok(true)
    }

    async fn handle_close_channel(&mut self, msg: CloseChannel<'_>) -> Result<(), Self::Error> {
        info!("Received: {}", msg);
        Ok(())
    }

    async fn handle_open_standard_mining_channel(
        &mut self,
        msg: OpenStandardMiningChannel<'_>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn handle_open_extended_mining_channel(
        &mut self,
        msg: OpenExtendedMiningChannel<'_>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn handle_update_channel(&mut self, msg: UpdateChannel<'_>) -> Result<(), Self::Error> {
        info!("Received: {}", msg);
        Ok(())
    }

    async fn handle_submit_shares_standard(
        &mut self,
        msg: SubmitSharesStandard,
    ) -> Result<(), Self::Error> {
        info!("Received SubmitSharesStandard");
        Ok(())
    }

    async fn handle_submit_shares_extended(
        &mut self,
        msg: SubmitSharesExtended<'_>,
    ) -> Result<(), Self::Error> {
        info!("Received SubmitSharesExtended");
        Ok(())
    }

    async fn handle_set_custom_mining_job(
        &mut self,
        msg: SetCustomMiningJob<'_>,
    ) -> Result<(), Self::Error> {
        warn!("Received: {}", msg);
        Ok(())
    }
}
