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
    ) -> Result<(), Self::Error> {
        let parsed: CommonMessages<'_> = (message_type, payload)
            .try_into()
            .map_err(Self::Error::parse_error)?;
        self.handle_common_message_from_server(parsed)
    }

    fn handle_common_message_from_server(
        &mut self,
        message: CommonMessages<'_>,
    ) -> Result<(), Self::Error> {
        match message {
            CommonMessages::SetupConnectionSuccess(msg) => {
                self.handle_setup_connection_success(msg)
            }
            CommonMessages::SetupConnectionError(msg) => self.handle_setup_connection_error(msg),
            CommonMessages::ChannelEndpointChanged(msg) => {
                self.handle_channel_endpoint_changed(msg)
            }
            CommonMessages::Reconnect(msg) => self.handle_reconnect(msg),

            CommonMessages::SetupConnection(_) => Err(Self::Error::unexpected_message(
                MESSAGE_TYPE_SETUP_CONNECTION,
            )),
        }
    }

    fn handle_setup_connection_success(
        &mut self,
        msg: SetupConnectionSuccess,
    ) -> Result<(), Self::Error>;

    fn handle_setup_connection_error(
        &mut self,
        msg: SetupConnectionError,
    ) -> Result<(), Self::Error>;

    fn handle_channel_endpoint_changed(
        &mut self,
        msg: ChannelEndpointChanged,
    ) -> Result<(), Self::Error>;

    fn handle_reconnect(&mut self, msg: Reconnect) -> Result<(), Self::Error>;
}

#[trait_variant::make(Send)]
pub trait HandleCommonMessagesFromServerAsync {
    type Error: HandlerErrorType;
    async fn handle_common_message_frame_from_server(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        async move {
            let parsed: CommonMessages<'_> = (message_type, payload)
                .try_into()
                .map_err(Self::Error::parse_error)?;
            self.handle_common_message_from_server(parsed).await
        }
    }

    async fn handle_common_message_from_server(
        &mut self,
        message: CommonMessages<'_>,
    ) -> Result<(), Self::Error> {
        async move {
            match message {
                CommonMessages::SetupConnectionSuccess(msg) => {
                    self.handle_setup_connection_success(msg).await
                }
                CommonMessages::SetupConnectionError(msg) => {
                    self.handle_setup_connection_error(msg).await
                }
                CommonMessages::ChannelEndpointChanged(msg) => {
                    self.handle_channel_endpoint_changed(msg).await
                }
                CommonMessages::Reconnect(msg) => self.handle_reconnect(msg).await,

                CommonMessages::SetupConnection(_) => Err(Self::Error::unexpected_message(
                    MESSAGE_TYPE_SETUP_CONNECTION,
                )),
            }
        }
    }

    async fn handle_setup_connection_success(
        &mut self,
        msg: SetupConnectionSuccess,
    ) -> Result<(), Self::Error>;

    async fn handle_setup_connection_error(
        &mut self,
        msg: SetupConnectionError,
    ) -> Result<(), Self::Error>;

    async fn handle_channel_endpoint_changed(
        &mut self,
        msg: ChannelEndpointChanged,
    ) -> Result<(), Self::Error>;

    async fn handle_reconnect(&mut self, msg: Reconnect) -> Result<(), Self::Error>;
}

pub trait HandleCommonMessagesFromClientSync {
    type Error: HandlerErrorType;
    fn handle_common_message_frame_from_client(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        let parsed: CommonMessages<'_> = (message_type, payload)
            .try_into()
            .map_err(Self::Error::parse_error)?;
        self.handle_common_message_from_client(parsed)
    }

    fn handle_common_message_from_client(
        &mut self,
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

            CommonMessages::SetupConnection(msg) => self.handle_setup_connection(msg),
        }
    }

    fn handle_setup_connection(&mut self, msg: SetupConnection) -> Result<(), Self::Error>;
}

#[trait_variant::make(Send)]
pub trait HandleCommonMessagesFromClientAsync {
    type Error: HandlerErrorType;
    async fn handle_common_message_frame_from_client(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        async move {
            let parsed: CommonMessages<'_> = (message_type, payload)
                .try_into()
                .map_err(Self::Error::parse_error)?;
            self.handle_common_message_from_client(parsed).await
        }
    }

    async fn handle_common_message_from_client(
        &mut self,
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
                CommonMessages::SetupConnection(msg) => self.handle_setup_connection(msg).await,
            }
        }
    }

    async fn handle_setup_connection(&mut self, msg: SetupConnection) -> Result<(), Self::Error>;
}
