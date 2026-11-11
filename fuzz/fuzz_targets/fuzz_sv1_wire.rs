#![no_main]

mod common;

use std::convert::TryFrom;

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use serde_json::Value;
use sv1_api::{
    json_rpc::Request, ClientRequest, ConfigureResult, Id, Message, Method, Response, RpcError,
    ServerNotification, SubscribeResult,
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
            MsgKind::Request => Message::Request(Request {
                id: Some(id),
                method: Method::from(fi.method),
                params: fi.params.0,
            }),
            MsgKind::Notification => Message::Request(Request {
                id: None,
                method: Method::from(fi.method),
                params: fi.params.0,
            }),
            MsgKind::OkResponse => Message::Response(Response::success(id, fi.result.0)),
            MsgKind::ErrorResponse => Message::Response(Response::failure(
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

fn test_msg_conversions(msg: Message) {
    let _ = format!("{}", msg);
    let _ = format!("{:?}", msg);

    match msg {
        Message::Request(request) => {
            if let Ok(m) = ClientRequest::try_from(request.clone()) {
                let _ = format!("{:?}", m);
                let line = serde_json::to_string(&Message::from(m)).unwrap();
                if let Ok(Message::Request(again)) = Message::decode(&line) {
                    ClientRequest::try_from(again).expect("encoded requests always decode");
                }
            }
            if let Ok(m) = ServerNotification::try_from(request) {
                let _ = format!("{:?}", m);
                let line = serde_json::to_string(&Message::from(m.clone())).unwrap();
                if let Ok(Message::Request(again)) = Message::decode(&line) {
                    let again = ServerNotification::try_from(again)
                        .expect("encoded notifications always decode");
                    assert_eq!(again, m);
                }
            }
        }
        Message::Response(response) => {
            let _ = response.result_as::<SubscribeResult>();
            let _ = response.result_as::<ConfigureResult>();
            let _ = response.result_as::<bool>();
        }
    }
}

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        if let Ok(msg) = Message::decode(s) {
            test_msg_conversions(msg.clone());
            round_trip(msg);
        }
    }
    if let Ok(val) = serde_json::from_slice::<Value>(data) {
        let _ = serde_json::from_value::<Message>(val);
    }
    if let Ok(input) = arbitrary::Unstructured::new(data).arbitrary::<FuzzInput>() {
        let msg: Message = input.into();
        test_msg_conversions(msg.clone());
        round_trip(msg);
    }
});
