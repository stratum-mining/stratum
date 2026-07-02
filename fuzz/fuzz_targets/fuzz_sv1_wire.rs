#![no_main]

mod common;

use std::convert::TryFrom;

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use serde_json::Value;
use sv1_api::{
    json_rpc::{JsonRpcError, Message, Notification, Response, StandardRequest},
    methods::{Client2Server, Method, Server2Client, Server2ClientResponse},
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
    StandardRequest,
    Notification,
    OkResponse,
    ErrorResponse,
}

#[derive(Arbitrary, Debug)]
struct FuzzInput {
    id: u64,
    method: String,
    params: ArbJson,
    result: ArbJson,
    error_code: i32,
    error_message: String,
    kind: MsgKind,
}

impl From<FuzzInput> for Message {
    fn from(fi: FuzzInput) -> Self {
        match fi.kind {
            MsgKind::StandardRequest => Message::StandardRequest(StandardRequest {
                id: fi.id,
                method: fi.method,
                params: fi.params.0,
            }),
            MsgKind::Notification => Message::Notification(Notification {
                method: fi.method,
                params: fi.params.0,
            }),
            MsgKind::OkResponse => Message::OkResponse(Response {
                id: fi.id,
                error: None,
                result: fi.result.0,
            }),
            MsgKind::ErrorResponse => Message::ErrorResponse(Response {
                id: fi.id,
                error: Some(JsonRpcError {
                    code: fi.error_code,
                    message: fi.error_message,
                    data: None,
                }),
                result: fi.result.0,
            }),
        }
    }
}

fn test_msg_conversions(msg: Message) {
    let _ = format!("{}", msg);
    let _ = format!("{:?}", msg);

    if let Ok(m) = Client2Server::try_from(msg.clone()) {
        let _ = format!("{:?}", m);
        if let Ok(m) = Method::try_from(msg.clone()) {
            let _ = format!("{:?}", m);
        }
    }

    if let Ok(m) = Server2ClientResponse::try_from(msg.clone()) {
        let _ = format!("{:?}", m);
        if let Ok(m) = Method::try_from(msg.clone()) {
            let _ = format!("{:?}", m);
        }
    }

    if let Ok(m) = Server2Client::try_from(msg.clone()) {
        let _ = format!("{:?}", m);
        if let Ok(m) = Method::try_from(msg.clone()) {
            let _ = format!("{:?}", m);
        }
    }

    if matches!(&msg, Message::ErrorResponse(_)) {
        if let Ok(Method::ErrorMessage(m)) = Method::try_from(msg) {
            let _ = format!("{:?}", m);
        }
    }
}

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        if let Ok(msg) = serde_json::from_str::<Message>(s) {
            test_msg_conversions(msg);
        }
    }
    if let Ok(val) = serde_json::from_slice::<Value>(data) {
        let _ = serde_json::from_value::<Message>(val);
    }
    if let Ok(input) = arbitrary::Unstructured::new(data).arbitrary::<FuzzInput>() {
        let msg: Message = input.into();
        let msg_for_serde = msg.clone();

        test_msg_conversions(msg);

        if let Ok(json) = serde_json::to_string(&msg_for_serde) {
            let _ = serde_json::from_str::<Message>(&json);
        }
    }
});
