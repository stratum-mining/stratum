use core::convert::TryInto;
use extensions_sv2::{RequestExtensions, RequestExtensionsError, RequestExtensionsSuccess};
use parsers_sv2::Extensions;

use crate::error::HandlerErrorType;

/// Synchronous handler trait for processing extension messages received from servers.
///
/// The server ID identifies which server a message originated from.
/// Whether this is relevant or not depends on which object is implementing the trait, and whether
/// this contextual information is readily available or not. In cases where `server_id` is either
/// irrelevant or can be inferred without the context, this should always be `None`.
pub trait HandleExtensionsFromServerSync {
    type Error: HandlerErrorType;

    fn handle_extensions_message_frame_from_server(
        &mut self,
        server_id: Option<usize>,
        extension_type: u16,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        let parsed: Extensions<'_> = (extension_type, message_type, payload)
            .try_into()
            .map_err(Self::Error::parse_error)?;
        self.handle_extensions_message_from_server(server_id, parsed)
    }

    fn handle_extensions_message_from_server(
        &mut self,
        server_id: Option<usize>,
        message: Extensions<'_>,
    ) -> Result<(), Self::Error> {
        match message {
            Extensions::RequestExtensionsSuccess(msg) => {
                self.handle_request_extensions_success(server_id, msg)
            }
            Extensions::RequestExtensionsError(msg) => {
                self.handle_request_extensions_error(server_id, msg)
            }
            // RequestExtensions is sent by client, not server
            Extensions::RequestExtensions(_) => Err(Self::Error::unexpected_message(
                extensions_sv2::MESSAGE_TYPE_REQUEST_EXTENSIONS,
            )),
        }
    }

    fn handle_request_extensions_success(
        &mut self,
        server_id: Option<usize>,
        msg: RequestExtensionsSuccess,
    ) -> Result<(), Self::Error>;

    fn handle_request_extensions_error(
        &mut self,
        server_id: Option<usize>,
        msg: RequestExtensionsError,
    ) -> Result<(), Self::Error>;
}

/// Asynchronous handler trait for processing extension messages received from servers.
///
/// The server ID identifies which server a message originated from.
/// Whether this is relevant or not depends on which object is implementing the trait, and whether
/// this contextual information is readily available or not. In cases where `server_id` is either
/// irrelevant or can be inferred without the context, this should always be `None`.
#[trait_variant::make(Send)]
pub trait HandleExtensionsFromServerAsync {
    type Error: HandlerErrorType;

    async fn handle_extensions_message_frame_from_server(
        &mut self,
        server_id: Option<usize>,
        extension_type: u16,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        async move {
            let parsed: Extensions<'_> = (extension_type, message_type, payload)
                .try_into()
                .map_err(Self::Error::parse_error)?;
            self.handle_extensions_message_from_server(server_id, parsed)
                .await
        }
    }

    async fn handle_extensions_message_from_server(
        &mut self,
        server_id: Option<usize>,
        message: Extensions<'_>,
    ) -> Result<(), Self::Error> {
        async move {
            match message {
                Extensions::RequestExtensionsSuccess(msg) => {
                    self.handle_request_extensions_success(server_id, msg).await
                }
                Extensions::RequestExtensionsError(msg) => {
                    self.handle_request_extensions_error(server_id, msg).await
                }
                // RequestExtensions is sent by client, not server
                Extensions::RequestExtensions(_) => Err(Self::Error::unexpected_message(
                    extensions_sv2::MESSAGE_TYPE_REQUEST_EXTENSIONS,
                )),
            }
        }
    }

    async fn handle_request_extensions_success(
        &mut self,
        server_id: Option<usize>,
        msg: RequestExtensionsSuccess,
    ) -> Result<(), Self::Error>;

    async fn handle_request_extensions_error(
        &mut self,
        server_id: Option<usize>,
        msg: RequestExtensionsError,
    ) -> Result<(), Self::Error>;
}

/// Synchronous handler trait for processing extension messages received from clients.
///
/// The client ID identifies which client a message originated from.
/// Whether this is relevant or not depends on which object is implementing the trait, and whether
/// this contextual information is readily available or not. In cases where `client_id` is either
/// irrelevant or can be inferred without the context, this should always be `None`.
pub trait HandleExtensionsFromClientSync {
    type Error: HandlerErrorType;

    fn handle_extensions_message_frame_from_client(
        &mut self,
        client_id: Option<usize>,
        extension_type: u16,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        let parsed: Extensions<'_> = (extension_type, message_type, payload)
            .try_into()
            .map_err(Self::Error::parse_error)?;
        self.handle_extensions_message_from_client(client_id, parsed)
    }

    fn handle_extensions_message_from_client(
        &mut self,
        client_id: Option<usize>,
        message: Extensions<'_>,
    ) -> Result<(), Self::Error> {
        match message {
            Extensions::RequestExtensions(msg) => self.handle_request_extensions(client_id, msg),
            // Success/Error are sent by server, not client
            Extensions::RequestExtensionsSuccess(_) => Err(Self::Error::unexpected_message(
                extensions_sv2::MESSAGE_TYPE_REQUEST_EXTENSIONS_SUCCESS,
            )),
            Extensions::RequestExtensionsError(_) => Err(Self::Error::unexpected_message(
                extensions_sv2::MESSAGE_TYPE_REQUEST_EXTENSIONS_ERROR,
            )),
        }
    }

    fn handle_request_extensions(
        &mut self,
        client_id: Option<usize>,
        msg: RequestExtensions,
    ) -> Result<(), Self::Error>;
}

/// Asynchronous handler trait for processing extension messages received from clients.
///
/// The client ID identifies which client a message originated from.
/// Whether this is relevant or not depends on which object is implementing the trait, and whether
/// this contextual information is readily available or not. In cases where `client_id` is either
/// irrelevant or can be inferred without the context, this should always be `None`.
#[trait_variant::make(Send)]
pub trait HandleExtensionsFromClientAsync {
    type Error: HandlerErrorType;

    async fn handle_extensions_message_frame_from_client(
        &mut self,
        client_id: Option<usize>,
        extension_type: u16,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        async move {
            let parsed: Extensions<'_> = (extension_type, message_type, payload)
                .try_into()
                .map_err(Self::Error::parse_error)?;
            self.handle_extensions_message_from_client(client_id, parsed)
                .await
        }
    }

    async fn handle_extensions_message_from_client(
        &mut self,
        client_id: Option<usize>,
        message: Extensions<'_>,
    ) -> Result<(), Self::Error> {
        async move {
            match message {
                Extensions::RequestExtensions(msg) => {
                    self.handle_request_extensions(client_id, msg).await
                }
                // Success/Error are sent by server, not client
                Extensions::RequestExtensionsSuccess(_) => Err(Self::Error::unexpected_message(
                    extensions_sv2::MESSAGE_TYPE_REQUEST_EXTENSIONS_SUCCESS,
                )),
                Extensions::RequestExtensionsError(_) => Err(Self::Error::unexpected_message(
                    extensions_sv2::MESSAGE_TYPE_REQUEST_EXTENSIONS_ERROR,
                )),
            }
        }
    }

    async fn handle_request_extensions(
        &mut self,
        client_id: Option<usize>,
        msg: RequestExtensions,
    ) -> Result<(), Self::Error>;
}
