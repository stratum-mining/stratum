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

/// The server ID identifies which server a message originated from.
/// It is optional because when there is only a single server,
/// an explicit ID is unnecessary.
pub trait HandleMiningMessagesFromServerSync {
    type Error: HandlerErrorType;

    fn get_channel_type_for_server(&self, server_id: Option<usize>) -> SupportedChannelTypes;
    fn is_work_selection_enabled_for_server(&self, server_id: Option<usize>) -> bool;

    fn handle_mining_message_frame_from_server(
        &mut self,
        server_id: Option<usize>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        let parsed: Mining = (message_type, payload)
            .try_into()
            .map_err(Self::Error::parse_error)?;
        self.handle_mining_message_from_server(server_id, parsed)
    }

    fn handle_mining_message_from_server(
        &mut self,
        server_id: Option<usize>,
        message: Mining,
    ) -> Result<(), Self::Error> {
        let (channel_type, work_selection) = (
            self.get_channel_type_for_server(server_id),
            self.is_work_selection_enabled_for_server(server_id),
        );

        use Mining::*;
        match message {
            OpenStandardMiningChannelSuccess(m) => match channel_type {
                SupportedChannelTypes::Standard
                | SupportedChannelTypes::Group
                | SupportedChannelTypes::GroupAndExtended => {
                    self.handle_open_standard_mining_channel_success(server_id, m)
                }
                _ => Err(Self::Error::unexpected_message(
                    MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS,
                )),
            },

            OpenExtendedMiningChannelSuccess(m) => match channel_type {
                SupportedChannelTypes::Extended | SupportedChannelTypes::GroupAndExtended => {
                    self.handle_open_extended_mining_channel_success(server_id, m)
                }
                _ => Err(Self::Error::unexpected_message(
                    MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCESS,
                )),
            },

            OpenMiningChannelError(m) => self.handle_open_mining_channel_error(server_id, m),
            UpdateChannelError(m) => self.handle_update_channel_error(server_id, m),
            CloseChannel(m) => self.handle_close_channel(server_id, m),
            SetExtranoncePrefix(m) => self.handle_set_extranonce_prefix(server_id, m),
            SubmitSharesSuccess(m) => self.handle_submit_shares_success(server_id, m),
            SubmitSharesError(m) => self.handle_submit_shares_error(server_id, m),

            NewMiningJob(m) => match channel_type {
                SupportedChannelTypes::Standard => self.handle_new_mining_job(server_id, m),
                _ => Err(Self::Error::unexpected_message(MESSAGE_TYPE_NEW_MINING_JOB)),
            },

            NewExtendedMiningJob(m) => match channel_type {
                SupportedChannelTypes::Extended
                | SupportedChannelTypes::Group
                | SupportedChannelTypes::GroupAndExtended => {
                    self.handle_new_extended_mining_job(server_id, m)
                }
                _ => Err(Self::Error::unexpected_message(
                    MESSAGE_TYPE_NEW_EXTENDED_MINING_JOB,
                )),
            },

            SetNewPrevHash(m) => self.handle_set_new_prev_hash(server_id, m),

            SetCustomMiningJobSuccess(m) => match (channel_type, work_selection) {
                (SupportedChannelTypes::Extended, true)
                | (SupportedChannelTypes::GroupAndExtended, true) => {
                    self.handle_set_custom_mining_job_success(server_id, m)
                }
                _ => Err(Self::Error::unexpected_message(
                    MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_SUCCESS,
                )),
            },

            SetCustomMiningJobError(m) => match (channel_type, work_selection) {
                (SupportedChannelTypes::Extended, true)
                | (SupportedChannelTypes::Group, true)
                | (SupportedChannelTypes::GroupAndExtended, true) => {
                    self.handle_set_custom_mining_job_error(server_id, m)
                }
                _ => Err(Self::Error::unexpected_message(
                    MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_ERROR,
                )),
            },

            SetTarget(m) => self.handle_set_target(server_id, m),

            SetGroupChannel(m) => match channel_type {
                SupportedChannelTypes::Group | SupportedChannelTypes::GroupAndExtended => {
                    self.handle_set_group_channel(server_id, m)
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
        server_id: Option<usize>,
        msg: OpenStandardMiningChannelSuccess,
    ) -> Result<(), Self::Error>;

    fn handle_open_extended_mining_channel_success(
        &mut self,
        server_id: Option<usize>,
        msg: OpenExtendedMiningChannelSuccess,
    ) -> Result<(), Self::Error>;

    fn handle_open_mining_channel_error(
        &mut self,
        server_id: Option<usize>,
        msg: OpenMiningChannelError,
    ) -> Result<(), Self::Error>;

    fn handle_update_channel_error(
        &mut self,
        server_id: Option<usize>,
        msg: UpdateChannelError,
    ) -> Result<(), Self::Error>;

    fn handle_close_channel(
        &mut self,
        server_id: Option<usize>,
        msg: CloseChannel,
    ) -> Result<(), Self::Error>;

    fn handle_set_extranonce_prefix(
        &mut self,
        server_id: Option<usize>,
        msg: SetExtranoncePrefix,
    ) -> Result<(), Self::Error>;

    fn handle_submit_shares_success(
        &mut self,
        server_id: Option<usize>,
        msg: SubmitSharesSuccess,
    ) -> Result<(), Self::Error>;

    fn handle_submit_shares_error(
        &mut self,
        server_id: Option<usize>,
        msg: SubmitSharesError,
    ) -> Result<(), Self::Error>;

    fn handle_new_mining_job(
        &mut self,
        server_id: Option<usize>,
        msg: NewMiningJob,
    ) -> Result<(), Self::Error>;

    fn handle_new_extended_mining_job(
        &mut self,
        server_id: Option<usize>,
        msg: NewExtendedMiningJob,
    ) -> Result<(), Self::Error>;

    fn handle_set_new_prev_hash(
        &mut self,
        server_id: Option<usize>,
        msg: SetNewPrevHash,
    ) -> Result<(), Self::Error>;

    fn handle_set_custom_mining_job_success(
        &mut self,
        server_id: Option<usize>,
        msg: SetCustomMiningJobSuccess,
    ) -> Result<(), Self::Error>;

    fn handle_set_custom_mining_job_error(
        &mut self,
        server_id: Option<usize>,
        msg: SetCustomMiningJobError,
    ) -> Result<(), Self::Error>;

    fn handle_set_target(
        &mut self,
        server_id: Option<usize>,
        msg: SetTarget,
    ) -> Result<(), Self::Error>;

    fn handle_set_group_channel(
        &mut self,
        server_id: Option<usize>,
        msg: SetGroupChannel,
    ) -> Result<(), Self::Error>;
}

/// The server ID identifies which server a message originated from.
/// It is optional because when there is only a single server,
/// an explicit ID is unnecessary.
#[trait_variant::make(Send)]
pub trait HandleMiningMessagesFromServerAsync {
    type Error: HandlerErrorType;

    fn get_channel_type_for_server(&self, server_id: Option<usize>) -> SupportedChannelTypes;
    fn is_work_selection_enabled_for_server(&self, server_id: Option<usize>) -> bool;

    async fn handle_mining_message_frame_from_server(
        &mut self,
        server_id: Option<usize>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        async move {
            let parsed: Mining = (message_type, payload)
                .try_into()
                .map_err(Self::Error::parse_error)?;
            self.handle_mining_message_from_server(server_id, parsed)
                .await
        }
    }

    async fn handle_mining_message_from_server(
        &mut self,
        server_id: Option<usize>,
        message: Mining,
    ) -> Result<(), Self::Error> {
        let (channel_type, work_selection) = (
            self.get_channel_type_for_server(server_id),
            self.is_work_selection_enabled_for_server(server_id),
        );

        async move {
            use Mining::*;
            match message {
                OpenStandardMiningChannelSuccess(m) => match channel_type {
                    SupportedChannelTypes::Standard
                    | SupportedChannelTypes::Group
                    | SupportedChannelTypes::GroupAndExtended => {
                        self.handle_open_standard_mining_channel_success(server_id, m)
                            .await
                    }
                    _ => Err(Self::Error::unexpected_message(
                        MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS,
                    )),
                },

                OpenExtendedMiningChannelSuccess(m) => match channel_type {
                    SupportedChannelTypes::Extended | SupportedChannelTypes::GroupAndExtended => {
                        self.handle_open_extended_mining_channel_success(server_id, m)
                            .await
                    }
                    _ => Err(Self::Error::unexpected_message(
                        MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCESS,
                    )),
                },

                OpenMiningChannelError(m) => {
                    self.handle_open_mining_channel_error(server_id, m).await
                }
                UpdateChannelError(m) => self.handle_update_channel_error(server_id, m).await,
                CloseChannel(m) => self.handle_close_channel(server_id, m).await,
                SetExtranoncePrefix(m) => self.handle_set_extranonce_prefix(server_id, m).await,
                SubmitSharesSuccess(m) => self.handle_submit_shares_success(server_id, m).await,
                SubmitSharesError(m) => self.handle_submit_shares_error(server_id, m).await,

                NewMiningJob(m) => match channel_type {
                    SupportedChannelTypes::Standard => {
                        self.handle_new_mining_job(server_id, m).await
                    }
                    _ => Err(Self::Error::unexpected_message(MESSAGE_TYPE_NEW_MINING_JOB)),
                },

                NewExtendedMiningJob(m) => match channel_type {
                    SupportedChannelTypes::Extended
                    | SupportedChannelTypes::Group
                    | SupportedChannelTypes::GroupAndExtended => {
                        self.handle_new_extended_mining_job(server_id, m).await
                    }
                    _ => Err(Self::Error::unexpected_message(
                        MESSAGE_TYPE_NEW_EXTENDED_MINING_JOB,
                    )),
                },

                SetNewPrevHash(m) => self.handle_set_new_prev_hash(server_id, m).await,

                SetCustomMiningJobSuccess(m) => match (channel_type, work_selection) {
                    (SupportedChannelTypes::Extended, true)
                    | (SupportedChannelTypes::GroupAndExtended, true) => {
                        self.handle_set_custom_mining_job_success(server_id, m)
                            .await
                    }
                    _ => Err(Self::Error::unexpected_message(
                        MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_SUCCESS,
                    )),
                },

                SetCustomMiningJobError(m) => match (channel_type, work_selection) {
                    (SupportedChannelTypes::Extended, true)
                    | (SupportedChannelTypes::Group, true)
                    | (SupportedChannelTypes::GroupAndExtended, true) => {
                        self.handle_set_custom_mining_job_error(server_id, m).await
                    }
                    _ => Err(Self::Error::unexpected_message(
                        MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_ERROR,
                    )),
                },

                SetTarget(m) => self.handle_set_target(server_id, m).await,

                SetGroupChannel(m) => match channel_type {
                    SupportedChannelTypes::Group | SupportedChannelTypes::GroupAndExtended => {
                        self.handle_set_group_channel(server_id, m).await
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
        server_id: Option<usize>,
        msg: OpenStandardMiningChannelSuccess,
    ) -> Result<(), Self::Error>;

    async fn handle_open_extended_mining_channel_success(
        &mut self,
        server_id: Option<usize>,
        msg: OpenExtendedMiningChannelSuccess,
    ) -> Result<(), Self::Error>;

    async fn handle_open_mining_channel_error(
        &mut self,
        server_id: Option<usize>,
        msg: OpenMiningChannelError,
    ) -> Result<(), Self::Error>;

    async fn handle_update_channel_error(
        &mut self,
        server_id: Option<usize>,
        msg: UpdateChannelError,
    ) -> Result<(), Self::Error>;

    async fn handle_close_channel(
        &mut self,
        server_id: Option<usize>,
        msg: CloseChannel,
    ) -> Result<(), Self::Error>;

    async fn handle_set_extranonce_prefix(
        &mut self,
        server_id: Option<usize>,
        msg: SetExtranoncePrefix,
    ) -> Result<(), Self::Error>;

    async fn handle_submit_shares_success(
        &mut self,
        server_id: Option<usize>,
        msg: SubmitSharesSuccess,
    ) -> Result<(), Self::Error>;

    async fn handle_submit_shares_error(
        &mut self,
        server_id: Option<usize>,
        msg: SubmitSharesError,
    ) -> Result<(), Self::Error>;

    async fn handle_new_mining_job(
        &mut self,
        server_id: Option<usize>,
        msg: NewMiningJob,
    ) -> Result<(), Self::Error>;

    async fn handle_new_extended_mining_job(
        &mut self,
        server_id: Option<usize>,
        msg: NewExtendedMiningJob,
    ) -> Result<(), Self::Error>;

    async fn handle_set_new_prev_hash(
        &mut self,
        server_id: Option<usize>,
        msg: SetNewPrevHash,
    ) -> Result<(), Self::Error>;

    async fn handle_set_custom_mining_job_success(
        &mut self,
        server_id: Option<usize>,
        msg: SetCustomMiningJobSuccess,
    ) -> Result<(), Self::Error>;

    async fn handle_set_custom_mining_job_error(
        &mut self,
        server_id: Option<usize>,
        msg: SetCustomMiningJobError,
    ) -> Result<(), Self::Error>;

    async fn handle_set_target(
        &mut self,
        server_id: Option<usize>,
        msg: SetTarget,
    ) -> Result<(), Self::Error>;

    async fn handle_set_group_channel(
        &mut self,
        server_id: Option<usize>,
        msg: SetGroupChannel,
    ) -> Result<(), Self::Error>;
}

/// The client ID identifies which client a message originated from.
/// It is optional because when there is only a single client,
/// an explicit ID is unnecessary.
pub trait HandleMiningMessagesFromClientSync {
    type Error: HandlerErrorType;

    fn get_channel_type_for_client(&self, client_id: Option<usize>) -> SupportedChannelTypes;
    fn is_work_selection_enabled_for_client(&self, client_id: Option<usize>) -> bool;
    fn is_client_authorized(
        &self,
        client_id: Option<usize>,
        user_identity: &Str0255,
    ) -> Result<bool, Self::Error>;

    fn handle_mining_message_frame_from_client(
        &mut self,
        client_id: Option<usize>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        let parsed: Mining = (message_type, payload)
            .try_into()
            .map_err(Self::Error::parse_error)?;
        self.handle_mining_message_from_client(client_id, parsed)
    }

    fn handle_mining_message_from_client(
        &mut self,
        client_id: Option<usize>,
        message: Mining,
    ) -> Result<(), Self::Error> {
        let (channel_type, work_selection) = (
            self.get_channel_type_for_client(client_id),
            self.is_work_selection_enabled_for_client(client_id),
        );

        use Mining::*;
        match message {
            OpenStandardMiningChannel(m) => match channel_type {
                SupportedChannelTypes::Standard
                | SupportedChannelTypes::Group
                | SupportedChannelTypes::GroupAndExtended => {
                    self.handle_open_standard_mining_channel(client_id, m)
                }
                SupportedChannelTypes::Extended => Err(Self::Error::unexpected_message(
                    MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL,
                )),
            },
            OpenExtendedMiningChannel(m) => match channel_type {
                SupportedChannelTypes::Extended | SupportedChannelTypes::GroupAndExtended => {
                    self.handle_open_extended_mining_channel(client_id, m)
                }
                _ => Err(Self::Error::unexpected_message(
                    MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL,
                )),
            },
            UpdateChannel(m) => self.handle_update_channel(client_id, m),

            SubmitSharesStandard(m) => match channel_type {
                SupportedChannelTypes::Standard
                | SupportedChannelTypes::Group
                | SupportedChannelTypes::GroupAndExtended => {
                    self.handle_submit_shares_standard(client_id, m)
                }
                SupportedChannelTypes::Extended => Err(Self::Error::unexpected_message(
                    MESSAGE_TYPE_SUBMIT_SHARES_STANDARD,
                )),
            },

            SubmitSharesExtended(m) => match channel_type {
                SupportedChannelTypes::Extended | SupportedChannelTypes::GroupAndExtended => {
                    self.handle_submit_shares_extended(client_id, m)
                }
                _ => Err(Self::Error::unexpected_message(
                    MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED,
                )),
            },

            SetCustomMiningJob(m) => match (channel_type, work_selection) {
                (SupportedChannelTypes::Extended, true)
                | (SupportedChannelTypes::GroupAndExtended, true) => {
                    self.handle_set_custom_mining_job(client_id, m)
                }
                _ => Err(Self::Error::unexpected_message(
                    MESSAGE_TYPE_SET_CUSTOM_MINING_JOB,
                )),
            },
            CloseChannel(m) => self.handle_close_channel(client_id, m),

            _ => Err(Self::Error::unexpected_message(0)),
        }
    }

    fn handle_close_channel(
        &mut self,
        client_id: Option<usize>,
        msg: CloseChannel,
    ) -> Result<(), Self::Error>;

    fn handle_open_standard_mining_channel(
        &mut self,
        client_id: Option<usize>,
        msg: OpenStandardMiningChannel,
    ) -> Result<(), Self::Error>;

    fn handle_open_extended_mining_channel(
        &mut self,
        client_id: Option<usize>,
        msg: OpenExtendedMiningChannel,
    ) -> Result<(), Self::Error>;

    fn handle_update_channel(
        &mut self,
        client_id: Option<usize>,
        msg: UpdateChannel,
    ) -> Result<(), Self::Error>;

    fn handle_submit_shares_standard(
        &mut self,
        client_id: Option<usize>,
        msg: SubmitSharesStandard,
    ) -> Result<(), Self::Error>;

    fn handle_submit_shares_extended(
        &mut self,
        client_id: Option<usize>,
        msg: SubmitSharesExtended,
    ) -> Result<(), Self::Error>;

    fn handle_set_custom_mining_job(
        &mut self,
        client_id: Option<usize>,
        msg: SetCustomMiningJob,
    ) -> Result<(), Self::Error>;
}

/// The client ID identifies which client a message originated from.
/// It is optional because when there is only a single client,
/// an explicit ID is unnecessary.
#[trait_variant::make(Send)]
pub trait HandleMiningMessagesFromClientAsync {
    type Error: HandlerErrorType;

    fn get_channel_type_for_client(&self, client_id: Option<usize>) -> SupportedChannelTypes;
    fn is_work_selection_enabled_for_client(&self, client_id: Option<usize>) -> bool;
    fn is_client_authorized(
        &self,
        client_id: Option<usize>,
        user_identity: &Str0255,
    ) -> Result<bool, Self::Error>;

    async fn handle_mining_message_frame_from_client(
        &mut self,
        client_id: Option<usize>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        async move {
            let parsed: Mining = (message_type, payload)
                .try_into()
                .map_err(Self::Error::parse_error)?;
            self.handle_mining_message_from_client(client_id, parsed)
                .await
        }
    }

    async fn handle_mining_message_from_client(
        &mut self,
        client_id: Option<usize>,
        message: Mining,
    ) -> Result<(), Self::Error> {
        let (channel_type, work_selection) = (
            self.get_channel_type_for_client(client_id),
            self.is_work_selection_enabled_for_client(client_id),
        );

        async move {
            use Mining::*;
            match message {
                OpenStandardMiningChannel(m) => match channel_type {
                    SupportedChannelTypes::Standard
                    | SupportedChannelTypes::Group
                    | SupportedChannelTypes::GroupAndExtended => {
                        self.handle_open_standard_mining_channel(client_id, m).await
                    }
                    SupportedChannelTypes::Extended => Err(Self::Error::unexpected_message(
                        MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL,
                    )),
                },
                OpenExtendedMiningChannel(m) => match channel_type {
                    SupportedChannelTypes::Extended | SupportedChannelTypes::GroupAndExtended => {
                        self.handle_open_extended_mining_channel(client_id, m).await
                    }
                    _ => Err(Self::Error::unexpected_message(
                        MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL,
                    )),
                },
                UpdateChannel(m) => self.handle_update_channel(client_id, m).await,

                SubmitSharesStandard(m) => match channel_type {
                    SupportedChannelTypes::Standard
                    | SupportedChannelTypes::Group
                    | SupportedChannelTypes::GroupAndExtended => {
                        self.handle_submit_shares_standard(client_id, m).await
                    }
                    SupportedChannelTypes::Extended => Err(Self::Error::unexpected_message(
                        MESSAGE_TYPE_SUBMIT_SHARES_STANDARD,
                    )),
                },

                SubmitSharesExtended(m) => match channel_type {
                    SupportedChannelTypes::Extended | SupportedChannelTypes::GroupAndExtended => {
                        self.handle_submit_shares_extended(client_id, m).await
                    }
                    _ => Err(Self::Error::unexpected_message(
                        MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED,
                    )),
                },

                SetCustomMiningJob(m) => match (channel_type, work_selection) {
                    (SupportedChannelTypes::Extended, true)
                    | (SupportedChannelTypes::GroupAndExtended, true) => {
                        self.handle_set_custom_mining_job(client_id, m).await
                    }
                    _ => Err(Self::Error::unexpected_message(
                        MESSAGE_TYPE_SET_CUSTOM_MINING_JOB,
                    )),
                },
                CloseChannel(m) => self.handle_close_channel(client_id, m).await,

                _ => Err(Self::Error::unexpected_message(0)),
            }
        }
    }

    async fn handle_close_channel(
        &mut self,
        client_id: Option<usize>,
        msg: CloseChannel,
    ) -> Result<(), Self::Error>;

    async fn handle_open_standard_mining_channel(
        &mut self,
        client_id: Option<usize>,
        msg: OpenStandardMiningChannel,
    ) -> Result<(), Self::Error>;

    async fn handle_open_extended_mining_channel(
        &mut self,
        client_id: Option<usize>,
        msg: OpenExtendedMiningChannel,
    ) -> Result<(), Self::Error>;

    async fn handle_update_channel(
        &mut self,
        client_id: Option<usize>,
        msg: UpdateChannel,
    ) -> Result<(), Self::Error>;

    async fn handle_submit_shares_standard(
        &mut self,
        client_id: Option<usize>,
        msg: SubmitSharesStandard,
    ) -> Result<(), Self::Error>;

    async fn handle_submit_shares_extended(
        &mut self,
        client_id: Option<usize>,
        msg: SubmitSharesExtended,
    ) -> Result<(), Self::Error>;

    async fn handle_set_custom_mining_job(
        &mut self,
        client_id: Option<usize>,
        msg: SetCustomMiningJob,
    ) -> Result<(), Self::Error>;
}
