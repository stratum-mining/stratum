use stratum_common::roles_logic_sv2::{
    handlers_sv2::{
        HandleMiningMessagesFromServerAsync, HandlerError as Error, SupportedChannelTypes,
    },
    mining_sv2::*,
};

use crate::channel_manager::ChannelManager;

impl HandleMiningMessagesFromServerAsync for ChannelManager {
    fn get_channel_type(&self) -> SupportedChannelTypes {
        todo!()
    }
    fn is_work_selection_enabled(&self) -> bool {
        todo!()
    }
    async fn handle_open_standard_mining_channel_success(
        &mut self,
        msg: OpenStandardMiningChannelSuccess<'_>,
    ) -> Result<(), Error> {
        todo!()
    }

    async fn handle_open_extended_mining_channel_success(
        &mut self,
        msg: OpenExtendedMiningChannelSuccess<'_>,
    ) -> Result<(), Error> {
        todo!()
    }

    async fn handle_open_mining_channel_error(
        &mut self,
        msg: OpenMiningChannelError<'_>,
    ) -> Result<(), Error> {
        todo!()
    }

    async fn handle_update_channel_error(
        &mut self,
        msg: UpdateChannelError<'_>,
    ) -> Result<(), Error> {
        todo!()
    }

    async fn handle_close_channel(&mut self, msg: CloseChannel<'_>) -> Result<(), Error> {
        todo!()
    }

    async fn handle_set_extranonce_prefix(
        &mut self,
        msg: SetExtranoncePrefix<'_>,
    ) -> Result<(), Error> {
        todo!()
    }

    async fn handle_submit_shares_success(
        &mut self,
        msg: SubmitSharesSuccess,
    ) -> Result<(), Error> {
        todo!()
    }

    async fn handle_submit_shares_error(
        &mut self,
        msg: SubmitSharesError<'_>,
    ) -> Result<(), Error> {
        todo!()
    }

    async fn handle_new_mining_job(&mut self, msg: NewMiningJob<'_>) -> Result<(), Error> {
        todo!()
    }

    async fn handle_new_extended_mining_job(
        &mut self,
        msg: NewExtendedMiningJob<'_>,
    ) -> Result<(), Error> {
        todo!()
    }

    async fn handle_set_new_prev_hash(&mut self, msg: SetNewPrevHash<'_>) -> Result<(), Error> {
        todo!()
    }

    async fn handle_set_custom_mining_job_success(
        &mut self,
        msg: SetCustomMiningJobSuccess,
    ) -> Result<(), Error> {
        todo!()
    }

    async fn handle_set_custom_mining_job_error(
        &mut self,
        msg: SetCustomMiningJobError<'_>,
    ) -> Result<(), Error> {
        todo!()
    }

    async fn handle_set_target(&mut self, msg: SetTarget<'_>) -> Result<(), Error> {
        todo!()
    }

    async fn handle_set_group_channel(&mut self, msg: SetGroupChannel<'_>) -> Result<(), Error> {
        todo!()
    }
}
