//! https://www.jsonrpc.org/specification#response_object
use serde::{Deserialize, Serialize};
use serde_json;

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum Message {
    StandardRequest(StandardRequest),
    Notification(Notification),
    Response(Response),
}

impl Message {
    pub fn is_response(&self) -> bool {
        match self {
            Message::Response(_) => true,
            _ => false,
        }
    }

    pub fn error(&self) -> Option<JsonRpcError> {
        match self {
            Message::Response(r) => r.error.clone(),
            _ => None,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct StandardRequest {
    pub id: String, // TODO can be number
    pub method: String,
    pub parameters: serde_json::Value,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Notification {
    pub method: String,
    pub parameters: serde_json::Value,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Response {
    pub id: String, // TODO can be number
    pub error: Option<JsonRpcError>,
    pub result: serde_json::Value,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct JsonRpcError {
    pub code: i32, // TODO json do not specify precision which one should be used?
    pub message: String,
    pub data: Option<serde_json::Value>,
}

impl From<Response> for Message {
    fn from(res: Response) -> Self {
        Message::Response(res)
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
