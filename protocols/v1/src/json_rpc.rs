//! https://www.jsonrpc.org/specification#response_object
use serde::{Deserialize, Serialize};
use sv2_serde_json_macros::{DeJson, SerJson};
use sv2_serde_json::value::{ToJsonValue, FromJsonValue};

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum Message {
    StandardRequest(StandardRequest),
    Notification(Notification),
    OkResponse(Response),
    ErrorResponse(Response),
}

#[derive(Clone, SerJson, DeJson, Debug)]
pub enum Message_ {
    StandardRequest(StandardRequest_),
    Notification(Notification_),
    OkResponse(Response_),
    ErrorResponse(Response_),
}

impl Message {
    // TODO REMOVE it
    pub fn is_response(&self) -> bool {
        match self {
            Message::StandardRequest(_) => false,
            Message::Notification(_) => false,
            Message::OkResponse(_) => true,
            Message::ErrorResponse(_) => true,
        }
    }

    //pub fn error(&self) -> Option<JsonRpcError> {
    //    match self {
    //        Message::Response(r) => r.error.clone(),
    //        _ => None,
    //    }
    //}
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct StandardRequest {
    pub id: u64,
    pub method: String,
    pub params: serde_json::Value,
}

#[derive(SerJson, DeJson, Clone, Debug, PartialEq)]
pub struct StandardRequest_ {
    pub id: u64,
    pub method: String,
    pub params: sv2_serde_json::value::Value
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Notification {
    pub method: String,
    pub params: serde_json::Value,
}

#[derive(Clone, SerJson, DeJson, Debug)]
pub struct Notification_ {
    pub method: String,
    pub params: sv2_serde_json::value::Value,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Response {
    pub id: u64,
    pub error: Option<JsonRpcError>,
    pub result: serde_json::Value,
}

#[derive(Clone, SerJson, DeJson, Debug)]
pub struct Response_ {
    pub id: u64,
    pub error: Option<JsonRpcError_>,
    pub result: sv2_serde_json::value::Value,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct JsonRpcError {
    pub code: i32, // json do not specify precision which one should be used?
    pub message: String,
    pub data: Option<serde_json::Value>,
}

#[derive(Clone, SerJson, DeJson, Debug)]
pub struct JsonRpcError_ {
    pub code: i32, // json do not specify precision which one should be used?
    pub message: String,
    pub data: Option<sv2_serde_json::value::Value>,
}

impl From<Response> for Message {
    fn from(res: Response) -> Self {
        if res.error.is_some() {
            Message::ErrorResponse(res)
        } else {
            Message::OkResponse(res)
        }
    }
}

impl From<StandardRequest> for Message {
    fn from(sr: StandardRequest) -> Self {
        Message::StandardRequest(sr)
    }
}

impl From<Notification> for Message {
    fn from(n: Notification) -> Self {
        Message::Notification(n)
    }
}
