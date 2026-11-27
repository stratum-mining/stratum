use common_messages_sv2::*;
use framing_sv2::header::Header;
use parsers_sv2::{AnyMessage, CommonMessages, Tlv};

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
/// The `tlv_fields` parameter of the handler methods is `Some` with the frame's TLV fields that
/// belong to a negotiated extension, and `None` when no extension is negotiated or the frame
/// carries no such field. Each field's value is passed as raw bytes, and checking it is left to the
/// implementor.
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
    ///
    /// A frame whose `extension_type` this crate does not implement is dropped and `Ok(())` is
    /// returned, as Sv2 spec §3.4.1 requires for a message processed locally.
    fn handle_common_message_frame_from_server(
        &mut self,
        server_id: Option<usize>,
        header: Header,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        let negotiated_extensions = self.get_negotiated_extensions_with_server(server_id)?;
        let Some((message, tlv_fields)) =
            crate::parse_frame(header, payload, &negotiated_extensions)
                .map_err(Self::Error::parse_error)?
        else {
            return Ok(());
        };
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
    /// The `tlv_fields` parameter is `Some` with the frame's TLV fields that belong to a negotiated
    /// extension, and `None` when no extension is negotiated or the frame carries no such field.
    /// Each field's value is passed as raw bytes, and checking it is left to the implementor.
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
/// The `tlv_fields` parameter of the handler methods is `Some` with the frame's TLV fields that
/// belong to a negotiated extension, and `None` when no extension is negotiated or the frame
/// carries no such field. Each field's value is passed as raw bytes, and checking it is left to the
/// implementor.
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
    ///
    /// A frame whose `extension_type` this crate does not implement is dropped and `Ok(())` is
    /// returned, as Sv2 spec §3.4.1 requires for a message processed locally.
    async fn handle_common_message_frame_from_server(
        &mut self,
        server_id: Option<usize>,
        header: Header,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        async move {
            let negotiated_extensions = self.get_negotiated_extensions_with_server(server_id)?;
            let Some((parsed, tlv_fields)) =
                crate::parse_frame(header, payload, &negotiated_extensions)
                    .map_err(Self::Error::parse_error)?
            else {
                return Ok(());
            };
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
    /// The `tlv_fields` parameter is `Some` with the frame's TLV fields that belong to a negotiated
    /// extension, and `None` when no extension is negotiated or the frame carries no such field.
    /// Each field's value is passed as raw bytes, and checking it is left to the implementor.
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
/// The `tlv_fields` parameter of the handler methods is `Some` with the frame's TLV fields that
/// belong to a negotiated extension, and `None` when no extension is negotiated or the frame
/// carries no such field. Each field's value is passed as raw bytes, and checking it is left to the
/// implementor.
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
    ///
    /// A frame whose `extension_type` this crate does not implement is dropped and `Ok(())` is
    /// returned, as Sv2 spec §3.4.1 requires for a message processed locally.
    fn handle_common_message_frame_from_client(
        &mut self,
        client_id: Option<usize>,
        header: Header,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        let negotiated_extensions = self.get_negotiated_extensions_with_client(client_id)?;
        let Some((parsed, tlv_fields)) =
            crate::parse_frame(header, payload, &negotiated_extensions)
                .map_err(Self::Error::parse_error)?
        else {
            return Ok(());
        };
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
    /// The `tlv_fields` parameter is `Some` with the frame's TLV fields that belong to a negotiated
    /// extension, and `None` when no extension is negotiated or the frame carries no such field.
    /// Each field's value is passed as raw bytes, and checking it is left to the implementor.
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
/// The `tlv_fields` parameter of the handler methods is `Some` with the frame's TLV fields that
/// belong to a negotiated extension, and `None` when no extension is negotiated or the frame
/// carries no such field. Each field's value is passed as raw bytes, and checking it is left to the
/// implementor.
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
    ///
    /// A frame whose `extension_type` this crate does not implement is dropped and `Ok(())` is
    /// returned, as Sv2 spec §3.4.1 requires for a message processed locally.
    async fn handle_common_message_frame_from_client(
        &mut self,
        client_id: Option<usize>,
        header: Header,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        async move {
            let negotiated_extensions = self.get_negotiated_extensions_with_client(client_id)?;
            let Some((parsed, tlv_fields)) =
                crate::parse_frame(header, payload, &negotiated_extensions)
                    .map_err(Self::Error::parse_error)?
            else {
                return Ok(());
            };
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
    /// The `tlv_fields` parameter is `Some` with the frame's TLV fields that belong to a negotiated
    /// extension, and `None` when no extension is negotiated or the frame carries no such field.
    /// Each field's value is passed as raw bytes, and checking it is left to the implementor.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{header, TestError};

    // Counts the messages that reach a handler method.
    struct Client {
        handled: usize,
        negotiated: Vec<u16>,
    }

    impl HandleCommonMessagesFromServerSync for Client {
        type Error = TestError;
        fn get_negotiated_extensions_with_server(
            &self,
            _: Option<usize>,
        ) -> Result<Vec<u16>, TestError> {
            Ok(self.negotiated.clone())
        }
        fn handle_setup_connection_success(
            &mut self,
            _: Option<usize>,
            _: SetupConnectionSuccess,
            _: Option<&[Tlv]>,
        ) -> Result<(), TestError> {
            self.handled += 1;
            Ok(())
        }
        fn handle_setup_connection_error(
            &mut self,
            _: Option<usize>,
            _: SetupConnectionError,
            _: Option<&[Tlv]>,
        ) -> Result<(), TestError> {
            self.handled += 1;
            Ok(())
        }
        fn handle_channel_endpoint_changed(
            &mut self,
            _: Option<usize>,
            _: ChannelEndpointChanged,
            _: Option<&[Tlv]>,
        ) -> Result<(), TestError> {
            self.handled += 1;
            Ok(())
        }
        fn handle_reconnect(
            &mut self,
            _: Option<usize>,
            _: Reconnect,
            _: Option<&[Tlv]>,
        ) -> Result<(), TestError> {
            self.handled += 1;
            Ok(())
        }
    }

    // 0x4000 is in the experimental extension range (spec §3.4) and is not implemented here.
    #[test]
    fn frame_of_unknown_extension_is_dropped() {
        for negotiated in [vec![], vec![0x0002]] {
            for extension_type in [0x4000, 0xc000] {
                let mut client = Client {
                    handled: 0,
                    negotiated: negotiated.clone(),
                };
                let mut payload = [1, 2, 3];
                let h = header(extension_type, 0x00, payload.len());
                assert!(client
                    .handle_common_message_frame_from_server(None, h, &mut payload)
                    .is_ok());
                assert_eq!(client.handled, 0);
            }
        }
    }

    #[test]
    fn frame_of_known_extension_with_unknown_message_still_fails() {
        let mut client = Client {
            handled: 0,
            negotiated: vec![],
        };
        let mut payload = [];
        let h = header(0x0001, 0xff, 0);
        assert!(matches!(
            client.handle_common_message_frame_from_server(None, h, &mut payload),
            Err(TestError::Parse)
        ));
    }
}
