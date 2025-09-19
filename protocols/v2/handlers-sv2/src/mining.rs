use crate::error::HandlerErrorType;
use binary_sv2::Str0255;
use mining_sv2::{
    CloseChannel, NewExtendedMiningJob, NewMiningJob, OpenExtendedMiningChannel,
    OpenExtendedMiningChannelSuccess, OpenMiningChannelError, OpenStandardMiningChannel,
    OpenStandardMiningChannelSuccess, SetCustomMiningJob, SetCustomMiningJobError,
    SetCustomMiningJobSuccess, SetExtranoncePrefix, SetGroupChannel, SetNewPrevHash, SetTarget,
    SubmitSharesError, SubmitSharesExtended, SubmitSharesStandard, SubmitSharesSuccess,
    UpdateChannel, UpdateChannelError,
};
use parsers_sv2::Mining;
use std::convert::TryInto;

use mining_sv2::*;

#[derive(PartialEq, Eq)]
pub enum SupportedChannelTypes {
    Standard,
    Extended,
    Group,
    GroupAndExtended,
}

pub trait HandleMiningMessagesFromServerSync {
    type Error: HandlerErrorType;

    fn get_channel_type_for_server(&self) -> SupportedChannelTypes;
    fn is_work_selection_enabled_for_server(&self) -> bool;

    fn handle_mining_message_frame_from_server(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        let parsed: Mining = (message_type, payload)
            .try_into()
            .map_err(Self::Error::parse_error)?;
        self.handle_mining_message_from_server(parsed)
    }

    fn handle_mining_message_from_server(&mut self, message: Mining) -> Result<(), Self::Error> {
        let (channel_type, work_selection) = (
            self.get_channel_type_for_server(),
            self.is_work_selection_enabled_for_server(),
        );

        use Mining::*;
        match message {
            OpenStandardMiningChannelSuccess(m) => match channel_type {
                SupportedChannelTypes::Standard
                | SupportedChannelTypes::Group
                | SupportedChannelTypes::GroupAndExtended => {
                    self.handle_open_standard_mining_channel_success(m)
                }
                _ => Err(Self::Error::unexpected_message(
                    MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS,
                )),
            },

            OpenExtendedMiningChannelSuccess(m) => match channel_type {
                SupportedChannelTypes::Extended | SupportedChannelTypes::GroupAndExtended => {
                    self.handle_open_extended_mining_channel_success(m)
                }
                _ => Err(Self::Error::unexpected_message(
                    MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCESS,
                )),
            },

            OpenMiningChannelError(m) => self.handle_open_mining_channel_error(m),
            UpdateChannelError(m) => self.handle_update_channel_error(m),
            CloseChannel(m) => self.handle_close_channel(m),
            SetExtranoncePrefix(m) => self.handle_set_extranonce_prefix(m),
            SubmitSharesSuccess(m) => self.handle_submit_shares_success(m),
            SubmitSharesError(m) => self.handle_submit_shares_error(m),

            NewMiningJob(m) => match channel_type {
                SupportedChannelTypes::Standard => self.handle_new_mining_job(m),
                _ => Err(Self::Error::unexpected_message(MESSAGE_TYPE_NEW_MINING_JOB)),
            },

            NewExtendedMiningJob(m) => match channel_type {
                SupportedChannelTypes::Extended
                | SupportedChannelTypes::Group
                | SupportedChannelTypes::GroupAndExtended => self.handle_new_extended_mining_job(m),
                _ => Err(Self::Error::unexpected_message(
                    MESSAGE_TYPE_NEW_EXTENDED_MINING_JOB,
                )),
            },

            SetNewPrevHash(m) => self.handle_set_new_prev_hash(m),

            SetCustomMiningJobSuccess(m) => match (channel_type, work_selection) {
                (SupportedChannelTypes::Extended, true)
                | (SupportedChannelTypes::GroupAndExtended, true) => {
                    self.handle_set_custom_mining_job_success(m)
                }
                _ => Err(Self::Error::unexpected_message(
                    MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_SUCCESS,
                )),
            },

            SetCustomMiningJobError(m) => match (channel_type, work_selection) {
                (SupportedChannelTypes::Extended, true)
                | (SupportedChannelTypes::Group, true)
                | (SupportedChannelTypes::GroupAndExtended, true) => {
                    self.handle_set_custom_mining_job_error(m)
                }
                _ => Err(Self::Error::unexpected_message(
                    MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_ERROR,
                )),
            },

            SetTarget(m) => self.handle_set_target(m),

            SetGroupChannel(m) => match channel_type {
                SupportedChannelTypes::Group | SupportedChannelTypes::GroupAndExtended => {
                    self.handle_set_group_channel(m)
                }
                _ => Err(Self::Error::unexpected_message(
                    MESSAGE_TYPE_SET_GROUP_CHANNEL,
                )),
            },

            _ => Err(Self::Error::unexpected_message(0)),
        }
    }

