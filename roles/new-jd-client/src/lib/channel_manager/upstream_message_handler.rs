use stratum_common::roles_logic_sv2::{
    handlers_sv2::{
        HandleMiningMessagesFromServerAsync, HandlerError as Error, SupportedChannelTypes,
    },
    mining_sv2::*,
};
use tracing::{info, warn};

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

        let prefix_len = msg.extranonce_prefix.to_vec().len();
        let self_len = 0;
        let total_len = prefix_len + msg.extranonce_size as usize;
        let range_0 = 0..prefix_len;
        let range_1 = prefix_len..prefix_len + self_len;
        let range_2 = prefix_len + self_len..total_len;

        let extranonces = ExtendedExtranonce::new(range_0, range_1, range_2, None).unwrap();
        self.channel_manager_data.super_safe_lock(|data| {
            data.extranonce_prefix_factory_extended = extranonces;
            data.upstream_channel_id = msg.channel_id;
        });

        Ok(())
    }

    async fn handle_open_mining_channel_error(
        &mut self,
        msg: OpenMiningChannelError<'_>,
    ) -> Result<(), Error> {
        info!("Received handle_open_mining_channel_error from Pool");
        Ok(())
    }

    async fn handle_update_channel_error(
        &mut self,
        msg: UpdateChannelError<'_>,
    ) -> Result<(), Error> {
        info!("Received handle_update_channel_error from Pool");
        Ok(())
    }

    async fn handle_close_channel(&mut self, msg: CloseChannel<'_>) -> Result<(), Error> {
        info!("Received handle_close_channel from Pool");
        Ok(())
    }

    async fn handle_set_extranonce_prefix(
        &mut self,
        msg: SetExtranoncePrefix<'_>,
    ) -> Result<(), Error> {
        info!("Received handle_set_extranonce_prefix from Pool");
        Ok(())
    }

    async fn handle_submit_shares_success(
        &mut self,
        msg: SubmitSharesSuccess,
    ) -> Result<(), Error> {
        info!("Received handle_submit_shares_success from Pool");
        Ok(())
    }

    async fn handle_submit_shares_error(
        &mut self,
        msg: SubmitSharesError<'_>,
    ) -> Result<(), Error> {
        info!("Received handle_submit_shares_error from Pool");
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
            data.job_id_to_template.insert(msg.job_id, last_declare_job);
        });
        Ok(())
    }

    async fn handle_set_custom_mining_job_error(
        &mut self,
        msg: SetCustomMiningJobError<'_>,
    ) -> Result<(), Error> {
        info!("Received handle_set_custom_mining_job_error from Pool");
        Ok(())
    }

    async fn handle_set_target(&mut self, msg: SetTarget<'_>) -> Result<(), Error> {
        info!("Received handle_set_target from Pool");
        Ok(())
    }

    async fn handle_set_group_channel(&mut self, msg: SetGroupChannel<'_>) -> Result<(), Error> {
        info!("Received handle_set_group_channel from Pool");
        Ok(())
    }
}
