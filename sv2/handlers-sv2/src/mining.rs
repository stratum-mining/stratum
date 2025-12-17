use crate::error::HandlerErrorType;
use binary_sv2::Str0255;
use framing_sv2::header::Header;
use mining_sv2::{
    CloseChannel, NewExtendedMiningJob, NewMiningJob, OpenExtendedMiningChannel,
    OpenExtendedMiningChannelSuccess, OpenMiningChannelError, OpenStandardMiningChannel,
    OpenStandardMiningChannelSuccess, SetCustomMiningJob, SetCustomMiningJobError,
    SetCustomMiningJobSuccess, SetExtranoncePrefix, SetGroupChannel, SetNewPrevHash, SetTarget,
    SubmitSharesError, SubmitSharesExtended, SubmitSharesStandard, SubmitSharesSuccess,
    UpdateChannel, UpdateChannelError,
};
use parsers_sv2::{parse_message_frame_with_tlvs, AnyMessage, Mining, Tlv};

use mining_sv2::*;

#[derive(PartialEq, Eq, Copy, Clone)]
pub enum SupportedChannelTypes {
    Standard,
    Extended,
    Group,
    GroupAndExtended,
}
/// Synchronous handler trait for processing mining messages received from servers.
///
/// The server ID identifies which server a message originated from.
/// Whether this is relevant or not depends on which object is implementing the trait, and whether
/// this contextual information is readily available or not. In cases where `server_id` is either
/// irrelevant or can be inferred without the context, this should always be `None`.
///
/// Handler methods receive an optional `tlv_data` parameter containing validated TLV fields
/// when extension data is appended to messages. It will be `Some` only if validation succeeds
/// against negotiated extensions.
pub trait HandleMiningMessagesFromServerSync {
    type Error: HandlerErrorType;

    fn get_channel_type_for_server(&self, server_id: Option<usize>) -> SupportedChannelTypes;
    fn is_work_selection_enabled_for_server(&self, server_id: Option<usize>) -> bool;

    /// Returns the list of negotiated extension_types with a server.
    ///
    /// Return an empty Vec if no extensions have been negotiated.
    fn get_negotiated_extensions_with_server(
        &self,
        server_id: Option<usize>,
    ) -> Result<Vec<u16>, Self::Error>;

    /// Handles a raw Mining protocol message frame from a server.
    ///
    /// This method parses the raw frame, extracts any TLV extension data, and delegates
    /// to `handle_mining_message_from_server` with the parsed message and TLV fields.
    fn handle_mining_message_frame_from_server(
        &mut self,
        server_id: Option<usize>,
        header: Header,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        let negotiated_extensions = self.get_negotiated_extensions_with_server(server_id)?;
        if negotiated_extensions.is_empty() {
            let parsed: Mining<'_> = (header.msg_type(), payload)
                .try_into()
                .map_err(Self::Error::parse_error)?;
            return self.handle_mining_message_from_server(server_id, parsed, None);
        }
        let (parsed, tlv_fields) =
            parse_message_frame_with_tlvs(header, payload, &negotiated_extensions)
                .map_err(Self::Error::parse_error)?;
        match parsed {
            AnyMessage::Mining(parsed) => {
                self.handle_mining_message_from_server(server_id, parsed, tlv_fields.as_deref())
            }
            _ => Err(Self::Error::unexpected_message(
                header.ext_type_without_channel_msg(),
                header.msg_type(),
            )),
        }
    }

