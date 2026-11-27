#![no_main]

mod common;

use std::convert::TryFrom;

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use serde_json::Value;
use sv1_api::{
    json_rpc::{RpcRequest, RpcResponse},
    utils::{HexU32Be, VERSION_ROLLING_MASK},
    ClientToServer, Id, Message, Method, RpcError, ServerToClient,
};

#[derive(Debug)]
struct ArbJson(Value);

impl<'a> Arbitrary<'a> for ArbJson {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        common::gen_json_value(u, 5).map(ArbJson)
    }
}

#[derive(Arbitrary, Debug)]
enum MsgKind {
    Request,
    Notification,
    OkResponse,
    ErrorResponse,
}

#[derive(Arbitrary, Debug)]
struct FuzzInput {
    id: u64,
    string_id: Option<String>,
    method: String,
    params: ArbJson,
    result: ArbJson,
    error_code: i32,
    error_message: String,
    kind: MsgKind,
}

impl From<FuzzInput> for Message {
    fn from(fi: FuzzInput) -> Self {
        let id = fi.string_id.map_or(Id::from(fi.id), Id::from);
        match fi.kind {
            MsgKind::Request => Message::Request(RpcRequest {
                id: Some(id),
                method: Method::from(fi.method),
                params: fi.params.0,
            }),
            MsgKind::Notification => Message::Request(RpcRequest {
                id: None,
                method: Method::from(fi.method),
                params: fi.params.0,
            }),
            MsgKind::OkResponse => Message::Response(RpcResponse::success(id, fi.result.0)),
            MsgKind::ErrorResponse => Message::Response(RpcResponse::failure(
                Some(id),
                RpcError {
                    code: fi.error_code,
                    message: fi.error_message,
                    data: None,
                },
            )),
        }
    }
}

fn round_trip(message: Message) {
    let line = serde_json::to_string(&message).expect("messages always serialize");
    let decoded = Message::decode(&line).expect("serialized messages always decode");
    assert_eq!(decoded, message);
}

/// Formatted output carries no raw control character, whatever the peer sent.
fn assert_escaped(formatted: String) {
    assert!(!formatted.chars().any(char::is_control), "{formatted:?}");
}

fn assert_formats_escaped(value: &(impl std::fmt::Display + std::fmt::Debug)) {
    assert_escaped(format!("{value}"));
    assert_escaped(format!("{value:?}"));
}

/// A decoded mask never covers reserved bits.
fn assert_no_reserved_bits(mask: HexU32Be) {
    assert_eq!(mask.0 & !VERSION_ROLLING_MASK, 0, "{mask}");
}

fn test_msg_conversions(msg: Message) {
    assert_formats_escaped(&msg);

    match msg {
        Message::Request(request) => {
            match ClientToServer::try_from(request.clone()) {
                Ok(m) => {
                    assert_escaped(format!("{m:?}"));
                    if let ClientToServer::Configure(configure) = &m {
                        if let Some(mask) = configure.params.version_rolling_mask() {
                            assert_no_reserved_bits(mask);
                        }
                        assert!(configure.params.version_rolling_min_bit_count() <= Some(32));
                    }
                    let line = serde_json::to_string(&Message::from(m)).unwrap();
                    if let Ok(Message::Request(again)) = Message::decode(&line) {
                        ClientToServer::try_from(again).expect("encoded requests always decode");
                    }
                }
                Err(e) => assert_formats_escaped(&e),
            }
            match ServerToClient::try_from(request) {
                Ok(m) => {
                    assert_escaped(format!("{m:?}"));
                    if let ServerToClient::SetVersionMask(set) = &m {
                        assert_no_reserved_bits(set.mask());
                    }
                    let line = serde_json::to_string(&Message::from(m.clone())).unwrap();
                    if let Ok(Message::Request(again)) = Message::decode(&line) {
                        let again = ServerToClient::try_from(again)
                            .expect("encoded notifications always decode");
                        assert_eq!(again, m);
                    }
                }
                Err(e) => assert_formats_escaped(&e),
            }
        }
        Message::Response(response) => {
            for method in [Method::Subscribe, Method::Configure, Method::Submit] {
                match ServerToClient::from_response(&response, &method) {
                    Ok(Some(m)) => {
                        assert_escaped(format!("{m:?}"));
                        if let ServerToClient::Configure(reply) = &m {
                            if let Ok(Some(version_rolling)) =
                                reply.result.as_ref().map(|result| &result.version_rolling)
                            {
                                assert_no_reserved_bits(version_rolling.mask());
                            }
                        }
                    }
                    Ok(None) => {}
                    Err(e) => assert_formats_escaped(&e),
                }
            }
        }
    }
}

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        match Message::decode(s) {
            Ok(msg) => {
                test_msg_conversions(msg.clone());
                round_trip(msg);
            }
            Err(e) => assert_formats_escaped(&e),
        }
    }
    if let Ok(val) = serde_json::from_slice::<Value>(data) {
        let _ = serde_json::from_value::<Message>(val);
    }
    if let Ok(input) = arbitrary::Unstructured::new(data).arbitrary::<FuzzInput>() {
        // A string ID over 64 bytes can be built locally, but is rejected when decoding.
        let oversized_id = !matches!(input.kind, MsgKind::Notification)
            && input.string_id.as_ref().is_some_and(|id| id.len() > 64);
        let msg: Message = input.into();
        test_msg_conversions(msg.clone());
        if oversized_id {
            let line = serde_json::to_string(&msg).expect("messages always serialize");
            assert!(Message::decode(&line).is_err());
        } else {
            round_trip(msg);
        }
    }
});
