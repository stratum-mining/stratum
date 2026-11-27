use common_messages_sv2::{
    ChannelEndpointChanged, Reconnect, SetupConnectionError, SetupConnectionSuccess,
    MESSAGE_TYPE_CHANNEL_ENDPOINT_CHANGED, MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
};
use framing_sv2::header::Header;
use handlers_sv2::{HandleCommonMessagesFromServerSync, HandlerErrorType};
use parsers_sv2::{ParserError, Tlv};

#[derive(Debug)]
enum Error {
    Unexpected,
    Parse(ParserError),
}

impl HandlerErrorType for Error {
    fn unexpected_message(_extension_type: u16, _message_type: u8) -> Self {
        Error::Unexpected
    }
    fn parse_error(error: ParserError) -> Self {
        Error::Parse(error)
    }
}

struct Client {
    negotiated_extensions: Vec<u16>,
    dispatched: Vec<u8>,
}

impl HandleCommonMessagesFromServerSync for Client {
    type Error = Error;

    fn get_negotiated_extensions_with_server(
        &self,
        _server_id: Option<usize>,
    ) -> Result<Vec<u16>, Error> {
        Ok(self.negotiated_extensions.clone())
    }

    fn handle_setup_connection_success(
        &mut self,
        _server_id: Option<usize>,
        _msg: SetupConnectionSuccess,
        _tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Error> {
        self.dispatched.push(MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS);
        Ok(())
    }

    fn handle_setup_connection_error(
        &mut self,
        _server_id: Option<usize>,
        _msg: SetupConnectionError,
        _tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Error> {
        unreachable!()
    }

    fn handle_channel_endpoint_changed(
        &mut self,
        _server_id: Option<usize>,
        _msg: ChannelEndpointChanged,
        _tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Error> {
        self.dispatched.push(MESSAGE_TYPE_CHANNEL_ENDPOINT_CHANGED);
        Ok(())
    }

    fn handle_reconnect(
        &mut self,
        _server_id: Option<usize>,
        _msg: Reconnect,
        _tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Error> {
        unreachable!()
    }
}

const CHANNEL_MSG: u16 = 0x8000;
// SetupConnectionSuccess { used_version: 2, flags: 0 }
const SETUP_CONNECTION_SUCCESS: [u8; 6] = [2, 0, 0, 0, 0, 0];
// ChannelEndpointChanged { channel_id: 1 }
const CHANNEL_ENDPOINT_CHANGED: [u8; 4] = [1, 0, 0, 0];

fn header(extension_type: u16, message_type: u8, payload_length: usize) -> Header {
    let ext = extension_type.to_le_bytes();
    let len = (payload_length as u32).to_le_bytes();
    Header::from_bytes(&[ext[0], ext[1], message_type, len[0], len[1], len[2]]).unwrap()
}

/// Feeds one frame to a client, once with no extensions negotiated and once with one, and checks
/// that both decode paths give the same verdict.
fn dispatch(header: Header, payload: &[u8]) -> Result<Vec<u8>, Error> {
    let mut verdicts = Vec::new();
    for negotiated_extensions in [vec![], vec![0x0002]] {
        let mut client = Client {
            negotiated_extensions,
            dispatched: Vec::new(),
        };
        let mut payload = payload.to_vec();
        verdicts.push(
            client
                .handle_common_message_frame_from_server(None, header, &mut payload)
                .map(|()| client.dispatched),
        );
    }
    let with_extensions = verdicts.pop().unwrap();
    let without_extensions = verdicts.pop().unwrap();
    assert_eq!(
        format!("{without_extensions:?}"),
        format!("{with_extensions:?}")
    );
    without_extensions
}

#[test]
fn dispatches_consistent_frames() {
    let h = header(0, MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS, 6);
    assert_eq!(
        dispatch(h, &SETUP_CONNECTION_SUCCESS).unwrap(),
        [MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS]
    );

    let h = header(CHANNEL_MSG, MESSAGE_TYPE_CHANNEL_ENDPOINT_CHANGED, 4);
    assert_eq!(
        dispatch(h, &CHANNEL_ENDPOINT_CHANGED).unwrap(),
        [MESSAGE_TYPE_CHANNEL_ENDPOINT_CHANGED]
    );
}

#[test]
fn rejects_core_message_type_under_extension_namespace() {
    let h = header(0x0002, MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS, 6);
    assert!(dispatch(h, &SETUP_CONNECTION_SUCCESS).is_err());
}

#[test]
fn rejects_channel_bit_mismatch() {
    let h = header(CHANNEL_MSG, MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS, 6);
    assert!(matches!(
        dispatch(h, &SETUP_CONNECTION_SUCCESS),
        Err(Error::Parse(ParserError::ChannelBitMismatch { .. }))
    ));

    let h = header(0, MESSAGE_TYPE_CHANNEL_ENDPOINT_CHANGED, 4);
    assert!(matches!(
        dispatch(h, &CHANNEL_ENDPOINT_CHANGED),
        Err(Error::Parse(ParserError::ChannelBitMismatch { .. }))
    ));
}

#[test]
fn rejects_payload_length_mismatch() {
    for declared in [0, 5, 7] {
        let h = header(0, MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS, declared);
        assert!(matches!(
            dispatch(h, &SETUP_CONNECTION_SUCCESS),
            Err(Error::Parse(ParserError::PayloadLengthMismatch { .. }))
        ));
    }
}
