use stratum_common::roles_logic_sv2::{
    self, Vardiff, VardiffState,
    bitcoin::{Amount, TxOut, consensus::Decodable},
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
    channel_manager::{ChannelManager, ChannelManagerChannel, RouteMessageTo},
    error::PoolError,
    utils::{StdFrame, deserialize_coinbase_outputs},
};

impl HandleMiningMessagesFromClientAsync for ChannelManager {
    type Error = PoolError;

    fn get_channel_type_for_client(&self) -> SupportedChannelTypes {
        SupportedChannelTypes::GroupAndExtended
    }

    fn is_work_selection_enabled_for_client(&self) -> bool {
        true
    }

    fn is_client_authorized(&self, _user_identity: &Str0255) -> Result<bool, Self::Error> {
        Ok(true)
    }

    async fn handle_close_channel(&mut self, msg: CloseChannel<'_>) -> Result<(), Self::Error> {
        info!("Received Close Channel: {msg}");
        self.channel_manager_data
            .super_safe_lock(|channel_manager_data| {
                let Some(downstream_id) = channel_manager_data
                    .channel_id_to_downstream_id
                    .remove(&msg.channel_id)
                else {
                    return Err(PoolError::DownstreamNotFoundWithChannelId(msg.channel_id));
                };

                let Some(downstream) = channel_manager_data.downstream.get_mut(&downstream_id)
                else {
                    return Err(PoolError::DownstreamNotFound(downstream_id));
                };

                downstream
                    .downstream_data
                    .super_safe_lock(|downstream_data| {
                        downstream_data.standard_channels.remove(&msg.channel_id);
                        downstream_data.extended_channels.remove(&msg.channel_id);
                    });
                Ok(())
            })
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
        info!("Received: {}", msg);

        // this is a naive implementation, but ideally we should check the SetCustomMiningJob
        // message parameters, especially:
        // - the mining_job_token
        // - the amount of the pool payout output
        let custom_job_coinbase_outputs = Vec::<TxOut>::consensus_decode(
            &mut msg.coinbase_tx_outputs.inner_as_ref().to_vec().as_slice(),
        )?;

        let message: RouteMessageTo =
            self.channel_manager_data
                .super_safe_lock(|channel_manager_data| {
                    let Some(downstream_id) = channel_manager_data
                        .channel_id_to_downstream_id
                        .get(&msg.channel_id)
                    else {
                        return Err(PoolError::DownstreamNotFound(msg.channel_id));
                    };

                    // check that the script_pubkey from self.coinbase_reward_script
                    // is present in the custom job coinbase outputs
                    let missing_script = !custom_job_coinbase_outputs.iter().any(|pool_output| {
                        *pool_output.script_pubkey == *self.coinbase_reward_script.script_pubkey()
                    });

                    if missing_script {
                        error!("SetCustomMiningJobError: pool-payout-script-missing");

                        let error = SetCustomMiningJobError {
                            request_id: msg.request_id,
                            channel_id: msg.channel_id,
                            error_code: "pool-payout-script-missing"
                                .to_string()
                                .try_into()
                                .expect("error code must be valid string"),
                        };

                        return Ok((*downstream_id, Mining::SetCustomMiningJobError(error)).into());
                    }

                    let Some(downstream) = channel_manager_data.downstream.get_mut(downstream_id)
                    else {
                        return Err(PoolError::DownstreamNotFound(*downstream_id));
                    };

                    downstream
                        .downstream_data
                        .super_safe_lock(|downstream_data| {
                            let Some(extended_channel) =
                                downstream_data.extended_channels.get_mut(&msg.channel_id)
                            else {
                                error!("SetCustomMiningJobError: invalid-channel-id");
                                let error = SetCustomMiningJobError {
                                    request_id: msg.request_id,
                                    channel_id: msg.channel_id,
                                    error_code: "invalid-channel-id"
                                        .to_string()
                                        .try_into()
                                        .expect("error code must be valid string"),
                                };
                                return Ok((
                                    *downstream_id,
                                    Mining::SetCustomMiningJobError(error),
                                )
                                    .into());
                            };

                            let job_id = extended_channel
                                .on_set_custom_mining_job(msg.clone().into_static())
                                .map_err(|_| PoolError::DownstreamIdNotFound)?;

                            let success = SetCustomMiningJobSuccess {
                                channel_id: msg.channel_id,
                                request_id: msg.request_id,
                                job_id,
                            };
                            return Ok((
                                *downstream_id,
                                Mining::SetCustomMiningJobSuccess(success),
                            )
                                .into());
                        })
                })?;

        message.forward(&self.channel_manager_channel).await;
        Ok(())
    }
}
