use binary_sv2::GetSize;
use common_messages_sv2::{
    ChannelEndpointChanged, Reconnect, SetupConnectionError, SetupConnectionSuccess, *,
};
use core::convert::TryInto;
use extensions_sv2::{has_valid_tlv_data, Tlv};
use parsers_sv2::CommonMessages;

use crate::error::HandlerErrorType;

/// Synchronous handler trait for processing common messages received from servers.
///
/// The server ID identifies which server a message originated from.
/// Whether this is relevant or not depends on which object is implementing the trait, and whether
/// this contextual information is readily available or not. In cases where `server_id` is either
/// irrelevant or can be inferred without the context, this should always be `None`.
///
/// ## TLV Extension Support
///
/// The `tlv_data` parameter in message handlers contains validated TLV fields if the message has
/// extension data appended. TLV fields are only passed if they match negotiated extensions
/// returned by `get_negotiated_extensions_with_server()`.
pub trait HandleCommonMessagesFromServerSync {
    type Error: HandlerErrorType;

    /// Returns the list of negotiated extension_types with a server.
    ///
    /// Used to validate TLV fields appended to messages. Return an empty slice if no
    /// extensions have been negotiated.
    fn get_negotiated_extensions_with_server(&self, server_id: Option<usize>) -> &[u16];

    fn handle_common_message_frame_from_server(
        &mut self,
        server_id: Option<usize>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        let raw_payload = payload.to_vec();
        let parsed: CommonMessages<'_> = (message_type, payload)
            .try_into()
            .map_err(Self::Error::parse_error)?;
        let parsed_size = parsed.get_size();

        // Check if there are remaining bytes that could be TLV data
        let tlv_fields = if raw_payload.len() > parsed_size {
            let remaining = &raw_payload[parsed_size..];
            let negotiated_extensions = self.get_negotiated_extensions_with_server(server_id);

            // Validate and parse TLV data against negotiated extensions
            if has_valid_tlv_data(remaining, negotiated_extensions) {
                Some(Tlv::parse_all(remaining))
            } else {
                None
            }
        } else {
            None
        };

        self.handle_common_message_from_server(server_id, parsed, tlv_fields.as_deref())
    }

    /// Handles a parsed common message from a server.
    ///
    /// The `tlv_fields` parameter contains parsed TLV fields if the message has extension
    /// data appended. It will be `Some(&[Tlv])` when valid TLV data is present, or `None`
    /// if no TLV data exists or validation fails. Each `Tlv` struct provides direct access to
    /// `extension_type`, `field_type`, `length`, and `value`.
    fn handle_common_message_from_server(
        &mut self,
        server_id: Option<usize>,
        message: CommonMessages<'_>,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error> {
        match message {
            CommonMessages::SetupConnectionSuccess(msg) => {
                self.handle_setup_connection_success(server_id, msg, tlv_fields)
            }
            CommonMessages::SetupConnectionError(msg) => {
                self.handle_setup_connection_error(server_id, msg, tlv_fields)
            }
            CommonMessages::ChannelEndpointChanged(msg) => {
                self.handle_channel_endpoint_changed(server_id, msg, tlv_fields)
            }
            CommonMessages::Reconnect(msg) => self.handle_reconnect(server_id, msg, tlv_fields),

            CommonMessages::SetupConnection(_) => Err(Self::Error::unexpected_message(
                MESSAGE_TYPE_SETUP_CONNECTION,
            )),
        }
    }

