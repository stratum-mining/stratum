use common_messages_sv2::{
    ChannelEndpointChanged, Reconnect, SetupConnectionError, SetupConnectionSuccess, *,
};
use core::convert::TryInto;
use parsers_sv2::CommonMessages;

use crate::error::HandlerErrorType;

pub trait HandleCommonMessagesFromServerSync {
    type Error: HandlerErrorType;
    fn handle_common_message_frame_from_server(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
        server_id: usize,
    ) -> Result<(), Self::Error> {
        let parsed: CommonMessages<'_> = (message_type, payload)
            .try_into()
            .map_err(Self::Error::parse_error)?;
        self.handle_common_message_from_server(parsed, server_id)
    }

    fn handle_common_message_from_server(
        &mut self,
        message: CommonMessages<'_>,
        server_id: usize,
    ) -> Result<(), Self::Error> {
        match message {
            CommonMessages::SetupConnectionSuccess(msg) => {
                self.handle_setup_connection_success(msg, server_id)
            }
            CommonMessages::SetupConnectionError(msg) => {
                self.handle_setup_connection_error(msg, server_id)
            }
            CommonMessages::ChannelEndpointChanged(msg) => {
                self.handle_channel_endpoint_changed(msg, server_id)
            }
            CommonMessages::Reconnect(msg) => self.handle_reconnect(msg, server_id),

            CommonMessages::SetupConnection(_) => Err(Self::Error::unexpected_message(
                MESSAGE_TYPE_SETUP_CONNECTION,
            )),
        }
    }

    fn handle_setup_connection_success(
        &mut self,
        msg: SetupConnectionSuccess,
        server_id: usize,
    ) -> Result<(), Self::Error>;

    fn handle_setup_connection_error(
        &mut self,
        msg: SetupConnectionError,
        server_id: usize,
    ) -> Result<(), Self::Error>;

    fn handle_channel_endpoint_changed(
        &mut self,
        msg: ChannelEndpointChanged,
        server_id: usize,
    ) -> Result<(), Self::Error>;

    fn handle_reconnect(&mut self, msg: Reconnect, server_id: usize) -> Result<(), Self::Error>;
}

#[trait_variant::make(Send)]
pub trait HandleCommonMessagesFromServerAsync {
    type Error: HandlerErrorType;
    async fn handle_common_message_frame_from_server(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
        server_id: usize,
    ) -> Result<(), Self::Error> {
        async move {
            let parsed: CommonMessages<'_> = (message_type, payload)
                .try_into()
                .map_err(Self::Error::parse_error)?;
            self.handle_common_message_from_server(parsed, server_id)
                .await
        }
    }

    async fn handle_common_message_from_server(
        &mut self,
        message: CommonMessages<'_>,
        server_id: usize,
    ) -> Result<(), Self::Error> {
        async move {
            match message {
                CommonMessages::SetupConnectionSuccess(msg) => {
                    self.handle_setup_connection_success(msg, server_id).await
                }
                CommonMessages::SetupConnectionError(msg) => {
                    self.handle_setup_connection_error(msg, server_id).await
                }
                CommonMessages::ChannelEndpointChanged(msg) => {
                    self.handle_channel_endpoint_changed(msg, server_id).await
                }
                CommonMessages::Reconnect(msg) => self.handle_reconnect(msg, server_id).await,

                CommonMessages::SetupConnection(_) => Err(Self::Error::unexpected_message(
                    MESSAGE_TYPE_SETUP_CONNECTION,
                )),
            }
        }
    }

    async fn handle_setup_connection_success(
        &mut self,
        msg: SetupConnectionSuccess,
        server_id: usize,
    ) -> Result<(), Self::Error>;

    async fn handle_setup_connection_error(
        &mut self,
        msg: SetupConnectionError,
        server_id: usize,
    ) -> Result<(), Self::Error>;

    async fn handle_channel_endpoint_changed(
        &mut self,
        msg: ChannelEndpointChanged,
        server_id: usize,
    ) -> Result<(), Self::Error>;

    async fn handle_reconnect(
        &mut self,
        msg: Reconnect,
        server_id: usize,
    ) -> Result<(), Self::Error>;
}

pub trait HandleCommonMessagesFromClientSync {
    type Error: HandlerErrorType;
    fn handle_common_message_frame_from_client(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
        client_id: usize,
    ) -> Result<(), Self::Error> {
        let parsed: CommonMessages<'_> = (message_type, payload)
            .try_into()
            .map_err(Self::Error::parse_error)?;
        self.handle_common_message_from_client(parsed, client_id)
    }

    fn handle_common_message_from_client(
        &mut self,
        message: CommonMessages<'_>,
        client_id: usize,
    ) -> Result<(), Self::Error> {
        match message {
            CommonMessages::SetupConnectionSuccess(_) => Err(Self::Error::unexpected_message(
                MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
            )),
            CommonMessages::SetupConnectionError(_) => Err(Self::Error::unexpected_message(
                MESSAGE_TYPE_SETUP_CONNECTION_ERROR,
            )),
            CommonMessages::ChannelEndpointChanged(_) => Err(Self::Error::unexpected_message(
                MESSAGE_TYPE_CHANNEL_ENDPOINT_CHANGED,
            )),
            CommonMessages::Reconnect(_) => {
                Err(Self::Error::unexpected_message(MESSAGE_TYPE_RECONNECT))
            }

            CommonMessages::SetupConnection(msg) => self.handle_setup_connection(msg, client_id),
        }
    }

    fn handle_setup_connection(
        &mut self,
        msg: SetupConnection,
        client_id: usize,
    ) -> Result<(), Self::Error>;
}

#[trait_variant::make(Send)]
pub trait HandleCommonMessagesFromClientAsync {
    type Error: HandlerErrorType;
    async fn handle_common_message_frame_from_client(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
        client_id: usize,
    ) -> Result<(), Self::Error> {
        async move {
            let parsed: CommonMessages<'_> = (message_type, payload)
                .try_into()
                .map_err(Self::Error::parse_error)?;
            self.handle_common_message_from_client(parsed, client_id)
                .await
        }
    }

    async fn handle_common_message_from_client(
        &mut self,
        message: CommonMessages<'_>,
        client_id: usize,
    ) -> Result<(), Self::Error> {
        async move {
            match message {
                CommonMessages::SetupConnectionSuccess(_) => Err(Self::Error::unexpected_message(
                    MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
                )),
                CommonMessages::SetupConnectionError(_) => Err(Self::Error::unexpected_message(
                    MESSAGE_TYPE_SETUP_CONNECTION_ERROR,
                )),
                CommonMessages::ChannelEndpointChanged(_) => Err(Self::Error::unexpected_message(
                    MESSAGE_TYPE_CHANNEL_ENDPOINT_CHANGED,
                )),
                CommonMessages::Reconnect(_) => {
                    Err(Self::Error::unexpected_message(MESSAGE_TYPE_RECONNECT))
                }
                CommonMessages::SetupConnection(msg) => {
                    self.handle_setup_connection(msg, client_id).await
                }
            }
        }
    }

    async fn handle_setup_connection(
        &mut self,
        msg: SetupConnection,
        client_id: usize,
    ) -> Result<(), Self::Error>;
}
