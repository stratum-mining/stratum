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
            Message::StandardRequest(sr) => write!(f, "{}", sr),
            Message::Notification(n) => write!(f, "{}", n),
            Message::OkResponse(r) => write!(f, "{}", r),
            Message::ErrorResponse(r) => write!(f, "{}", r),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct StandardRequest {
    pub id: u64,
    pub method: String,
    pub params: serde_json::Value,
}

impl fmt::Display for StandardRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let params =
            serde_json::to_string_pretty(&self.params).unwrap_or_else(|_| self.params.to_string());
        write!(
            f,
            "{{ id: {}, method: {}, params: {} }}",
            self.id, self.method, params
        )
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Notification {
    pub method: String,
    pub params: serde_json::Value,
}

impl fmt::Display for Notification {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let params =
            serde_json::to_string_pretty(&self.params).unwrap_or_else(|_| self.params.to_string());

        write!(f, "{{ method: \"{}\", params: {} }}", self.method, params)
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Response {
    pub id: u64,
    pub error: Option<JsonRpcError>,
    pub result: serde_json::Value,
}

impl fmt::Display for Response {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let result =
            serde_json::to_string_pretty(&self.result).unwrap_or_else(|_| self.result.to_string());

        if let Some(err) = &self.error {
            write!(
                f,
                "{{ id: {}, error: {:?}, result: {} }}",
                self.id, err, result
            )
        } else {
            write!(f, "{{ id: {}, result: {} }}", self.id, result)
        }
    }
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
