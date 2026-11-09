use crate::{HandleCommonMessagesFromServerSync, HandlerErrorType};
use common_messages_sv2::{
    ChannelEndpointChanged, Reconnect, SetupConnectionError, SetupConnectionSuccess,
    MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
};
use framing_sv2::header::Header;
use parsers_sv2::{ParserError, Tlv};

#[derive(Debug, PartialEq)]
enum TestError {
    UnexpectedMessage(u16, u8),
    Parse,
}

impl HandlerErrorType for TestError {
    fn unexpected_message(extension_type: u16, message_type: u8) -> Self {
        TestError::UnexpectedMessage(extension_type, message_type)
    }

    fn parse_error(_error: ParserError) -> Self {
        TestError::Parse
    }
}

/// Server-facing handler with no negotiated extensions, so every frame takes the fast path.
#[derive(Default)]
struct Client {
    setup_connection_successes: usize,
}

impl HandleCommonMessagesFromServerSync for Client {
    type Error = TestError;

    fn get_negotiated_extensions_with_server(
        &self,
        _server_id: Option<usize>,
    ) -> Result<Vec<u16>, Self::Error> {
        Ok(Vec::new())
    }

    fn handle_setup_connection_success(
        &mut self,
        _server_id: Option<usize>,
        _msg: SetupConnectionSuccess,
        _tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error> {
        self.setup_connection_successes += 1;
        Ok(())
    }

    fn handle_setup_connection_error(
        &mut self,
        _server_id: Option<usize>,
        _msg: SetupConnectionError,
        _tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error> {
        unreachable!()
    }

    fn handle_channel_endpoint_changed(
        &mut self,
        _server_id: Option<usize>,
        _msg: ChannelEndpointChanged,
        _tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error> {
        unreachable!()
    }

    fn handle_reconnect(
        &mut self,
        _server_id: Option<usize>,
        _msg: Reconnect,
        _tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error> {
        unreachable!()
    }
}

/// Builds a `SetupConnectionSuccess` frame under the given extension type.
fn setup_connection_success_frame(extension_type: u16) -> (Header, Vec<u8>) {
    let payload = binary_sv2::to_bytes(SetupConnectionSuccess {
        used_version: 2,
        flags: 0,
    })
    .unwrap();
    let mut header = Vec::new();
    header.extend_from_slice(&extension_type.to_le_bytes());
    header.push(MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS);
    header.extend_from_slice(&(payload.len() as u32).to_le_bytes()[..3]);
    (Header::from_bytes(&header).unwrap(), payload)
}

#[test]
fn fast_path_dispatches_core_messages() {
    let mut client = Client::default();
    let (header, mut payload) = setup_connection_success_frame(0);

    client
        .handle_common_message_frame_from_server(None, header, &mut payload)
        .unwrap();

    assert_eq!(client.setup_connection_successes, 1);
}

#[test]
fn fast_path_rejects_frames_in_an_extension_namespace() {
    let mut client = Client::default();
    // Same message type and payload, but in the Extensions Negotiation namespace (0x0001),
    // where message type 0x01 is RequestExtensions.Success.
    let (header, mut payload) = setup_connection_success_frame(0x0001);

    let result = client.handle_common_message_frame_from_server(None, header, &mut payload);

    assert_eq!(
        result,
        Err(TestError::UnexpectedMessage(
            0x0001,
            MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS
        ))
    );
    assert_eq!(client.setup_connection_successes, 0);
}
