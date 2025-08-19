use stratum_common::roles_logic_sv2::{
    channels_sv2::client::extended::ExtendedChannel,
    handlers_sv2::{
        HandleMiningMessagesFromServerAsync, HandlerError as Error, SupportedChannelTypes,
    },
    mining_sv2::*,
};
use tracing::{error, info, warn};

use crate::channel_manager::ChannelManager;

impl HandleMiningMessagesFromServerAsync for ChannelManager {
    fn get_channel_type_for_server(&self) -> SupportedChannelTypes {
        SupportedChannelTypes::Extended
    }
    fn is_work_selection_enabled_for_server(&self) -> bool {
        true
    }
    async fn handle_open_standard_mining_channel_success(
        &mut self,
        msg: OpenStandardMiningChannelSuccess<'_>,
    ) -> Result<(), Error> {
        // Can't have standard channel with the pool under JD. fallback, if you
        // ever receive this from pool.
        info!("Received handle_open_standard_mining_channel_success from Pool");
        Ok(())
    }

    async fn handle_open_extended_mining_channel_success(
        &mut self,
        msg: OpenExtendedMiningChannelSuccess<'_>,
    ) -> Result<(), Error> {
        info!(
            "Received OpenExtendedMiningChannelSuccess with request id: {} and channel id: {}",
            msg.request_id, msg.channel_id
        );
        let (ident, hashrate, min_extranonce_size) =
            self.channel_manager_data.super_safe_lock(|data| {
                data.pending_channel
                    .remove(&0)
                    .unwrap_or_else(|| ("unknown".to_string(), 100000.0, 8_usize))
            });

        let prefix_len = msg.extranonce_prefix.clone().to_vec().len();
        let jdc_extranonce_len = std::cmp::min(
            (msg.extranonce_size as usize).saturating_sub((min_extranonce_size)),
            8,
        );
        let self_len = 0;
        let total_len = prefix_len + msg.extranonce_size as usize;
        let range_0 = 0..prefix_len;
        let range_1 = prefix_len..prefix_len + jdc_extranonce_len;
        let range_2 = prefix_len + jdc_extranonce_len..total_len;

        let extranonces = ExtendedExtranonce::from_upstream_extranonce(
            msg.extranonce_prefix.clone().into(),
            range_0,
            range_1,
            range_2,
        )
        .unwrap();
        // Come up with something better
        let extended_channel = ExtendedChannel::new(
            msg.channel_id,
            ident,
            msg.extranonce_prefix.to_vec(),
            msg.target.into(),
            hashrate,
            true,
            min_extranonce_size as u16,
        );
        self.channel_manager_data.super_safe_lock(|data| {
            data.extranonce_prefix_factory_extended = extranonces.clone();
            data.extranonce_prefix_factory_standard = extranonces;
            data.upstream_channel_id = msg.channel_id;
            data.upstream_channel = Some(extended_channel)
        });

        Ok(())
    }

    async fn handle_open_mining_channel_error(
        &mut self,
        msg: OpenMiningChannelError<'_>,
    ) -> Result<(), Error> {
        // In case of an error, just fallback
        info!("Received handle_open_mining_channel_error from Pool");
        Ok(())
    }

    async fn handle_update_channel_error(
        &mut self,
        msg: UpdateChannelError<'_>,
    ) -> Result<(), Error> {
        //Don't fallback, even in an update error. Send update messages, on new downstream
        // connection or disconnection.
        info!("Received handle_update_channel_error from Pool");
        Ok(())
    }

    async fn handle_close_channel(&mut self, msg: CloseChannel<'_>) -> Result<(), Error> {
        // Fallback
        info!("Received handle_close_channel from Pool");
        self.channel_manager_data.super_safe_lock(|data| {
            data.upstream_channel = None;
        });
        Ok(())
    }

    async fn handle_set_extranonce_prefix(
        &mut self,
        msg: SetExtranoncePrefix<'_>,
    ) -> Result<(), Error> {
        // Update the extranonce factory
        info!("Received handle_set_extranonce_prefix from Pool");
        Ok(())
    }

    async fn handle_submit_shares_success(
        &mut self,
        msg: SubmitSharesSuccess,
    ) -> Result<(), Error> {
        // Send shares to pool
        info!("Received submit_shares_success from Pool: {msg:?}");
        Ok(())
    }

    async fn handle_submit_shares_error(
        &mut self,
        msg: SubmitSharesError<'_>,
    ) -> Result<(), Error> {
        // log as an error, and don't fallback
        error!("Received handle_submit_shares_error from Pool: {msg:?}");
        
        Ok(())
    }

    async fn handle_new_mining_job(&mut self, msg: NewMiningJob<'_>) -> Result<(), Error> {
        warn!("Extended job received from upstream, proxy ignore it, and use the one declared by JOB DECLARATOR");
        Ok(())
    }

    async fn handle_new_extended_mining_job(
        &mut self,
        msg: NewExtendedMiningJob<'_>,
    ) -> Result<(), Error> {
        info!("Received handle_new_extended_mining_job from Pool");
        Ok(())
    }

    async fn handle_set_new_prev_hash(&mut self, msg: SetNewPrevHash<'_>) -> Result<(), Error> {
        warn!("SNPH received from upstream, proxy ignored it, and used the one declared by JDC");
        Ok(())
    }

    async fn handle_set_custom_mining_job_success(
        &mut self,
        msg: SetCustomMiningJobSuccess,
    ) -> Result<(), Error> {
        info!("Received handle_set_custom_mining_job_success from Pool");
        self.channel_manager_data.super_safe_lock(|data| {
            let value = data.last_declare_job_store.get(&msg.request_id).cloned();
            let last_declare_job = value.unwrap();
            data.template_id_to_upstream_job_id
                .insert(last_declare_job.template.template_id, msg.job_id as u64);
            data.job_id_to_template.insert(msg.job_id, last_declare_job);
        });
        Ok(())
    }

    async fn handle_set_custom_mining_job_error(
        &mut self,
        msg: SetCustomMiningJobError<'_>,
    ) -> Result<(), Error> {
        // Fallback
        info!("Received handle_set_custom_mining_job_error from Pool");
        Ok(())
    }

    async fn handle_set_target(&mut self, msg: SetTarget<'_>) -> Result<(), Error> {
        // Update the target and store in JDC
        info!("Received handle_set_target from Pool");
        self.channel_manager_data.super_safe_lock(|data| {
            if let Some(ref mut upstream) = data.upstream_channel {
                upstream.set_target(msg.maximum_target.clone().into());
                for (downstream_id, downstream) in data.downstream.iter_mut() {
                    downstream.downstream_data.super_safe_lock(|data| {
                        for (channel_id, standard_channel) in data.standard_channels.iter_mut() {
                            standard_channel.set_target(msg.maximum_target.clone().into());
                        }

                        for (channel_id, extended_channel) in data.extended_channels.iter_mut() {
                            extended_channel.set_target(msg.maximum_target.clone().into());
                        }
                    });
                }
            }
        });
        Ok(())
    }

    async fn handle_set_group_channel(&mut self, msg: SetGroupChannel<'_>) -> Result<(), Error> {
        // fallback
        info!("Received handle_set_group_channel from Pool");
        Ok(())
    }
}