    fn handle_setup_connection_success(
        &mut self,
        server_id: Option<usize>,
        msg: SetupConnectionSuccess,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_setup_connection_error(
        &mut self,
        server_id: Option<usize>,
        msg: SetupConnectionError,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_channel_endpoint_changed(
        &mut self,
        server_id: Option<usize>,
        msg: ChannelEndpointChanged,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_reconnect(
        &mut self,
        server_id: Option<usize>,
        msg: Reconnect,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;
}

/// Asynchronous handler trait for processing common messages received from servers.
///
/// The server ID identifies which server a message originated from.
/// Whether this is relevant or not depends on which object is implementing the trait, and whether
/// this contextual information is readily available or not. In cases where `server_id` is either
/// irrelevant or can be inferred without the context, this should always be `None`.
///
/// ## TLV Extension Support
///
/// The `tlv_data` parameter in message handlers contains validated TLV fields if the message has
/// extension data appended. TLV fields are only passed if they match negotiated extensions
/// returned by `get_negotiated_extensions_with_server()`.
#[trait_variant::make(Send)]
pub trait HandleCommonMessagesFromServerAsync {
    type Error: HandlerErrorType;

    /// Returns the list of negotiated extension_types with a server.
    ///
    /// Used to validate TLV fields appended to messages. Return an empty slice if no
    /// extensions have been negotiated.
    fn get_negotiated_extensions_with_server(&self, server_id: Option<usize>) -> &[u16];

    async fn handle_common_message_frame_from_server(
        &mut self,
        server_id: Option<usize>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        async move {
            let raw_payload = payload.to_vec();
            let parsed: CommonMessages<'_> = (message_type, payload)
                .try_into()
                .map_err(Self::Error::parse_error)?;
            let parsed_size = parsed.get_size();

            // Check if there are remaining bytes that could be TLV data
            let tlv_fields = if raw_payload.len() > parsed_size {
                let remaining = &raw_payload[parsed_size..];
                let negotiated_extensions = self.get_negotiated_extensions_with_server(server_id);

                // Validate and parse TLV data against negotiated extensions
                if has_valid_tlv_data(remaining, negotiated_extensions) {
                    Some(Tlv::parse_all(remaining))
                } else {
                    None
                }
            } else {
                None
            };

            self.handle_common_message_from_server(server_id, parsed, tlv_fields.as_deref())
                .await
        }
    }

    /// Handles a parsed common message from a server.
    ///
    /// The `tlv_fields` parameter contains parsed TLV fields if the message has extension
    /// data appended. It will be `Some(&[Tlv])` when valid TLV data is present, or `None`
    /// if no TLV data exists or validation fails. Each `Tlv` struct provides direct access to
    /// `extension_type`, `field_type`, `length`, and `value`.
    async fn handle_common_message_from_server(
        &mut self,
        server_id: Option<usize>,
        message: CommonMessages<'_>,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error> {
        async move {
            match message {
                CommonMessages::SetupConnectionSuccess(msg) => {
                    self.handle_setup_connection_success(server_id, msg, tlv_fields)
                        .await
                }
                CommonMessages::SetupConnectionError(msg) => {
                    self.handle_setup_connection_error(server_id, msg, tlv_fields)
                        .await
                }
                CommonMessages::ChannelEndpointChanged(msg) => {
                    self.handle_channel_endpoint_changed(server_id, msg, tlv_fields)
                        .await
                }
                CommonMessages::Reconnect(msg) => {
                    self.handle_reconnect(server_id, msg, tlv_fields).await
                }

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
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_setup_connection_error(
        &mut self,
        server_id: Option<usize>,
        msg: SetupConnectionError,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_channel_endpoint_changed(
        &mut self,
        server_id: Option<usize>,
        msg: ChannelEndpointChanged,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_reconnect(
        &mut self,
        server_id: Option<usize>,
        msg: Reconnect,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;
}

/// Synchronous handler trait for processing common messages received from clients.
///
/// The client ID identifies which client a message originated from.
/// Whether this is relevant or not depends on which object is implementing the trait, and whether
/// this contextual information is readily available or not. In cases where `client_id` is either
/// irrelevant or can be inferred without the context, this should always be `None`.
///
/// ## TLV Extension Support
///
/// The `tlv_data` parameter in message handlers contains validated TLV fields if the message has
/// extension data appended. TLV fields are only passed if they match negotiated extensions
/// returned by `get_negotiated_extensions_with_client()`.
pub trait HandleCommonMessagesFromClientSync {
    type Error: HandlerErrorType;

    /// Returns the list of negotiated extension_types with a client.
    ///
    /// Used to validate TLV fields appended to messages. Return an empty slice if no
    /// extensions have been negotiated.
    fn get_negotiated_extensions_with_client(&self, client_id: Option<usize>) -> &[u16];

    fn handle_common_message_frame_from_client(
        &mut self,
        client_id: Option<usize>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        let raw_payload = payload.to_vec();
        let parsed: CommonMessages<'_> = (message_type, payload)
            .try_into()
            .map_err(Self::Error::parse_error)?;
        let parsed_size = parsed.get_size();

        // Check if there are remaining bytes that could be TLV data
        let tlv_fields = if raw_payload.len() > parsed_size {
            let remaining = &raw_payload[parsed_size..];
            let negotiated_extensions = self.get_negotiated_extensions_with_client(client_id);

            // Validate and parse TLV data against negotiated extensions
            if has_valid_tlv_data(remaining, negotiated_extensions) {
                Some(Tlv::parse_all(remaining))
            } else {
                None
            }
        } else {
            None
        };

        self.handle_common_message_from_client(client_id, parsed, tlv_fields.as_deref())
    }

    /// Handles a parsed common message from a client.
    ///
    /// The `tlv_fields` parameter contains parsed TLV fields if the message has extension
    /// data appended. It will be `Some(&[Tlv])` when valid TLV data is present, or `None`
    /// if no TLV data exists or validation fails. Each `Tlv` struct provides direct access to
    /// `extension_type`, `field_type`, `length`, and `value`.
    fn handle_common_message_from_client(
        &mut self,
        client_id: Option<usize>,
        message: CommonMessages<'_>,
        tlv_fields: Option<&[Tlv]>,
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

            CommonMessages::SetupConnection(msg) => {
                self.handle_setup_connection(client_id, msg, tlv_fields)
            }
        }
    }

    fn handle_setup_connection(
        &mut self,
        client_id: Option<usize>,
        msg: SetupConnection,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;
}

/// Asynchronous handler trait for processing common messages received from clients.
///
/// The client ID identifies which client a message originated from.
/// Whether this is relevant or not depends on which object is implementing the trait, and whether
/// this contextual information is readily available or not. In cases where `client_id` is either
/// irrelevant or can be inferred without the context, this should always be `None`.
///
/// ## TLV Extension Support
///
/// The `tlv_data` parameter in message handlers contains validated TLV fields if the message has
/// extension data appended. TLV fields are only passed if they match negotiated extensions
/// returned by `get_negotiated_extensions_with_client()`.
#[trait_variant::make(Send)]
pub trait HandleCommonMessagesFromClientAsync {
    type Error: HandlerErrorType;

    /// Returns the list of negotiated extension_types with a client.
    ///
    /// Used to validate TLV fields appended to messages. Return an empty slice if no
    /// extensions have been negotiated.
    fn get_negotiated_extensions_with_client(&self, client_id: Option<usize>) -> &[u16];

    async fn handle_common_message_frame_from_client(
        &mut self,
        client_id: Option<usize>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        async move {
            let raw_payload = payload.to_vec();
            let parsed: CommonMessages<'_> = (message_type, payload)
                .try_into()
                .map_err(Self::Error::parse_error)?;
            let parsed_size = parsed.get_size();

            // Check if there are remaining bytes that could be TLV data
            let tlv_fields = if raw_payload.len() > parsed_size {
                let remaining = &raw_payload[parsed_size..];
                let negotiated_extensions = self.get_negotiated_extensions_with_client(client_id);

                // Validate and parse TLV data against negotiated extensions
                if has_valid_tlv_data(remaining, negotiated_extensions) {
                    Some(Tlv::parse_all(remaining))
                } else {
                    None
                }
            } else {
                None
            };

            self.handle_common_message_from_client(client_id, parsed, tlv_fields.as_deref())
                .await
        }
    }

    /// Handles a parsed common message from a client.
    ///
    /// The `tlv_fields` parameter contains parsed TLV fields if the message has extension
    /// data appended. It will be `Some(&[Tlv])` when valid TLV data is present, or `None`
    /// if no TLV data exists or validation fails. Each `Tlv` struct provides direct access to
    /// `extension_type`, `field_type`, `length`, and `value`.
    async fn handle_common_message_from_client(
        &mut self,
        client_id: Option<usize>,
        message: CommonMessages<'_>,
        tlv_fields: Option<&[Tlv]>,
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
                    self.handle_setup_connection(client_id, msg, tlv_fields)
                        .await
                }
            }
        }
    }

    async fn handle_setup_connection(
        &mut self,
        client_id: Option<usize>,
        msg: SetupConnection,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;
}