    fn handle_open_standard_mining_channel_success(
        &mut self,
        msg: OpenStandardMiningChannelSuccess,
    ) -> Result<(), Self::Error>;

    fn handle_open_extended_mining_channel_success(
        &mut self,
        msg: OpenExtendedMiningChannelSuccess,
    ) -> Result<(), Self::Error>;

    fn handle_open_mining_channel_error(
        &mut self,
        msg: OpenMiningChannelError,
    ) -> Result<(), Self::Error>;

    fn handle_update_channel_error(&mut self, msg: UpdateChannelError) -> Result<(), Self::Error>;

    fn handle_close_channel(&mut self, msg: CloseChannel) -> Result<(), Self::Error>;

    fn handle_set_extranonce_prefix(&mut self, msg: SetExtranoncePrefix)
        -> Result<(), Self::Error>;

    fn handle_submit_shares_success(&mut self, msg: SubmitSharesSuccess)
        -> Result<(), Self::Error>;

    fn handle_submit_shares_error(&mut self, msg: SubmitSharesError) -> Result<(), Self::Error>;

    fn handle_new_mining_job(&mut self, msg: NewMiningJob) -> Result<(), Self::Error>;

    fn handle_new_extended_mining_job(
        &mut self,
        msg: NewExtendedMiningJob,
    ) -> Result<(), Self::Error>;

    fn handle_set_new_prev_hash(&mut self, msg: SetNewPrevHash) -> Result<(), Self::Error>;

    fn handle_set_custom_mining_job_success(
        &mut self,
        msg: SetCustomMiningJobSuccess,
    ) -> Result<(), Self::Error>;

    fn handle_set_custom_mining_job_error(
        &mut self,
        msg: SetCustomMiningJobError,
    ) -> Result<(), Self::Error>;

    fn handle_set_target(&mut self, msg: SetTarget) -> Result<(), Self::Error>;

    fn handle_set_group_channel(&mut self, msg: SetGroupChannel) -> Result<(), Self::Error>;
}

#[trait_variant::make(Send)]
pub trait HandleMiningMessagesFromServerAsync {
    type Error: HandlerErrorType;

    fn get_channel_type_for_server(&self) -> SupportedChannelTypes;
    fn is_work_selection_enabled_for_server(&self) -> bool;

    async fn handle_mining_message_frame_from_server(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        async move {
            let parsed: Mining = (message_type, payload)
                .try_into()
                .map_err(Self::Error::parse_error)?;
            self.handle_mining_message_from_server(parsed).await
        }
    }

