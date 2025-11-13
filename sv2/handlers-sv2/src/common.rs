use common_messages_sv2::{
    ChannelEndpointChanged, Reconnect, SetupConnectionError, SetupConnectionSuccess, *,
};
use framing_sv2::header::Header;
use parsers_sv2::{parse_message_frame_with_tlvs, AnyMessage, CommonMessages, Tlv};

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
    /// Return an empty Vec if no extensions have been negotiated.
    fn get_negotiated_extensions_with_server(
        &self,
        server_id: Option<usize>,
    ) -> Result<Vec<u16>, Self::Error>;

    /// Handles a raw Common protocol message frame from a server.
    ///
    /// This method parses the raw frame, extracts any TLV extension data, and delegates
    /// to `handle_common_message_from_server` with the parsed message and TLV fields.
    fn handle_common_message_frame_from_server(
        &mut self,
        server_id: Option<usize>,
        header: Header,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        let negotiated_extensions = self.get_negotiated_extensions_with_server(server_id)?;
        if negotiated_extensions.is_empty() {
            let parsed: CommonMessages<'_> = (header.msg_type(), payload)
                .try_into()
                .map_err(Self::Error::parse_error)?;
            return self.handle_common_message_from_server(server_id, parsed, None);
        }
        let (message, tlv_fields) =
            parse_message_frame_with_tlvs(header, payload, &negotiated_extensions)
                .map_err(Self::Error::parse_error)?;
        match message {
            AnyMessage::Common(parsed) => {
                self.handle_common_message_from_server(server_id, parsed, tlv_fields.as_deref())
            }
            _ => Err(Self::Error::unexpected_message(
                header.ext_type_without_channel_msg(),
                header.msg_type(),
            )),
        }
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
                0,
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
    /// Return an empty Vec if no extensions have been negotiated.
    fn get_negotiated_extensions_with_server(
        &self,
        server_id: Option<usize>,
    ) -> Result<Vec<u16>, Self::Error>;

    /// Handles a raw Common protocol message frame from a server.
    ///
    /// This method parses the raw frame, extracts any TLV extension data, and delegates
    /// to `handle_common_message_from_server` with the parsed message and TLV fields.
    async fn handle_common_message_frame_from_server(
        &mut self,
        server_id: Option<usize>,
        header: Header,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        async move {
            let negotiated_extensions = self.get_negotiated_extensions_with_server(server_id)?;
            if negotiated_extensions.is_empty() {
                let parsed: CommonMessages<'_> = (header.msg_type(), payload)
                    .try_into()
                    .map_err(Self::Error::parse_error)?;
                return self
                    .handle_common_message_from_server(server_id, parsed, None)
                    .await;
            }
            let (parsed, tlv_fields) =
                parse_message_frame_with_tlvs(header, payload, &negotiated_extensions)
                    .map_err(Self::Error::parse_error)?;
            match parsed {
                AnyMessage::Common(parsed) => {
                    self.handle_common_message_from_server(server_id, parsed, tlv_fields.as_deref())
                        .await
                }

                _ => Err(Self::Error::unexpected_message(
                    header.ext_type_without_channel_msg(),
                    header.msg_type(),
                )),
            }
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
                    0,
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
    /// Return an empty Vec if no extensions have been negotiated.
    fn get_negotiated_extensions_with_client(
        &self,
        client_id: Option<usize>,
    ) -> Result<Vec<u16>, Self::Error>;

    /// Handles a raw Common protocol message frame from a client.
    ///
    /// This method parses the raw frame, extracts any TLV extension data, and delegates
    /// to `handle_common_message_from_client` with the parsed message and TLV fields.
    fn handle_common_message_frame_from_client(
        &mut self,
        client_id: Option<usize>,
        header: Header,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        let negotiated_extensions = self.get_negotiated_extensions_with_client(client_id)?;
        if negotiated_extensions.is_empty() {
            let parsed: CommonMessages<'_> = (header.msg_type(), payload)
                .try_into()
                .map_err(Self::Error::parse_error)?;
            return self.handle_common_message_from_client(client_id, parsed, None);
        }
        let (parsed, tlv_fields) =
            parse_message_frame_with_tlvs(header, payload, &negotiated_extensions)
                .map_err(Self::Error::parse_error)?;
        match parsed {
            AnyMessage::Common(parsed) => {
                self.handle_common_message_from_client(client_id, parsed, tlv_fields.as_deref())
            }
            _ => Err(Self::Error::unexpected_message(
                header.ext_type_without_channel_msg(),
                header.msg_type(),
            )),
        }
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
                0,
                MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
            )),
            CommonMessages::SetupConnectionError(_) => Err(Self::Error::unexpected_message(
                0,
                MESSAGE_TYPE_SETUP_CONNECTION_ERROR,
            )),
            CommonMessages::ChannelEndpointChanged(_) => Err(Self::Error::unexpected_message(
                0,
                MESSAGE_TYPE_CHANNEL_ENDPOINT_CHANGED,
            )),
            CommonMessages::Reconnect(_) => {
                Err(Self::Error::unexpected_message(0, MESSAGE_TYPE_RECONNECT))
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
    /// Return an empty Vec if no extensions have been negotiated.
    fn get_negotiated_extensions_with_client(
        &self,
        client_id: Option<usize>,
    ) -> Result<Vec<u16>, Self::Error>;

    /// Handles a raw Common protocol message frame from a client.
    ///
    /// This method parses the raw frame, extracts any TLV extension data, and delegates
    /// to `handle_common_message_from_client` with the parsed message and TLV fields.
    async fn handle_common_message_frame_from_client(
        &mut self,
        client_id: Option<usize>,
        header: Header,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        async move {
            let negotiated_extensions = self.get_negotiated_extensions_with_client(client_id)?;
            if negotiated_extensions.is_empty() {
                let parsed: CommonMessages<'_> = (header.msg_type(), payload)
                    .try_into()
                    .map_err(Self::Error::parse_error)?;
                return self
                    .handle_common_message_from_client(client_id, parsed, None)
                    .await;
            }
            let (parsed, tlv_fields) =
                parse_message_frame_with_tlvs(header, payload, &negotiated_extensions)
                    .map_err(Self::Error::parse_error)?;
            match parsed {
                AnyMessage::Common(parsed) => {
                    self.handle_common_message_from_client(client_id, parsed, tlv_fields.as_deref())
                        .await
                }
                _ => Err(Self::Error::unexpected_message(
                    header.ext_type_without_channel_msg(),
                    header.msg_type(),
                )),
            }
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
                    0,
                    MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
                )),
                CommonMessages::SetupConnectionError(_) => Err(Self::Error::unexpected_message(
                    0,
                    MESSAGE_TYPE_SETUP_CONNECTION_ERROR,
                )),
                CommonMessages::ChannelEndpointChanged(_) => Err(Self::Error::unexpected_message(
                    0,
                    MESSAGE_TYPE_CHANNEL_ENDPOINT_CHANGED,
                )),
                CommonMessages::Reconnect(_) => {
                    Err(Self::Error::unexpected_message(0, MESSAGE_TYPE_RECONNECT))
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
