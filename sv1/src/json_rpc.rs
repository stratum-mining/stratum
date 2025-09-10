//! https://www.jsonrpc.org/specification#response_object
use serde::{Deserialize, Serialize};
use std::{fmt, fmt::Display};

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum Message {
    StandardRequest(StandardRequest),
    Notification(Notification),
    OkResponse(Response),
    ErrorResponse(Response),
}

impl Message {
    // TODO: Remove this. Keeping this in to avoid upgrading the major version of the crate.
    pub fn is_response(&self) -> bool {
        match self {
            Message::StandardRequest(_) => false,
            Message::Notification(_) => false,
            Message::OkResponse(_) => true,
            Message::ErrorResponse(_) => true,
        }
    }
}

impl Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Message::StandardRequest(sr) => write!(f, "{:?}", sr.method),
            Message::Notification(n) => write!(f, "{:?}", n.method),
            Message::OkResponse(_) => write!(f, "\"result\": true"),
            Message::ErrorResponse(r) => write!(
                f,
                "\"result\": false, \"error\": {:?}",
                r.error.as_ref().unwrap().message
            ),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct StandardRequest {
    pub id: u64,
    pub method: String,
    pub params: serde_json::Value,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Notification {
    pub method: String,
    pub params: serde_json::Value,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Response {
    pub id: u64,
    pub error: Option<JsonRpcError>,
    pub result: serde_json::Value,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct JsonRpcError {
    pub code: i32, // json do not specify precision which one should be used?
    pub message: String,
    pub data: Option<serde_json::Value>,
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