    async fn handle_mining_message_from_server(
        &mut self,
        message: Mining,
    ) -> Result<(), Self::Error> {
        let (channel_type, work_selection) = (
            self.get_channel_type_for_server(),
            self.is_work_selection_enabled_for_server(),
        );

        async move {
            use Mining::*;
            match message {
                OpenStandardMiningChannelSuccess(m) => match channel_type {
                    SupportedChannelTypes::Standard
                    | SupportedChannelTypes::Group
                    | SupportedChannelTypes::GroupAndExtended => {
                        self.handle_open_standard_mining_channel_success(m).await
                    }
                    _ => Err(Self::Error::unexpected_message(
                        MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS,
                    )),
                },

                OpenExtendedMiningChannelSuccess(m) => match channel_type {
                    SupportedChannelTypes::Extended | SupportedChannelTypes::GroupAndExtended => {
                        self.handle_open_extended_mining_channel_success(m).await
                    }
                    _ => Err(Self::Error::unexpected_message(
                        MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCESS,
                    )),
                },

                OpenMiningChannelError(m) => self.handle_open_mining_channel_error(m).await,
                UpdateChannelError(m) => self.handle_update_channel_error(m).await,
                CloseChannel(m) => self.handle_close_channel(m).await,
                SetExtranoncePrefix(m) => self.handle_set_extranonce_prefix(m).await,
                SubmitSharesSuccess(m) => self.handle_submit_shares_success(m).await,
                SubmitSharesError(m) => self.handle_submit_shares_error(m).await,

                NewMiningJob(m) => match channel_type {
                    SupportedChannelTypes::Standard => self.handle_new_mining_job(m).await,
                    _ => Err(Self::Error::unexpected_message(MESSAGE_TYPE_NEW_MINING_JOB)),
                },

                NewExtendedMiningJob(m) => match channel_type {
                    SupportedChannelTypes::Extended
                    | SupportedChannelTypes::Group
                    | SupportedChannelTypes::GroupAndExtended => {
                        self.handle_new_extended_mining_job(m).await
                    }
                    _ => Err(Self::Error::unexpected_message(
                        MESSAGE_TYPE_NEW_EXTENDED_MINING_JOB,
                    )),
                },

                SetNewPrevHash(m) => self.handle_set_new_prev_hash(m).await,

                SetCustomMiningJobSuccess(m) => match (channel_type, work_selection) {
                    (SupportedChannelTypes::Extended, true)
                    | (SupportedChannelTypes::GroupAndExtended, true) => {
                        self.handle_set_custom_mining_job_success(m).await
                    }
                    _ => Err(Self::Error::unexpected_message(
                        MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_SUCCESS,
                    )),
                },

                SetCustomMiningJobError(m) => match (channel_type, work_selection) {
                    (SupportedChannelTypes::Extended, true)
                    | (SupportedChannelTypes::Group, true)
                    | (SupportedChannelTypes::GroupAndExtended, true) => {
                        self.handle_set_custom_mining_job_error(m).await
                    }
                    _ => Err(Self::Error::unexpected_message(
                        MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_ERROR,
                    )),
                },

                SetTarget(m) => self.handle_set_target(m).await,

                SetGroupChannel(m) => match channel_type {
                    SupportedChannelTypes::Group | SupportedChannelTypes::GroupAndExtended => {
                        self.handle_set_group_channel(m).await
                    }
                    _ => Err(Self::Error::unexpected_message(
                        MESSAGE_TYPE_SET_GROUP_CHANNEL,
                    )),
                },
                _ => Err(Self::Error::unexpected_message(0)),
            }
        }
    }

    async fn handle_open_standard_mining_channel_success(
        &mut self,
        msg: OpenStandardMiningChannelSuccess,
    ) -> Result<(), Self::Error>;

    async fn handle_open_extended_mining_channel_success(
        &mut self,
        msg: OpenExtendedMiningChannelSuccess,
    ) -> Result<(), Self::Error>;

    async fn handle_open_mining_channel_error(
        &mut self,
        msg: OpenMiningChannelError,
    ) -> Result<(), Self::Error>;

    async fn handle_update_channel_error(
        &mut self,
        msg: UpdateChannelError,
    ) -> Result<(), Self::Error>;

    async fn handle_close_channel(&mut self, msg: CloseChannel) -> Result<(), Self::Error>;

    async fn handle_set_extranonce_prefix(
        &mut self,
        msg: SetExtranoncePrefix,
    ) -> Result<(), Self::Error>;

    async fn handle_submit_shares_success(
        &mut self,
        msg: SubmitSharesSuccess,
    ) -> Result<(), Self::Error>;

    async fn handle_submit_shares_error(
        &mut self,
        msg: SubmitSharesError,
    ) -> Result<(), Self::Error>;

    async fn handle_new_mining_job(&mut self, msg: NewMiningJob) -> Result<(), Self::Error>;

    async fn handle_new_extended_mining_job(
        &mut self,
        msg: NewExtendedMiningJob,
    ) -> Result<(), Self::Error>;

    async fn handle_set_new_prev_hash(&mut self, msg: SetNewPrevHash) -> Result<(), Self::Error>;

