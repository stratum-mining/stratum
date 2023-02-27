//! https://www.jsonrpc.org/specification#response_object
use serde::{de, Deserialize, Deserializer, Serialize};
use std::fmt;

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum Message {
    StandardRequest(StandardRequest),
    Notification(Notification),
    OkResponse(Response),
    ErrorResponse(Response),
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

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct StandardRequest {
    #[serde(deserialize_with = "deserialize_string_and_number_into_string")]
    pub id: String, // can be number
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
    pub id: String, // can be number
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

struct DeserializeStringAndNumberIntoString;

impl<'de> de::Visitor<'de> for DeserializeStringAndNumberIntoString {
    type Value = String;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an integer or a string")
    }

    fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(v.to_string())
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(v.to_string())
    }
}

fn deserialize_string_and_number_into_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_any(DeserializeStringAndNumberIntoString)
}
