use extensions_sv2::{RequestExtensions, RequestExtensionsError, RequestExtensionsSuccess};
use framing_sv2::header::Header;
use parsers_sv2::{parse_message_frame_with_tlvs, AnyMessage, Extensions, Tlv};

use crate::error::HandlerErrorType;

/// Synchronous handler trait for processing extension messages received from servers.
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
pub trait HandleExtensionsFromServerSync {
    type Error: HandlerErrorType;

    /// Returns the list of negotiated extension_types with a server.
    ///
    /// Return an empty Vec if no extensions have been negotiated.
    fn get_negotiated_extensions_with_server(
        &self,
        server_id: Option<usize>,
    ) -> Result<Vec<u16>, Self::Error>;

    /// Handles a raw Extensions protocol message frame from a server.
    ///
    /// This method parses the raw frame, extracts any TLV extension data, and delegates
    /// to `handle_extensions_message_from_server` with the parsed message and TLV fields.
    fn handle_extensions_message_frame_from_server(
        &mut self,
        server_id: Option<usize>,
        header: Header,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        let negotiated_extensions = self.get_negotiated_extensions_with_server(server_id)?;
        if negotiated_extensions.is_empty() {
            let parsed: Extensions<'_> = (header.ext_type(), header.msg_type(), payload)
                .try_into()
                .map_err(Self::Error::parse_error)?;
            return self.handle_extensions_message_from_server(server_id, parsed, None);
        }
        let (parsed, tlv_fields) =
            parse_message_frame_with_tlvs(header, payload, &negotiated_extensions)
                .map_err(Self::Error::parse_error)?;
        match parsed {
            AnyMessage::Extensions(parsed) => {
                self.handle_extensions_message_from_server(server_id, parsed, tlv_fields.as_deref())
            }
            _ => Err(Self::Error::unexpected_message(
                header.ext_type_without_channel_msg(),
                header.msg_type(),
            )),
        }
    }

    /// Handles a parsed extensions message from a server.
    ///
    /// The `tlv_fields` parameter contains parsed TLV fields if the message has extension
    /// data appended. It will be `Some(&[Tlv])` when valid TLV data is present, or `None`
    /// if no TLV data exists or validation fails. Each `Tlv` struct provides direct access to
    /// `extension_type`, `field_type`, `length`, and `value`.
    fn handle_extensions_message_from_server(
        &mut self,
        server_id: Option<usize>,
        message: Extensions<'_>,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error> {
        match message {
            Extensions::ExtensionsNegotiation(ext) => match ext {
                parsers_sv2::ExtensionsNegotiation::RequestExtensionsSuccess(msg) => {
                    self.handle_request_extensions_success(server_id, msg, tlv_fields)
                }
                parsers_sv2::ExtensionsNegotiation::RequestExtensionsError(msg) => {
                    self.handle_request_extensions_error(server_id, msg, tlv_fields)
                }
                // RequestExtensions is sent by client, not server
                parsers_sv2::ExtensionsNegotiation::RequestExtensions(_) => {
                    Err(Self::Error::unexpected_message(
                        extensions_sv2::EXTENSION_TYPE_EXTENSIONS_NEGOTIATION,
                        extensions_sv2::MESSAGE_TYPE_REQUEST_EXTENSIONS,
                    ))
                }
            },
        }
    }