    async fn handle_set_custom_mining_job_success(
        &mut self,
        msg: SetCustomMiningJobSuccess,
    ) -> Result<(), Self::Error>;

    async fn handle_set_custom_mining_job_error(
        &mut self,
        msg: SetCustomMiningJobError,
    ) -> Result<(), Self::Error>;

    async fn handle_set_target(&mut self, msg: SetTarget) -> Result<(), Self::Error>;

    async fn handle_set_group_channel(&mut self, msg: SetGroupChannel) -> Result<(), Self::Error>;
}

pub trait HandleMiningMessagesFromClientSync {
    type Error: HandlerErrorType;

    fn get_channel_type_for_client(&self) -> SupportedChannelTypes;
    fn is_work_selection_enabled_for_client(&self) -> bool;
    fn is_client_authorized(&self, user_identity: &Str0255) -> Result<bool, Self::Error>;

    fn handle_mining_message_frame_from_client(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        let parsed: Mining = (message_type, payload)
            .try_into()
            .map_err(Self::Error::parse_error)?;
        self.handle_mining_message_from_client(parsed)
    }

    fn handle_mining_message_from_client(&mut self, message: Mining) -> Result<(), Self::Error> {
        let (channel_type, work_selection) = (
            self.get_channel_type_for_client(),
            self.is_work_selection_enabled_for_client(),
        );

        use Mining::*;
        match message {
            OpenStandardMiningChannel(m) => match channel_type {
                SupportedChannelTypes::Standard
                | SupportedChannelTypes::Group
                | SupportedChannelTypes::GroupAndExtended => {
                    self.handle_open_standard_mining_channel(m)
                }
                SupportedChannelTypes::Extended => Err(Self::Error::unexpected_message(
                    MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL,
                )),
            },
            OpenExtendedMiningChannel(m) => match channel_type {
                SupportedChannelTypes::Extended | SupportedChannelTypes::GroupAndExtended => {
                    self.handle_open_extended_mining_channel(m)
                }
                _ => Err(Self::Error::unexpected_message(
                    MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL,
                )),
            },
            UpdateChannel(m) => self.handle_update_channel(m),

            SubmitSharesStandard(m) => match channel_type {
                SupportedChannelTypes::Standard
                | SupportedChannelTypes::Group
                | SupportedChannelTypes::GroupAndExtended => self.handle_submit_shares_standard(m),
                SupportedChannelTypes::Extended => Err(Self::Error::unexpected_message(
                    MESSAGE_TYPE_SUBMIT_SHARES_STANDARD,
                )),
            },

            SubmitSharesExtended(m) => match channel_type {
                SupportedChannelTypes::Extended | SupportedChannelTypes::GroupAndExtended => {
                    self.handle_submit_shares_extended(m)
                }
                _ => Err(Self::Error::unexpected_message(
                    MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED,
                )),
            },

            SetCustomMiningJob(m) => match (channel_type, work_selection) {
                (SupportedChannelTypes::Extended, true)
                | (SupportedChannelTypes::GroupAndExtended, true) => {
                    self.handle_set_custom_mining_job(m)
                }
                _ => Err(Self::Error::unexpected_message(
                    MESSAGE_TYPE_SET_CUSTOM_MINING_JOB,
                )),
            },
            CloseChannel(m) => self.handle_close_channel(m),

            _ => Err(Self::Error::unexpected_message(0)),
        }
    }

    fn handle_close_channel(&mut self, msg: CloseChannel) -> Result<(), Self::Error>;

    fn handle_open_standard_mining_channel(
        &mut self,
        msg: OpenStandardMiningChannel,
    ) -> Result<(), Self::Error>;

    fn handle_open_extended_mining_channel(
        &mut self,
        msg: OpenExtendedMiningChannel,
    ) -> Result<(), Self::Error>;

    fn handle_update_channel(&mut self, msg: UpdateChannel) -> Result<(), Self::Error>;

    fn handle_submit_shares_standard(
        &mut self,
        msg: SubmitSharesStandard,
    ) -> Result<(), Self::Error>;

    fn handle_submit_shares_extended(
        &mut self,
        msg: SubmitSharesExtended,
    ) -> Result<(), Self::Error>;

