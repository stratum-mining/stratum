//! https://www.jsonrpc.org/specification#response_object
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{fmt, fmt::Display};

#[derive(Clone, Serialize, Debug)]
#[serde(untagged)]
pub enum Message {
    StandardRequest(StandardRequest),
    Notification(Notification),
    OkResponse(Response),
    ErrorResponse(Response),
}

impl<'de> Deserialize<'de> for Message {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum MessageWire {
            StandardRequest(StandardRequest),
            Notification(Notification),
            Response(Response),
        }

        Ok(match MessageWire::deserialize(deserializer)? {
            MessageWire::StandardRequest(request) => Self::StandardRequest(request),
            MessageWire::Notification(notification) => Self::Notification(notification),
            MessageWire::Response(response) => response.into(),
        })
    }
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

/// An SV1 response error, serialized as `[code, message, data]`.
///
/// The three-element format is used by the [original Stratum implementation][a] and the
/// [NiceHash extranonce extension][b]. Deserialization additionally accepts `[code, message]`
/// with absent data, and object-form errors, for compatibility.
///
/// [a]: https://github.com/slush0/stratum/blob/master/stratum/protocol.py
/// [b]: https://github.com/nicehash/Specifications/blob/master/NiceHash_extranonce_subscribe_extension.txt
#[derive(Clone, Debug, PartialEq)]
pub struct JsonRpcError {
    pub code: i32, // json do not specify precision which one should be used?
    pub message: String,
    pub data: Option<serde_json::Value>,
}

impl Serialize for JsonRpcError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.code, &self.message, &self.data).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for JsonRpcError {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum JsonRpcErrorWire {
            Legacy((i32, String, Option<serde_json::Value>)),
            LegacyWithoutData((i32, String)),
            Object {
                code: i32,
                message: String,
                data: Option<serde_json::Value>,
            },
        }

        let (code, message, data) = match JsonRpcErrorWire::deserialize(deserializer)? {
            JsonRpcErrorWire::Legacy((code, message, data))
            | JsonRpcErrorWire::Object {
                code,
                message,
                data,
            } => (code, message, data),
            JsonRpcErrorWire::LegacyWithoutData((code, message)) => (code, message, None),
        };
        Ok(Self {
            code,
            message,
            data,
        })
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_legacy_sv1_error_array() {
        let response: Response = serde_json::from_value(serde_json::json!({
            "id": 42,
            "result": null,
            "error": [22, "Duplicate share", null],
        }))
        .unwrap();

        assert_eq!(
            response.error,
            Some(JsonRpcError {
                code: 22,
                message: "Duplicate share".to_string(),
                data: None,
            })
        );
    }

    #[test]
    fn deserializes_two_element_errors_and_serializes_three_elements() {
        for code in 20..=25 {
            let message: Message = serde_json::from_value(serde_json::json!({
                "id": 42,
                "result": null,
                "error": [code, "Rejected"],
            }))
            .unwrap();

            let Message::ErrorResponse(response) = &message else {
                panic!("a two-element error must remain an error response");
            };
            assert_eq!(
                response.error,
                Some(JsonRpcError {
                    code,
                    message: "Rejected".to_string(),
                    data: None,
                })
            );
            assert_eq!(
                serde_json::to_value(&message).unwrap(),
                serde_json::json!({
                    "id": 42,
                    "result": null,
                    "error": [code, "Rejected", null],
                })
            );
            assert!(matches!(
                crate::methods::Method::try_from(message),
                Ok(crate::methods::Method::ErrorMessage(_))
            ));
        }
    }

    #[test]
    fn legacy_error_round_trip_preserves_additional_data() {
        let wire = serde_json::json!([20, "Rejected", {"detail": "invalid share"}]);
        let error: JsonRpcError = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(
            error.data,
            Some(serde_json::json!({"detail": "invalid share"}))
        );
        assert_eq!(serde_json::to_value(error).unwrap(), wire);
    }

    #[test]
    fn rejects_legacy_errors_with_invalid_types_or_arity() {
        for wire in [
            serde_json::json!([]),
            serde_json::json!([20]),
            serde_json::json!([20, "Rejected", null, null]),
            serde_json::json!(["20", "Rejected"]),
            serde_json::json!(["20", "Rejected", null]),
            serde_json::json!([20.5, "Rejected"]),
            serde_json::json!([20, 42]),
            serde_json::json!([20, 42, null]),
        ] {
            assert!(
                serde_json::from_value::<JsonRpcError>(wire.clone()).is_err(),
                "invalid error unexpectedly accepted: {wire}"
            );
        }
    }

    #[test]
    fn deserializes_json_rpc_error_object_for_compatibility() {
        let response: Response = serde_json::from_value(serde_json::json!({
            "id": 42,
            "result": null,
            "error": {
                "code": 22,
                "message": "Duplicate share",
                "data": null,
            },
        }))
        .unwrap();

        assert_eq!(
            response.error,
            Some(JsonRpcError {
                code: 22,
                message: "Duplicate share".to_string(),
                data: None,
            })
        );
    }

    #[test]
    fn message_classifies_response_from_error_field() {
        let error: Message = serde_json::from_value(serde_json::json!({
            "id": 42,
            "result": null,
            "error": [22, "Duplicate share", null],
        }))
        .unwrap();
        assert!(matches!(&error, Message::ErrorResponse(_)));
        assert!(matches!(
            crate::methods::Method::try_from(error),
            Ok(crate::methods::Method::ErrorMessage(_))
        ));

        let success: Message = serde_json::from_value(serde_json::json!({
            "id": 43,
            "result": true,
            "error": null,
        }))
        .unwrap();
        assert!(matches!(success, Message::OkResponse(_)));
    }
}
