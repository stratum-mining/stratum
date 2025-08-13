use crate::error::HandlerError as Error;
use common_messages_sv2::{
    ChannelEndpointChanged, Reconnect, SetupConnectionError, SetupConnectionSuccess, *,
};
use core::convert::TryInto;
use parsers_sv2::CommonMessages;

pub trait HandleCommonMessagesFromServerSync {
    fn handle_common_message_server(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Error> {
        let parsed: CommonMessages<'_> = (message_type, payload).try_into()?;
        self.dispatch_common_message_server(parsed)
    }

    fn dispatch_common_message_server(&mut self, message: CommonMessages<'_>) -> Result<(), Error> {
        match message {
            CommonMessages::SetupConnectionSuccess(msg) => {
                self.handle_setup_connection_success(msg)
            }
            CommonMessages::SetupConnectionError(msg) => self.handle_setup_connection_error(msg),
            CommonMessages::ChannelEndpointChanged(msg) => {
                self.handle_channel_endpoint_changed(msg)
            }
            CommonMessages::Reconnect(msg) => self.handle_reconnect(msg),

            CommonMessages::SetupConnection(_) => {
                Err(Error::UnexpectedMessage(MESSAGE_TYPE_SETUP_CONNECTION))
            }
        }
    }

    fn handle_setup_connection_success(&mut self, msg: SetupConnectionSuccess)
        -> Result<(), Error>;

    fn handle_setup_connection_error(&mut self, msg: SetupConnectionError) -> Result<(), Error>;

    fn handle_channel_endpoint_changed(&mut self, msg: ChannelEndpointChanged)
        -> Result<(), Error>;

    fn handle_reconnect(&mut self, msg: Reconnect) -> Result<(), Error>;
}

#[trait_variant::make(Send)]
pub trait HandleCommonMessagesFromServerAsync {
    async fn handle_common_message_server(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Error> {
        let parsed: Result<CommonMessages<'_>, _> = (message_type, payload).try_into();
        async move {
            let parsed = parsed?;
            self.dispatch_common_message_server(parsed).await
        }
    }

    async fn dispatch_common_message_server(
        &mut self,
        message: CommonMessages<'_>,
    ) -> Result<(), Error> {
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

                CommonMessages::SetupConnection(_) => {
                    Err(Error::UnexpectedMessage(MESSAGE_TYPE_SETUP_CONNECTION))
                }
            }
        }
    }

    async fn handle_setup_connection_success(
        &mut self,
        msg: SetupConnectionSuccess,
    ) -> Result<(), Error>;

    async fn handle_setup_connection_error(
        &mut self,
        msg: SetupConnectionError,
    ) -> Result<(), Error>;

    async fn handle_channel_endpoint_changed(
        &mut self,
        msg: ChannelEndpointChanged,
    ) -> Result<(), Error>;

    async fn handle_reconnect(&mut self, msg: Reconnect) -> Result<(), Error>;
}

pub trait HandleCommonMessagesFromClientSync {
    fn handle_common_message_client(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Error> {
        let parsed: CommonMessages<'_> = (message_type, payload).try_into()?;
        self.dispatch_common_message_client(parsed)
    }

    fn dispatch_common_message_client(&mut self, message: CommonMessages<'_>) -> Result<(), Error> {
        match message {
            CommonMessages::SetupConnectionSuccess(_) => Err(Error::UnexpectedMessage(
                MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
            )),
            CommonMessages::SetupConnectionError(_) => Err(Error::UnexpectedMessage(
                MESSAGE_TYPE_SETUP_CONNECTION_ERROR,
            )),
            CommonMessages::ChannelEndpointChanged(_) => Err(Error::UnexpectedMessage(
                MESSAGE_TYPE_CHANNEL_ENDPOINT_CHANGED,
            )),
            CommonMessages::Reconnect(_) => Err(Error::UnexpectedMessage(MESSAGE_TYPE_RECONNECT)),

            CommonMessages::SetupConnection(msg) => self.handle_setup_connection(msg),
        }
    }

    fn handle_setup_connection(&mut self, msg: SetupConnection) -> Result<(), Error>;
}

#[trait_variant::make(Send)]
pub trait HandleCommonMessagesFromClientAsync {
    async fn handle_common_message_client(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Error> {
        let parsed: Result<CommonMessages<'_>, _> = (message_type, payload).try_into();
        async move {
            let parsed = parsed?;
            self.dispatch_common_message_client(parsed).await
        }
    }

    async fn dispatch_common_message_client(
        &mut self,
        message: CommonMessages<'_>,
    ) -> Result<(), Error> {
        async move {
            match message {
                CommonMessages::SetupConnectionSuccess(_) => Err(Error::UnexpectedMessage(
                    MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
                )),
                CommonMessages::SetupConnectionError(_) => Err(Error::UnexpectedMessage(
                    MESSAGE_TYPE_SETUP_CONNECTION_ERROR,
                )),
                CommonMessages::ChannelEndpointChanged(_) => Err(Error::UnexpectedMessage(
                    MESSAGE_TYPE_CHANNEL_ENDPOINT_CHANGED,
                )),
                CommonMessages::Reconnect(_) => {
                    Err(Error::UnexpectedMessage(MESSAGE_TYPE_RECONNECT))
                }
                CommonMessages::SetupConnection(msg) => self.handle_setup_connection(msg).await,
            }
        }
    }

    async fn handle_setup_connection(&mut self, msg: SetupConnection) -> Result<(), Error>;
}