    fn handle_set_custom_mining_job(&mut self, msg: SetCustomMiningJob) -> Result<(), Self::Error>;
}

#[trait_variant::make(Send)]
pub trait HandleMiningMessagesFromClientAsync {
    type Error: HandlerErrorType;

    fn get_channel_type_for_client(&self) -> SupportedChannelTypes;
    fn is_work_selection_enabled_for_client(&self) -> bool;
    fn is_client_authorized(&self, user_identity: &Str0255) -> Result<bool, Self::Error>;

    async fn handle_mining_message_frame_from_client(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        async move {
            let parsed: Mining = (message_type, payload)
                .try_into()
                .map_err(Self::Error::parse_error)?;
            self.handle_mining_message_from_client(parsed).await
        }
    }

    async fn handle_mining_message_from_client(
        &mut self,
        message: Mining,
    ) -> Result<(), Self::Error> {
        let (channel_type, work_selection) = (
            self.get_channel_type_for_client(),
            self.is_work_selection_enabled_for_client(),
        );

        async move {
            use Mining::*;
            match message {
                OpenStandardMiningChannel(m) => match channel_type {
                    SupportedChannelTypes::Standard
                    | SupportedChannelTypes::Group
                    | SupportedChannelTypes::GroupAndExtended => {
                        self.handle_open_standard_mining_channel(m).await
                    }
                    SupportedChannelTypes::Extended => Err(Self::Error::unexpected_message(
                        MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL,
                    )),
                },
                OpenExtendedMiningChannel(m) => match channel_type {
                    SupportedChannelTypes::Extended | SupportedChannelTypes::GroupAndExtended => {
                        self.handle_open_extended_mining_channel(m).await
                    }
                    _ => Err(Self::Error::unexpected_message(
                        MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL,
                    )),
                },
                UpdateChannel(m) => self.handle_update_channel(m).await,

                SubmitSharesStandard(m) => match channel_type {
                    SupportedChannelTypes::Standard
                    | SupportedChannelTypes::Group
                    | SupportedChannelTypes::GroupAndExtended => {
                        self.handle_submit_shares_standard(m).await
                    }
                    SupportedChannelTypes::Extended => Err(Self::Error::unexpected_message(
                        MESSAGE_TYPE_SUBMIT_SHARES_STANDARD,
                    )),
                },

                SubmitSharesExtended(m) => match channel_type {
                    SupportedChannelTypes::Extended | SupportedChannelTypes::GroupAndExtended => {
                        self.handle_submit_shares_extended(m).await
                    }
                    _ => Err(Self::Error::unexpected_message(
                        MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED,
                    )),
                },

                SetCustomMiningJob(m) => match (channel_type, work_selection) {
                    (SupportedChannelTypes::Extended, true)
                    | (SupportedChannelTypes::GroupAndExtended, true) => {
                        self.handle_set_custom_mining_job(m).await
                    }
                    _ => Err(Self::Error::unexpected_message(
                        MESSAGE_TYPE_SET_CUSTOM_MINING_JOB,
                    )),
                },
                CloseChannel(m) => self.handle_close_channel(m).await,

                _ => Err(Self::Error::unexpected_message(0)),
            }
        }
    }

    async fn handle_close_channel(&mut self, msg: CloseChannel) -> Result<(), Self::Error>;

    async fn handle_open_standard_mining_channel(
        &mut self,
        msg: OpenStandardMiningChannel,
    ) -> Result<(), Self::Error>;

    async fn handle_open_extended_mining_channel(
        &mut self,
        msg: OpenExtendedMiningChannel,
    ) -> Result<(), Self::Error>;

    async fn handle_update_channel(&mut self, msg: UpdateChannel) -> Result<(), Self::Error>;

    async fn handle_submit_shares_standard(
        &mut self,
        msg: SubmitSharesStandard,
    ) -> Result<(), Self::Error>;

    async fn handle_submit_shares_extended(
        &mut self,
        msg: SubmitSharesExtended,
    ) -> Result<(), Self::Error>;

    async fn handle_set_custom_mining_job(
        &mut self,
        msg: SetCustomMiningJob,
    ) -> Result<(), Self::Error>;
}