    fn handle_request_extensions_success(
        &mut self,
        server_id: Option<usize>,
        msg: RequestExtensionsSuccess,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_request_extensions_error(
        &mut self,
        server_id: Option<usize>,
        msg: RequestExtensionsError,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;
}

/// Asynchronous handler trait for processing extension messages received from servers.
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
pub trait HandleExtensionsFromServerAsync {
    type Error: HandlerErrorType;

    /// Returns the list of negotiated extension_types with a server.
    ///
    /// Return an empty Vec if no extensions have been negotiated.
    fn get_negotiated_extensions_with_server(
        &self,
        server_id: Option<usize>,
    ) -> Result<Vec<u16>, Self::Error>;

    /// Handles a raw Extensions protocol message frame from a server.
    ///
    /// This method parses the raw frame, extracts any TLV extension data, and delegates
    /// to `handle_extensions_message_from_server` with the parsed message and TLV fields.
    async fn handle_extensions_message_frame_from_server(
        &mut self,
        server_id: Option<usize>,
        header: Header,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        async move {
            let negotiated_extensions = self.get_negotiated_extensions_with_server(server_id)?;
            if negotiated_extensions.is_empty() {
                let parsed: Extensions<'_> = (header.ext_type(), header.msg_type(), payload)
                    .try_into()
                    .map_err(Self::Error::parse_error)?;
                return self
                    .handle_extensions_message_from_server(server_id, parsed, None)
                    .await;
            }
            let (parsed, tlv_fields) =
                parse_message_frame_with_tlvs(header, payload, &negotiated_extensions)
                    .map_err(Self::Error::parse_error)?;
            match parsed {
                AnyMessage::Extensions(parsed) => {
                    self.handle_extensions_message_from_server(
                        server_id,
                        parsed,
                        tlv_fields.as_deref(),
                    )
                    .await
                }
                _ => Err(Self::Error::unexpected_message(
                    header.ext_type_without_channel_msg(),
                    header.msg_type(),
                )),
            }
        }
    }

    /// Handles a parsed extensions message from a server.
    ///
    /// The `tlv_fields` parameter contains parsed TLV fields if the message has extension
    /// data appended. It will be `Some(&[Tlv])` when valid TLV data is present, or `None`
    /// if no TLV data exists or validation fails. Each `Tlv` struct provides direct access to
    /// `extension_type`, `field_type`, `length`, and `value`.
    async fn handle_extensions_message_from_server(
        &mut self,
        server_id: Option<usize>,
        message: Extensions<'_>,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error> {
        async move {
            match message {
                Extensions::ExtensionsNegotiation(ext) => match ext {
                    parsers_sv2::ExtensionsNegotiation::RequestExtensionsSuccess(msg) => {
                        self.handle_request_extensions_success(server_id, msg, tlv_fields)
                            .await
                    }
                    parsers_sv2::ExtensionsNegotiation::RequestExtensionsError(msg) => {
                        self.handle_request_extensions_error(server_id, msg, tlv_fields)
                            .await
                    }
                    // RequestExtensions is sent by client, not server
                    parsers_sv2::ExtensionsNegotiation::RequestExtensions(_) => {
                        Err(Self::Error::unexpected_message(
                            extensions_sv2::EXTENSION_TYPE_EXTENSIONS_NEGOTIATION,
                            extensions_sv2::MESSAGE_TYPE_REQUEST_EXTENSIONS,
                        ))
                    }
                },
            }
        }
    }

    async fn handle_request_extensions_success(
        &mut self,
        server_id: Option<usize>,
        msg: RequestExtensionsSuccess,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_request_extensions_error(
        &mut self,
        server_id: Option<usize>,
        msg: RequestExtensionsError,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;
}

/// Synchronous handler trait for processing extension messages received from clients.
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
pub trait HandleExtensionsFromClientSync {
    type Error: HandlerErrorType;

    /// Returns the list of negotiated extension_types with a client.
    ///
    /// Return an empty Vec if no extensions have been negotiated.
    fn get_negotiated_extensions_with_client(
        &self,
        client_id: Option<usize>,
    ) -> Result<Vec<u16>, Self::Error>;

    /// Handles a raw Extensions protocol message frame from a client.
    ///
    /// This method parses the raw frame, extracts any TLV extension data, and delegates
    /// to `handle_extensions_message_from_client` with the parsed message and TLV fields.
    fn handle_extensions_message_frame_from_client(
        &mut self,
        client_id: Option<usize>,
        header: Header,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        let negotiated_extensions = self.get_negotiated_extensions_with_client(client_id)?;
        if negotiated_extensions.is_empty() {
            let parsed: Extensions<'_> = (header.ext_type(), header.msg_type(), payload)
                .try_into()
                .map_err(Self::Error::parse_error)?;
            return self.handle_extensions_message_from_client(client_id, parsed, None);
        }
        let (parsed, tlv_fields) =
            parse_message_frame_with_tlvs(header, payload, &negotiated_extensions)
                .map_err(Self::Error::parse_error)?;
        match parsed {
            AnyMessage::Extensions(parsed) => {
                self.handle_extensions_message_from_client(client_id, parsed, tlv_fields.as_deref())
            }
            _ => Err(Self::Error::unexpected_message(
                header.ext_type_without_channel_msg(),
                header.msg_type(),
            )),
        }
    }

    /// Handles a parsed extensions message from a client.
    ///
    /// The `tlv_fields` parameter contains parsed TLV fields if the message has extension
    /// data appended. It will be `Some(&[Tlv])` when valid TLV data is present, or `None`
    /// if no TLV data exists or validation fails. Each `Tlv` struct provides direct access to
    /// `extension_type`, `field_type`, `length`, and `value`.
    fn handle_extensions_message_from_client(
        &mut self,
        client_id: Option<usize>,
        message: Extensions<'_>,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error> {
        match message {
            Extensions::ExtensionsNegotiation(ext) => match ext {
                parsers_sv2::ExtensionsNegotiation::RequestExtensions(msg) => {
                    self.handle_request_extensions(client_id, msg, tlv_fields)
                }
                // Success/Error are sent by server, not client
                parsers_sv2::ExtensionsNegotiation::RequestExtensionsSuccess(_) => {
                    Err(Self::Error::unexpected_message(
                        extensions_sv2::EXTENSION_TYPE_EXTENSIONS_NEGOTIATION,
                        extensions_sv2::MESSAGE_TYPE_REQUEST_EXTENSIONS_SUCCESS,
                    ))
                }
                parsers_sv2::ExtensionsNegotiation::RequestExtensionsError(_) => {
                    Err(Self::Error::unexpected_message(
                        extensions_sv2::EXTENSION_TYPE_EXTENSIONS_NEGOTIATION,
                        extensions_sv2::MESSAGE_TYPE_REQUEST_EXTENSIONS_ERROR,
                    ))
                }
            },
        }
    }

    fn handle_request_extensions(
        &mut self,
        client_id: Option<usize>,
        msg: RequestExtensions,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;
}

/// Asynchronous handler trait for processing extension messages received from clients.
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
pub trait HandleExtensionsFromClientAsync {
    type Error: HandlerErrorType;

    /// Returns the list of negotiated extension_types with a client.
    ///
    /// Return an empty Vec if no extensions have been negotiated.
    fn get_negotiated_extensions_with_client(
        &self,
        client_id: Option<usize>,
    ) -> Result<Vec<u16>, Self::Error>;

    /// Handles a raw Extensions protocol message frame from a client.
    ///
    /// This method parses the raw frame, extracts any TLV extension data, and delegates
    /// to `handle_extensions_message_from_client` with the parsed message and TLV fields.
    async fn handle_extensions_message_frame_from_client(
        &mut self,
        client_id: Option<usize>,
        header: Header,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        async move {
            let negotiated_extensions = self.get_negotiated_extensions_with_client(client_id)?;
            if negotiated_extensions.is_empty() {
                let parsed: Extensions<'_> = (header.ext_type(), header.msg_type(), payload)
                    .try_into()
                    .map_err(Self::Error::parse_error)?;
                return self
                    .handle_extensions_message_from_client(client_id, parsed, None)
                    .await;
            }
            let (parsed, tlv_fields) =
                parse_message_frame_with_tlvs(header, payload, &negotiated_extensions)
                    .map_err(Self::Error::parse_error)?;
            match parsed {
                AnyMessage::Extensions(parsed) => {
                    self.handle_extensions_message_from_client(
                        client_id,
                        parsed,
                        tlv_fields.as_deref(),
                    )
                    .await
                }
                _ => Err(Self::Error::unexpected_message(
                    header.ext_type_without_channel_msg(),
                    header.msg_type(),
                )),
            }
        }
    }

    /// Handles a parsed extensions message from a client.
    ///
    /// The `tlv_fields` parameter contains parsed TLV fields if the message has extension
    /// data appended. It will be `Some(&[Tlv])` when valid TLV data is present, or `None`
    /// if no TLV data exists or validation fails. Each `Tlv` struct provides direct access to
    /// `extension_type`, `field_type`, `length`, and `value`.
    async fn handle_extensions_message_from_client(
        &mut self,
        client_id: Option<usize>,
        message: Extensions<'_>,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error> {
        async move {
            match message {
                Extensions::ExtensionsNegotiation(ext) => match ext {
                    parsers_sv2::ExtensionsNegotiation::RequestExtensions(msg) => {
                        self.handle_request_extensions(client_id, msg, tlv_fields)
                            .await
                    }
                    // Success/Error are sent by server, not client
                    parsers_sv2::ExtensionsNegotiation::RequestExtensionsSuccess(_) => {
                        Err(Self::Error::unexpected_message(
                            extensions_sv2::EXTENSION_TYPE_EXTENSIONS_NEGOTIATION,
                            extensions_sv2::MESSAGE_TYPE_REQUEST_EXTENSIONS_SUCCESS,
                        ))
                    }
                    parsers_sv2::ExtensionsNegotiation::RequestExtensionsError(_) => {
                        Err(Self::Error::unexpected_message(
                            extensions_sv2::EXTENSION_TYPE_EXTENSIONS_NEGOTIATION,
                            extensions_sv2::MESSAGE_TYPE_REQUEST_EXTENSIONS_ERROR,
                        ))
                    }
                },
            }
        }
    }

    async fn handle_request_extensions(
        &mut self,
        client_id: Option<usize>,
        msg: RequestExtensions,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;
}