    /// Handles a parsed mining message from a server.
    ///
    /// The `tlv_fields` parameter contains parsed TLV fields if the message has extension
    /// data appended. It will be `Some(&[Tlv])` when valid TLV data is present, or `None`
    /// if no TLV data exists or validation fails. Each `Tlv` struct provides direct access to
    /// `extension_type`, `field_type`, `length`, and `value`.
    fn handle_mining_message_from_server(
        &mut self,
        server_id: Option<usize>,
        message: Mining,
        tlv_fields: Option<&[Tlv]>,
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
                    self.handle_open_standard_mining_channel_success(server_id, m, tlv_fields)
                }
                _ => Err(Self::Error::unexpected_message(
                    0,
                    MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS,
                )),
            },
            OpenExtendedMiningChannelSuccess(m) => match channel_type {
                SupportedChannelTypes::Extended | SupportedChannelTypes::GroupAndExtended => {
                    self.handle_open_extended_mining_channel_success(server_id, m, tlv_fields)
                }
                _ => Err(Self::Error::unexpected_message(
                    0,
                    MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCESS,
                )),
            },

            OpenMiningChannelError(m) => {
                self.handle_open_mining_channel_error(server_id, m, tlv_fields)
            }
            UpdateChannelError(m) => self.handle_update_channel_error(server_id, m, tlv_fields),
            CloseChannel(m) => self.handle_close_channel(server_id, m, tlv_fields),
            SetExtranoncePrefix(m) => self.handle_set_extranonce_prefix(server_id, m, tlv_fields),
            SubmitSharesSuccess(m) => self.handle_submit_shares_success(server_id, m, tlv_fields),
            SubmitSharesError(m) => self.handle_submit_shares_error(server_id, m, tlv_fields),

            NewMiningJob(m) => match channel_type {
                SupportedChannelTypes::Standard => {
                    self.handle_new_mining_job(server_id, m, tlv_fields)
                }
                _ => Err(Self::Error::unexpected_message(
                    0,
                    MESSAGE_TYPE_NEW_MINING_JOB,
                )),
            },
            NewExtendedMiningJob(m) => match channel_type {
                SupportedChannelTypes::Extended
                | SupportedChannelTypes::Group
                | SupportedChannelTypes::GroupAndExtended => {
                    self.handle_new_extended_mining_job(server_id, m, tlv_fields)
                }
                _ => Err(Self::Error::unexpected_message(
                    0,
                    MESSAGE_TYPE_NEW_EXTENDED_MINING_JOB,
                )),
            },

            SetNewPrevHash(m) => self.handle_set_new_prev_hash(server_id, m, tlv_fields),

            SetCustomMiningJobSuccess(m) => match (channel_type, work_selection) {
                (SupportedChannelTypes::Extended, true)
                | (SupportedChannelTypes::GroupAndExtended, true) => {
                    self.handle_set_custom_mining_job_success(server_id, m, tlv_fields)
                }
                _ => Err(Self::Error::unexpected_message(
                    0,
                    MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_SUCCESS,
                )),
            },
            SetCustomMiningJobError(m) => match (channel_type, work_selection) {
                (SupportedChannelTypes::Extended, true)
                | (SupportedChannelTypes::Group, true)
                | (SupportedChannelTypes::GroupAndExtended, true) => {
                    self.handle_set_custom_mining_job_error(server_id, m, tlv_fields)
                }
                _ => Err(Self::Error::unexpected_message(
                    0,
                    MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_ERROR,
                )),
            },

            SetTarget(m) => self.handle_set_target(server_id, m, tlv_fields),

            SetGroupChannel(m) => match channel_type {
                SupportedChannelTypes::Group | SupportedChannelTypes::GroupAndExtended => {
                    self.handle_set_group_channel(server_id, m, tlv_fields)
                }
                _ => Err(Self::Error::unexpected_message(
                    0,
                    MESSAGE_TYPE_SET_GROUP_CHANNEL,
                )),
            },
            SubmitSharesExtended(_) => Err(Self::Error::unexpected_message(
                0,
                MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED,
            )),
            SubmitSharesStandard(_) => Err(Self::Error::unexpected_message(
                0,
                MESSAGE_TYPE_SUBMIT_SHARES_STANDARD,
            )),
            UpdateChannel(_) => Err(Self::Error::unexpected_message(
                0,
                MESSAGE_TYPE_UPDATE_CHANNEL,
            )),
            OpenExtendedMiningChannel(_) => Err(Self::Error::unexpected_message(
                0,
                MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL,
            )),
            OpenStandardMiningChannel(_) => Err(Self::Error::unexpected_message(
                0,
                MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL,
            )),
            SetCustomMiningJob(_) => Err(Self::Error::unexpected_message(
                0,
                MESSAGE_TYPE_SET_CUSTOM_MINING_JOB,
            )),
        }
    }

    fn handle_open_standard_mining_channel_success(
        &mut self,
        server_id: Option<usize>,
        msg: OpenStandardMiningChannelSuccess,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_open_extended_mining_channel_success(
        &mut self,
        server_id: Option<usize>,
        msg: OpenExtendedMiningChannelSuccess,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_open_mining_channel_error(
        &mut self,
        server_id: Option<usize>,
        msg: OpenMiningChannelError,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_update_channel_error(
        &mut self,
        server_id: Option<usize>,
        msg: UpdateChannelError,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_close_channel(
        &mut self,
        server_id: Option<usize>,
        msg: CloseChannel,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_set_extranonce_prefix(
        &mut self,
        server_id: Option<usize>,
        msg: SetExtranoncePrefix,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_submit_shares_success(
        &mut self,
        server_id: Option<usize>,
        msg: SubmitSharesSuccess,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_submit_shares_error(
        &mut self,
        server_id: Option<usize>,
        msg: SubmitSharesError,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_new_mining_job(
        &mut self,
        server_id: Option<usize>,
        msg: NewMiningJob,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_new_extended_mining_job(
        &mut self,
        server_id: Option<usize>,
        msg: NewExtendedMiningJob,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_set_new_prev_hash(
        &mut self,
        server_id: Option<usize>,
        msg: SetNewPrevHash,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_set_custom_mining_job_success(
        &mut self,
        server_id: Option<usize>,
        msg: SetCustomMiningJobSuccess,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_set_custom_mining_job_error(
        &mut self,
        server_id: Option<usize>,
        msg: SetCustomMiningJobError,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_set_target(
        &mut self,
        server_id: Option<usize>,
        msg: SetTarget,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_set_group_channel(
        &mut self,
        server_id: Option<usize>,
        msg: SetGroupChannel,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;
}

/// Asynchronous handler trait for processing mining messages received from servers.
///
/// The server ID identifies which server a message originated from.
/// Whether this is relevant or not depends on which object is implementing the trait, and whether
/// this contextual information is readily available or not. In cases where `server_id` is either
/// irrelevant or can be inferred without the context, this should always be `None`.
///
/// Handler methods receive an optional `tlv_data` parameter containing validated TLV fields
/// when extension data is appended to messages. It will be `Some` only if validation succeeds
/// against negotiated extensions.
#[trait_variant::make(Send)]
pub trait HandleMiningMessagesFromServerAsync {
    type Error: HandlerErrorType;

    fn get_channel_type_for_server(&self, server_id: Option<usize>) -> SupportedChannelTypes;
    fn is_work_selection_enabled_for_server(&self, server_id: Option<usize>) -> bool;

    /// Returns the list of negotiated extension_types with a server.
    ///
    /// Return an empty Vec if no extensions have been negotiated.
    fn get_negotiated_extensions_with_server(
        &self,
        server_id: Option<usize>,
    ) -> Result<Vec<u16>, Self::Error>;

    /// Handles a raw Mining protocol message frame from a server.
    ///
    /// This method parses the raw frame, extracts any TLV extension data, and delegates
    /// to `handle_mining_message_from_server` with the parsed message and TLV fields.
    async fn handle_mining_message_frame_from_server(
        &mut self,
        server_id: Option<usize>,
        header: Header,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        async move {
            let negotiated_extensions = self.get_negotiated_extensions_with_server(server_id)?;
            if negotiated_extensions.is_empty() {
                let parsed: Mining<'_> = (header.msg_type(), payload)
                    .try_into()
                    .map_err(Self::Error::parse_error)?;
                return self
                    .handle_mining_message_from_server(server_id, parsed, None)
                    .await;
            }
            let (parsed, tlv_fields) =
                parse_message_frame_with_tlvs(header, payload, &negotiated_extensions)
                    .map_err(Self::Error::parse_error)?;
            match parsed {
                AnyMessage::Mining(parsed) => {
                    self.handle_mining_message_from_server(server_id, parsed, tlv_fields.as_deref())
                        .await
                }
                _ => Err(Self::Error::unexpected_message(
                    header.ext_type_without_channel_msg(),
                    header.msg_type(),
                )),
            }
        }
    }

    /// Handles a parsed Mining protocol message from a server.
    ///
    /// The `tlv_fields` parameter contains parsed TLV fields if the message has extension
    /// data appended. It will be `Some(&[Tlv])` when valid TLV data is present, or `None`
    /// if no TLV data exists or validation fails. Each `Tlv` struct provides direct access to
    /// `extension_type`, `field_type`, `length`, and `value`.
    async fn handle_mining_message_from_server(
        &mut self,
        server_id: Option<usize>,
        message: Mining,
        tlv_fields: Option<&[Tlv]>,
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
                        self.handle_open_standard_mining_channel_success(server_id, m, tlv_fields)
                            .await
                    }
                    _ => Err(Self::Error::unexpected_message(
                        0,
                        MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS,
                    )),
                },
                OpenExtendedMiningChannelSuccess(m) => match channel_type {
                    SupportedChannelTypes::Extended | SupportedChannelTypes::GroupAndExtended => {
                        self.handle_open_extended_mining_channel_success(server_id, m, tlv_fields)
                            .await
                    }
                    _ => Err(Self::Error::unexpected_message(
                        0,
                        MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCESS,
                    )),
                },
                OpenMiningChannelError(m) => {
                    self.handle_open_mining_channel_error(server_id, m, tlv_fields)
                        .await
                }
                UpdateChannelError(m) => {
                    self.handle_update_channel_error(server_id, m, tlv_fields)
                        .await
                }
                CloseChannel(m) => self.handle_close_channel(server_id, m, tlv_fields).await,
                SetExtranoncePrefix(m) => {
                    self.handle_set_extranonce_prefix(server_id, m, tlv_fields)
                        .await
                }
                SubmitSharesSuccess(m) => {
                    self.handle_submit_shares_success(server_id, m, tlv_fields)
                        .await
                }
                SubmitSharesError(m) => {
                    self.handle_submit_shares_error(server_id, m, tlv_fields)
                        .await
                }

                NewMiningJob(m) => match channel_type {
                    SupportedChannelTypes::Standard => {
                        self.handle_new_mining_job(server_id, m, tlv_fields).await
                    }
                    _ => Err(Self::Error::unexpected_message(
                        0,
                        MESSAGE_TYPE_NEW_MINING_JOB,
                    )),
                },
                NewExtendedMiningJob(m) => match channel_type {
                    SupportedChannelTypes::Extended
                    | SupportedChannelTypes::Group
                    | SupportedChannelTypes::GroupAndExtended => {
                        self.handle_new_extended_mining_job(server_id, m, tlv_fields)
                            .await
                    }
                    _ => Err(Self::Error::unexpected_message(
                        0,
                        MESSAGE_TYPE_NEW_EXTENDED_MINING_JOB,
                    )),
                },

                SetNewPrevHash(m) => {
                    self.handle_set_new_prev_hash(server_id, m, tlv_fields)
                        .await
                }

                SetCustomMiningJobSuccess(m) => match (channel_type, work_selection) {
                    (SupportedChannelTypes::Extended, true)
                    | (SupportedChannelTypes::GroupAndExtended, true) => {
                        self.handle_set_custom_mining_job_success(server_id, m, tlv_fields)
                            .await
                    }
                    _ => Err(Self::Error::unexpected_message(
                        0,
                        MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_SUCCESS,
                    )),
                },
                SetCustomMiningJobError(m) => match (channel_type, work_selection) {
                    (SupportedChannelTypes::Extended, true)
                    | (SupportedChannelTypes::Group, true)
                    | (SupportedChannelTypes::GroupAndExtended, true) => {
                        self.handle_set_custom_mining_job_error(server_id, m, tlv_fields)
                            .await
                    }
                    _ => Err(Self::Error::unexpected_message(
                        0,
                        MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_ERROR,
                    )),
                },

                SetTarget(m) => self.handle_set_target(server_id, m, tlv_fields).await,

                SetGroupChannel(m) => match channel_type {
                    SupportedChannelTypes::Group | SupportedChannelTypes::GroupAndExtended => {
                        self.handle_set_group_channel(server_id, m, tlv_fields)
                            .await
                    }
                    _ => Err(Self::Error::unexpected_message(
                        0,
                        MESSAGE_TYPE_SET_GROUP_CHANNEL,
                    )),
                },
                SubmitSharesExtended(_) => Err(Self::Error::unexpected_message(
                    0,
                    MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED,
                )),
                SubmitSharesStandard(_) => Err(Self::Error::unexpected_message(
                    0,
                    MESSAGE_TYPE_SUBMIT_SHARES_STANDARD,
                )),
                UpdateChannel(_) => Err(Self::Error::unexpected_message(
                    0,
                    MESSAGE_TYPE_UPDATE_CHANNEL,
                )),
                OpenExtendedMiningChannel(_) => Err(Self::Error::unexpected_message(
                    0,
                    MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL,
                )),
                OpenStandardMiningChannel(_) => Err(Self::Error::unexpected_message(
                    0,
                    MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL,
                )),
                SetCustomMiningJob(_) => Err(Self::Error::unexpected_message(
                    0,
                    MESSAGE_TYPE_SET_CUSTOM_MINING_JOB,
                )),
            }
        }
    }

    async fn handle_open_standard_mining_channel_success(
        &mut self,
        server_id: Option<usize>,
        msg: OpenStandardMiningChannelSuccess,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_open_extended_mining_channel_success(
        &mut self,
        server_id: Option<usize>,
        msg: OpenExtendedMiningChannelSuccess,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_open_mining_channel_error(
        &mut self,
        server_id: Option<usize>,
        msg: OpenMiningChannelError,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_update_channel_error(
        &mut self,
        server_id: Option<usize>,
        msg: UpdateChannelError,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_close_channel(
        &mut self,
        server_id: Option<usize>,
        msg: CloseChannel,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_set_extranonce_prefix(
        &mut self,
        server_id: Option<usize>,
        msg: SetExtranoncePrefix,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_submit_shares_success(
        &mut self,
        server_id: Option<usize>,
        msg: SubmitSharesSuccess,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_submit_shares_error(
        &mut self,
        server_id: Option<usize>,
        msg: SubmitSharesError,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_new_mining_job(
        &mut self,
        server_id: Option<usize>,
        msg: NewMiningJob,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_new_extended_mining_job(
        &mut self,
        server_id: Option<usize>,
        msg: NewExtendedMiningJob,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_set_new_prev_hash(
        &mut self,
        server_id: Option<usize>,
        msg: SetNewPrevHash,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_set_custom_mining_job_success(
        &mut self,
        server_id: Option<usize>,
        msg: SetCustomMiningJobSuccess,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_set_custom_mining_job_error(
        &mut self,
        server_id: Option<usize>,
        msg: SetCustomMiningJobError,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_set_target(
        &mut self,
        server_id: Option<usize>,
        msg: SetTarget,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_set_group_channel(
        &mut self,
        server_id: Option<usize>,
        msg: SetGroupChannel,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;
}

/// Synchronous handler trait for processing mining messages received from clients.
///
/// The client ID identifies which client a message originated from.
/// Whether this is relevant or not depends on which object is implementing the trait, and whether
/// this contextual information is readily available or not. In cases where `client_id` is either
/// irrelevant or can be inferred without the context, this should always be `None`.
///
/// Handler methods receive an optional `tlv_data` parameter containing validated TLV fields
/// when extension data is appended to messages. It will be `Some` only if validation succeeds
/// against negotiated extensions.
pub trait HandleMiningMessagesFromClientSync {
    type Error: HandlerErrorType;

    fn get_channel_type_for_client(&self, client_id: Option<usize>) -> SupportedChannelTypes;
    fn is_work_selection_enabled_for_client(&self, client_id: Option<usize>) -> bool;
    fn is_client_authorized(
        &self,
        client_id: Option<usize>,
        user_identity: &Str0255,
    ) -> Result<bool, Self::Error>;

    /// Returns the list of negotiated extension_types with a client.
    ///
    /// Return an empty Vec if no extensions have been negotiated.
    fn get_negotiated_extensions_with_client(
        &self,
        client_id: Option<usize>,
    ) -> Result<Vec<u16>, Self::Error>;

    /// Handles a raw Mining protocol message frame from a client.
    ///
    /// This method parses the raw frame, extracts any TLV extension data, and delegates
    /// to `handle_mining_message_from_client` with the parsed message and TLV fields.
    fn handle_mining_message_frame_from_client(
        &mut self,
        client_id: Option<usize>,
        header: Header,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        let negotiated_extensions = self.get_negotiated_extensions_with_client(client_id)?;
        if negotiated_extensions.is_empty() {
            let parsed: Mining<'_> = (header.msg_type(), payload)
                .try_into()
                .map_err(Self::Error::parse_error)?;
            return self.handle_mining_message_from_client(client_id, parsed, None);
        }
        let (parsed, tlv_fields) =
            parse_message_frame_with_tlvs(header, payload, &negotiated_extensions)
                .map_err(Self::Error::parse_error)?;
        match parsed {
            AnyMessage::Mining(parsed) => {
                self.handle_mining_message_from_client(client_id, parsed, tlv_fields.as_deref())
            }
            _ => Err(Self::Error::unexpected_message(
                header.ext_type_without_channel_msg(),
                header.msg_type(),
            )),
        }
    }

    /// Handles a parsed mining message from a client.
    ///
    /// The `tlv_data` parameter contains validated TLV fields if the message has extension
    /// data appended. It will be `Some` only if TLV validation succeeds against negotiated
    /// extensions, otherwise `None`. Use `extensions_sv2` helper functions to extract
    /// specific extension data (e.g., `extract_worker_identity_from_submit_shares`).
    fn handle_mining_message_from_client(
        &mut self,
        client_id: Option<usize>,
        message: Mining,
        tlv_fields: Option<&[Tlv]>,
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
                    self.handle_open_standard_mining_channel(client_id, m, tlv_fields)
                }
                SupportedChannelTypes::Extended => Err(Self::Error::unexpected_message(
                    0,
                    MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL,
                )),
            },
            OpenExtendedMiningChannel(m) => match channel_type {
                SupportedChannelTypes::Extended | SupportedChannelTypes::GroupAndExtended => {
                    self.handle_open_extended_mining_channel(client_id, m, tlv_fields)
                }
                _ => Err(Self::Error::unexpected_message(
                    0,
                    MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL,
                )),
            },
            UpdateChannel(m) => self.handle_update_channel(client_id, m, tlv_fields),

            SubmitSharesStandard(m) => match channel_type {
                SupportedChannelTypes::Standard
                | SupportedChannelTypes::Group
                | SupportedChannelTypes::GroupAndExtended => {
                    self.handle_submit_shares_standard(client_id, m, tlv_fields)
                }
                SupportedChannelTypes::Extended => Err(Self::Error::unexpected_message(
                    0,
                    MESSAGE_TYPE_SUBMIT_SHARES_STANDARD,
                )),
            },
            SubmitSharesExtended(m) => match channel_type {
                SupportedChannelTypes::Extended | SupportedChannelTypes::GroupAndExtended => {
                    self.handle_submit_shares_extended(client_id, m, tlv_fields)
                }
                _ => Err(Self::Error::unexpected_message(
                    0,
                    MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED,
                )),
            },
            SetCustomMiningJob(m) => match (channel_type, work_selection) {
                (SupportedChannelTypes::Extended, true)
                | (SupportedChannelTypes::GroupAndExtended, true) => {
                    self.handle_set_custom_mining_job(client_id, m, tlv_fields)
                }
                _ => Err(Self::Error::unexpected_message(
                    0,
                    MESSAGE_TYPE_SET_CUSTOM_MINING_JOB,
                )),
            },
            CloseChannel(m) => self.handle_close_channel(client_id, m, tlv_fields),
            NewExtendedMiningJob(_) => Err(Self::Error::unexpected_message(
                0,
                MESSAGE_TYPE_NEW_EXTENDED_MINING_JOB,
            )),
            NewMiningJob(_) => Err(Self::Error::unexpected_message(
                0,
                MESSAGE_TYPE_NEW_MINING_JOB,
            )),
            OpenExtendedMiningChannelSuccess(_) => Err(Self::Error::unexpected_message(
                0,
                MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCESS,
            )),
            OpenMiningChannelError(_) => Err(Self::Error::unexpected_message(
                0,
                MESSAGE_TYPE_OPEN_MINING_CHANNEL_ERROR,
            )),
            OpenStandardMiningChannelSuccess(_) => Err(Self::Error::unexpected_message(
                0,
                MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS,
            )),
            SetCustomMiningJobError(_) => Err(Self::Error::unexpected_message(
                0,
                MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_ERROR,
            )),
            SetCustomMiningJobSuccess(_) => Err(Self::Error::unexpected_message(
                0,
                MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_SUCCESS,
            )),
            SetExtranoncePrefix(_) => Err(Self::Error::unexpected_message(
                0,
                MESSAGE_TYPE_SET_EXTRANONCE_PREFIX,
            )),
            SetGroupChannel(_) => Err(Self::Error::unexpected_message(
                0,
                MESSAGE_TYPE_SET_GROUP_CHANNEL,
            )),
            SetNewPrevHash(_) => Err(Self::Error::unexpected_message(
                0,
                MESSAGE_TYPE_MINING_SET_NEW_PREV_HASH,
            )),
            SetTarget(_) => Err(Self::Error::unexpected_message(0, MESSAGE_TYPE_SET_TARGET)),
            SubmitSharesError(_) => Err(Self::Error::unexpected_message(
                0,
                MESSAGE_TYPE_SUBMIT_SHARES_ERROR,
            )),
            SubmitSharesSuccess(_) => Err(Self::Error::unexpected_message(
                0,
                MESSAGE_TYPE_SUBMIT_SHARES_SUCCESS,
            )),
            UpdateChannelError(_) => Err(Self::Error::unexpected_message(
                0,
                MESSAGE_TYPE_UPDATE_CHANNEL_ERROR,
            )),
        }
    }

    fn handle_close_channel(
        &mut self,
        client_id: Option<usize>,
        msg: CloseChannel,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_open_standard_mining_channel(
        &mut self,
        client_id: Option<usize>,
        msg: OpenStandardMiningChannel,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_open_extended_mining_channel(
        &mut self,
        client_id: Option<usize>,
        msg: OpenExtendedMiningChannel,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_update_channel(
        &mut self,
        client_id: Option<usize>,
        msg: UpdateChannel,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_submit_shares_standard(
        &mut self,
        client_id: Option<usize>,
        msg: SubmitSharesStandard,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_submit_shares_extended(
        &mut self,
        client_id: Option<usize>,
        msg: SubmitSharesExtended,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_set_custom_mining_job(
        &mut self,
        client_id: Option<usize>,
        msg: SetCustomMiningJob,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;
}

/// Asynchronous handler trait for processing mining messages received from clients.
///
/// The client ID identifies which client a message originated from.
/// Whether this is relevant or not depends on which object is implementing the trait, and whether
/// this contextual information is readily available or not. In cases where `client_id` is either
/// irrelevant or can be inferred without the context, this should always be `None`.
///
/// Handler methods receive an optional `tlv_data` parameter containing validated TLV fields
/// when extension data is appended to messages. It will be `Some` only if validation succeeds
/// against negotiated extensions.
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

    /// Returns the list of negotiated extension types for a client.
    ///
    /// Return an empty Vec if no extensions have been negotiated.
    fn get_negotiated_extensions_with_client(
        &self,
        client_id: Option<usize>,
    ) -> Result<Vec<u16>, Self::Error>;

    /// Handles a raw Mining protocol message frame from a client.
    ///
    /// This method parses the raw frame, extracts any TLV extension data, and delegates
    /// to `handle_mining_message_from_client` with the parsed message and TLV fields.
    async fn handle_mining_message_frame_from_client(
        &mut self,
        client_id: Option<usize>,
        header: Header,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        async move {
            let negotiated_extensions = self.get_negotiated_extensions_with_client(client_id)?;
            if negotiated_extensions.is_empty() {
                let parsed: Mining<'_> = (header.msg_type(), payload)
                    .try_into()
                    .map_err(Self::Error::parse_error)?;
                return self
                    .handle_mining_message_from_client(client_id, parsed, None)
                    .await;
            }
            let (parsed, tlv_fields) =
                parse_message_frame_with_tlvs(header, payload, &negotiated_extensions)
                    .map_err(Self::Error::parse_error)?;
            match parsed {
                AnyMessage::Mining(parsed) => {
                    self.handle_mining_message_from_client(client_id, parsed, tlv_fields.as_deref())
                        .await
                }
                _ => Err(Self::Error::unexpected_message(
                    header.ext_type_without_channel_msg(),
                    header.msg_type(),
                )),
            }
        }
    }

    /// Handles a parsed Mining protocol message from a client.
    ///
    /// The `tlv_fields` parameter contains parsed TLV fields if the message has extension
    /// data appended. It will be `Some(&[Tlv])` when valid TLV data is present, or `None`
    /// if no TLV data exists or validation fails. Each `Tlv` struct provides direct access to
    /// `extension_type`, `field_type`, `length`, and `value`.
    async fn handle_mining_message_from_client(
        &mut self,
        client_id: Option<usize>,
        message: Mining,
        tlv_fields: Option<&[Tlv]>,
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
                        self.handle_open_standard_mining_channel(client_id, m, tlv_fields)
                            .await
                    }
                    SupportedChannelTypes::Extended => Err(Self::Error::unexpected_message(
                        0,
                        MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL,
                    )),
                },
                OpenExtendedMiningChannel(m) => match channel_type {
                    SupportedChannelTypes::Extended | SupportedChannelTypes::GroupAndExtended => {
                        self.handle_open_extended_mining_channel(client_id, m, tlv_fields)
                            .await
                    }
                    _ => Err(Self::Error::unexpected_message(
                        0,
                        MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL,
                    )),
                },
                UpdateChannel(m) => self.handle_update_channel(client_id, m, tlv_fields).await,

                SubmitSharesStandard(m) => match channel_type {
                    SupportedChannelTypes::Standard
                    | SupportedChannelTypes::Group
                    | SupportedChannelTypes::GroupAndExtended => {
                        self.handle_submit_shares_standard(client_id, m, tlv_fields)
                            .await
                    }
                    SupportedChannelTypes::Extended => Err(Self::Error::unexpected_message(
                        0,
                        MESSAGE_TYPE_SUBMIT_SHARES_STANDARD,
                    )),
                },
                SubmitSharesExtended(m) => match channel_type {
                    SupportedChannelTypes::Extended | SupportedChannelTypes::GroupAndExtended => {
                        self.handle_submit_shares_extended(client_id, m, tlv_fields)
                            .await
                    }
                    _ => Err(Self::Error::unexpected_message(
                        0,
                        MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED,
                    )),
                },
                SetCustomMiningJob(m) => match (channel_type, work_selection) {
                    (SupportedChannelTypes::Extended, true)
                    | (SupportedChannelTypes::GroupAndExtended, true) => {
                        self.handle_set_custom_mining_job(client_id, m, tlv_fields)
                            .await
                    }
                    _ => Err(Self::Error::unexpected_message(
                        0,
                        MESSAGE_TYPE_SET_CUSTOM_MINING_JOB,
                    )),
                },
                CloseChannel(m) => self.handle_close_channel(client_id, m, tlv_fields).await,
                NewExtendedMiningJob(_) => Err(Self::Error::unexpected_message(
                    0,
                    MESSAGE_TYPE_NEW_EXTENDED_MINING_JOB,
                )),
                NewMiningJob(_) => Err(Self::Error::unexpected_message(
                    0,
                    MESSAGE_TYPE_NEW_MINING_JOB,
                )),
                OpenExtendedMiningChannelSuccess(_) => Err(Self::Error::unexpected_message(
                    0,
                    MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCESS,
                )),
                OpenMiningChannelError(_) => Err(Self::Error::unexpected_message(
                    0,
                    MESSAGE_TYPE_OPEN_MINING_CHANNEL_ERROR,
                )),
                OpenStandardMiningChannelSuccess(_) => Err(Self::Error::unexpected_message(
                    0,
                    MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS,
                )),
                SetCustomMiningJobError(_) => Err(Self::Error::unexpected_message(
                    0,
                    MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_ERROR,
                )),
                SetCustomMiningJobSuccess(_) => Err(Self::Error::unexpected_message(
                    0,
                    MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_SUCCESS,
                )),
                SetExtranoncePrefix(_) => Err(Self::Error::unexpected_message(
                    0,
                    MESSAGE_TYPE_SET_EXTRANONCE_PREFIX,
                )),
                SetGroupChannel(_) => Err(Self::Error::unexpected_message(
                    0,
                    MESSAGE_TYPE_SET_GROUP_CHANNEL,
                )),
                SetNewPrevHash(_) => Err(Self::Error::unexpected_message(
                    0,
                    MESSAGE_TYPE_MINING_SET_NEW_PREV_HASH,
                )),
                SetTarget(_) => Err(Self::Error::unexpected_message(0, MESSAGE_TYPE_SET_TARGET)),
                SubmitSharesError(_) => Err(Self::Error::unexpected_message(
                    0,
                    MESSAGE_TYPE_SUBMIT_SHARES_ERROR,
                )),
                SubmitSharesSuccess(_) => Err(Self::Error::unexpected_message(
                    0,
                    MESSAGE_TYPE_SUBMIT_SHARES_SUCCESS,
                )),
                UpdateChannelError(_) => Err(Self::Error::unexpected_message(
                    0,
                    MESSAGE_TYPE_UPDATE_CHANNEL_ERROR,
                )),
            }
        }
    }

    async fn handle_close_channel(
        &mut self,
        client_id: Option<usize>,
        msg: CloseChannel,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_open_standard_mining_channel(
        &mut self,
        client_id: Option<usize>,
        msg: OpenStandardMiningChannel,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_open_extended_mining_channel(
        &mut self,
        client_id: Option<usize>,
        msg: OpenExtendedMiningChannel,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_update_channel(
        &mut self,
        client_id: Option<usize>,
        msg: UpdateChannel,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_submit_shares_standard(
        &mut self,
        client_id: Option<usize>,
        msg: SubmitSharesStandard,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_submit_shares_extended(
        &mut self,
        client_id: Option<usize>,
        msg: SubmitSharesExtended,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_set_custom_mining_job(
        &mut self,
        client_id: Option<usize>,
        msg: SetCustomMiningJob,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;
}
