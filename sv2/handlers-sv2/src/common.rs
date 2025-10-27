use common_messages_sv2::{
    ChannelEndpointChanged, Reconnect, SetupConnectionError, SetupConnectionSuccess, *,
};
use core::convert::TryInto;
use parsers_sv2::CommonMessages;

use crate::error::HandlerErrorType;

/// Synchronous handler trait for processing common messages received from servers.
///
/// The server ID identifies which server a message originated from.
/// Whether this is relevant or not depends on which object is implementing the trait, and whether
/// this contextual information is readily available or not. In cases where `server_id` is either
/// irrelevant or can be inferred without the context, this should always be `None`.
pub trait HandleCommonMessagesFromServerSync {
    type Error: HandlerErrorType;
    fn handle_common_message_frame_from_server(
        &mut self,
        server_id: Option<usize>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        let parsed: CommonMessages<'_> = (message_type, payload)
            .try_into()
            .map_err(Self::Error::parse_error)?;
        self.handle_common_message_from_server(server_id, parsed)
    }

    fn handle_common_message_from_server(
        &mut self,
        server_id: Option<usize>,
        message: CommonMessages<'_>,
    ) -> Result<(), Self::Error> {
        match message {
            CommonMessages::SetupConnectionSuccess(msg) => {
                self.handle_setup_connection_success(server_id, msg)
            }
            CommonMessages::SetupConnectionError(msg) => {
                self.handle_setup_connection_error(server_id, msg)
            }
            CommonMessages::ChannelEndpointChanged(msg) => {
                self.handle_channel_endpoint_changed(server_id, msg)
            }
            CommonMessages::Reconnect(msg) => self.handle_reconnect(server_id, msg),

            CommonMessages::SetupConnection(_) => Err(Self::Error::unexpected_message(
                MESSAGE_TYPE_SETUP_CONNECTION,
            )),
        }
    }

    fn handle_setup_connection_success(
        &mut self,
        server_id: Option<usize>,
        msg: SetupConnectionSuccess,
    ) -> Result<(), Self::Error>;

    fn handle_setup_connection_error(
        &mut self,
        server_id: Option<usize>,
        msg: SetupConnectionError,
    ) -> Result<(), Self::Error>;

    fn handle_channel_endpoint_changed(
        &mut self,
        server_id: Option<usize>,
        msg: ChannelEndpointChanged,
    ) -> Result<(), Self::Error>;

    fn handle_reconnect(
        &mut self,
        server_id: Option<usize>,
        msg: Reconnect,
    ) -> Result<(), Self::Error>;
}

/// Asynchronous handler trait for processing common messages received from servers.
///
/// The server ID identifies which server a message originated from.
/// Whether this is relevant or not depends on which object is implementing the trait, and whether
/// this contextual information is readily available or not. In cases where `server_id` is either
/// irrelevant or can be inferred without the context, this should always be `None`.
#[trait_variant::make(Send)]
pub trait HandleCommonMessagesFromServerAsync {
    type Error: HandlerErrorType;
    async fn handle_common_message_frame_from_server(
        &mut self,
        server_id: Option<usize>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        async move {
            let parsed: CommonMessages<'_> = (message_type, payload)
                .try_into()
                .map_err(Self::Error::parse_error)?;
            self.handle_common_message_from_server(server_id, parsed)
                .await
        }
    }

    async fn handle_common_message_from_server(
        &mut self,
        server_id: Option<usize>,
        message: CommonMessages<'_>,
    ) -> Result<(), Self::Error> {
        async move {
            match message {
                CommonMessages::SetupConnectionSuccess(msg) => {
                    self.handle_setup_connection_success(server_id, msg).await
                }
                CommonMessages::SetupConnectionError(msg) => {
                    self.handle_setup_connection_error(server_id, msg).await
                }
                CommonMessages::ChannelEndpointChanged(msg) => {
                    self.handle_channel_endpoint_changed(server_id, msg).await
                }
                CommonMessages::Reconnect(msg) => self.handle_reconnect(server_id, msg).await,

                CommonMessages::SetupConnection(_) => Err(Self::Error::unexpected_message(
                    MESSAGE_TYPE_SETUP_CONNECTION,
                )),
            }
        }
    }

    async fn handle_setup_connection_success(
        &mut self,
        server_id: Option<usize>,
        msg: SetupConnectionSuccess,
    ) -> Result<(), Self::Error>;

    async fn handle_setup_connection_error(
        &mut self,
        server_id: Option<usize>,
        msg: SetupConnectionError,
    ) -> Result<(), Self::Error>;

    async fn handle_channel_endpoint_changed(
        &mut self,
        server_id: Option<usize>,
        msg: ChannelEndpointChanged,
    ) -> Result<(), Self::Error>;

    async fn handle_reconnect(
        &mut self,
        server_id: Option<usize>,
        msg: Reconnect,
    ) -> Result<(), Self::Error>;
}

/// Synchronous handler trait for processing common messages received from clients.
///
/// The client ID identifies which client a message originated from.
/// Whether this is relevant or not depends on which object is implementing the trait, and whether
/// this contextual information is readily available or not. In cases where `client_id` is either
/// irrelevant or can be inferred without the context, this should always be `None`.
pub trait HandleCommonMessagesFromClientSync {
    type Error: HandlerErrorType;
    fn handle_common_message_frame_from_client(
        &mut self,
        client_id: Option<usize>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        let parsed: CommonMessages<'_> = (message_type, payload)
            .try_into()
            .map_err(Self::Error::parse_error)?;
        self.handle_common_message_from_client(client_id, parsed)
    }

    fn handle_common_message_from_client(
        &mut self,
        client_id: Option<usize>,
        message: CommonMessages<'_>,
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

            CommonMessages::SetupConnection(msg) => self.handle_setup_connection(client_id, msg),
        }
    }

    fn handle_setup_connection(
        &mut self,
        client_id: Option<usize>,
        msg: SetupConnection,
    ) -> Result<(), Self::Error>;
}

/// Asynchronous handler trait for processing common messages received from clients.
///
/// The client ID identifies which client a message originated from.
/// Whether this is relevant or not depends on which object is implementing the trait, and whether
/// this contextual information is readily available or not. In cases where `client_id` is either
/// irrelevant or can be inferred without the context, this should always be `None`.
#[trait_variant::make(Send)]
pub trait HandleCommonMessagesFromClientAsync {
    type Error: HandlerErrorType;
    async fn handle_common_message_frame_from_client(
        &mut self,
        client_id: Option<usize>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        async move {
            let parsed: CommonMessages<'_> = (message_type, payload)
                .try_into()
                .map_err(Self::Error::parse_error)?;
            self.handle_common_message_from_client(client_id, parsed)
                .await
        }
    }

    async fn handle_common_message_from_client(
        &mut self,
        client_id: Option<usize>,
        message: CommonMessages<'_>,
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
                    self.handle_setup_connection(client_id, msg).await
                }
            }
        }
    }

    async fn handle_setup_connection(
        &mut self,
        client_id: Option<usize>,
        msg: SetupConnection,
    ) -> Result<(), Self::Error>;
}
