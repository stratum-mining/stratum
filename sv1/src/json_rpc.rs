//! SV1 message envelope: one newline-delimited JSON object.
//!
//! An object with a string `method` is a [`Request`]; it is a notification when its `id` is
//! absent or null. Any other object is a [`Response`].

use crate::error::{DecodeError, DecodeErrorKind, ResponseError};
use serde::{
    de::{self, DeserializeOwned, IgnoredAny, MapAccess, Visitor},
    ser::SerializeMap,
    Deserialize, Deserializer, Serialize, Serializer,
};
use serde_json::Value;
use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    Request(Request),
    Response(Response),
}

#[derive(Clone, PartialEq)]
pub struct Request {
    /// `None` makes this a notification, which never gets a reply.
    pub id: Option<Id>,
    pub method: Method,
    pub params: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Response {
    /// `None` for an error the peer could not correlate with a request.
    pub id: Option<Id>,
    pub result: Result<Value, RpcError>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Method {
    Configure,
    Subscribe,
    Authorize,
    Submit,
    ExtranonceSubscribe,
    SuggestDifficulty,
    Notify,
    SetDifficulty,
    SetExtranonce,
    SetVersionMask,
    Other(String),
}

/// A request ID: an unsigned integer, a negative integer, or a string.
///
/// Numeric `1` and string `"1"` are different IDs.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Id(IdRepr);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum IdRepr {
    Unsigned(u64),
    Negative(i64),
    String(String),
}

/// An SV1 error, serialized as `[code, message, data]`.
///
/// The three-element format is used by the [original Stratum implementation][a] and the
/// [NiceHash extranonce extension][b]. Deserialization additionally accepts `[code, message]`
/// with absent data, and object-form errors, for compatibility.
///
/// [a]: https://github.com/slush0/stratum/blob/master/stratum/protocol.py
/// [b]: https://github.com/nicehash/Specifications/blob/master/NiceHash_extranonce_subscribe_extension.txt
#[derive(Clone, Debug, PartialEq)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    pub data: Option<Value>,
}

impl Message {
    pub(crate) fn request(id: Option<Id>, method: Method, params: Value) -> Self {
        Message::Request(Request { id, method, params })
    }

    pub(crate) fn notification(method: Method, params: Vec<Value>) -> Self {
        Message::request(None, method, Value::Array(params))
    }

    pub fn decode(line: &str) -> Result<Message, DecodeError> {
        let raw: RawEnvelope =
            serde_json::from_str(line).map_err(|e| DecodeError::new(DecodeErrorKind::Json(e)))?;
        raw.classify()
    }
}

impl From<Response> for Message {
    fn from(response: Response) -> Self {
        Message::Response(response)
    }
}

impl Response {
    pub fn success(id: Id, result: impl Into<Value>) -> Self {
        Response {
            id: Some(id),
            result: Ok(result.into()),
        }
    }

    pub fn failure(id: Option<Id>, error: RpcError) -> Self {
        Response {
            id,
            result: Err(error),
        }
    }

    /// Returns the peer's error, or decodes the result as `T`.
    pub fn result_as<T: DeserializeOwned>(&self) -> Result<T, ResponseError> {
        match &self.result {
            Ok(value) => T::deserialize(value).map_err(ResponseError::InvalidResult),
            Err(error) => Err(ResponseError::Rpc(error.clone())),
        }
    }
}

impl Method {
    pub fn as_str(&self) -> &str {
        match self {
            Method::Configure => "mining.configure",
            Method::Subscribe => "mining.subscribe",
            Method::Authorize => "mining.authorize",
            Method::Submit => "mining.submit",
            Method::ExtranonceSubscribe => "mining.extranonce.subscribe",
            Method::SuggestDifficulty => "mining.suggest_difficulty",
            Method::Notify => "mining.notify",
            Method::SetDifficulty => "mining.set_difficulty",
            Method::SetExtranonce => "mining.set_extranonce",
            Method::SetVersionMask => "mining.set_version_mask",
            Method::Other(method) => method,
        }
    }

    fn known(method: &str) -> Option<Method> {
        Some(match method {
            "mining.configure" => Method::Configure,
            "mining.subscribe" => Method::Subscribe,
            "mining.authorize" => Method::Authorize,
            "mining.submit" => Method::Submit,
            "mining.extranonce.subscribe" => Method::ExtranonceSubscribe,
            "mining.suggest_difficulty" => Method::SuggestDifficulty,
            "mining.notify" => Method::Notify,
            "mining.set_difficulty" => Method::SetDifficulty,
            "mining.set_extranonce" => Method::SetExtranonce,
            "mining.set_version_mask" => Method::SetVersionMask,
            _ => return None,
        })
    }
}

impl From<&str> for Method {
    fn from(method: &str) -> Self {
        Method::known(method).unwrap_or_else(|| Method::Other(method.to_string()))
    }
}

impl From<String> for Method {
    fn from(method: String) -> Self {
        Method::known(&method).unwrap_or(Method::Other(method))
    }
}

/// Unknown method names come from the peer, so they are escaped.
impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Method::Other(method) => write!(f, "{method:?}"),
            known => f.write_str(known.as_str()),
        }
    }
}

impl Id {
    /// The length of a string ID; numeric IDs count as their longest decimal form.
    pub(crate) fn text_len(&self) -> usize {
        match &self.0 {
            IdRepr::String(id) => id.len(),
            IdRepr::Unsigned(_) | IdRepr::Negative(_) => 20,
        }
    }

    fn from_value(value: Value) -> Result<Id, &'static str> {
        match value {
            Value::Number(number) => {
                if let Some(id) = number.as_u64() {
                    Ok(Id(IdRepr::Unsigned(id)))
                } else if let Some(id) = number.as_i64() {
                    Ok(Id(IdRepr::Negative(id)))
                } else {
                    Err("id is not an integer")
                }
            }
            Value::String(id) => Ok(Id(IdRepr::String(id))),
            _ => Err("id is not a number or a string"),
        }
    }
}

impl From<u64> for Id {
    fn from(id: u64) -> Self {
        Id(IdRepr::Unsigned(id))
    }
}

impl From<i64> for Id {
    fn from(id: i64) -> Self {
        match u64::try_from(id) {
            Ok(id) => Id(IdRepr::Unsigned(id)),
            Err(_) => Id(IdRepr::Negative(id)),
        }
    }
}

impl From<String> for Id {
    fn from(id: String) -> Self {
        Id(IdRepr::String(id))
    }
}

impl From<&str> for Id {
    fn from(id: &str) -> Self {
        Id(IdRepr::String(id.to_string()))
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            IdRepr::Unsigned(id) => write!(f, "{id}"),
            IdRepr::Negative(id) => write!(f, "{id}"),
            IdRepr::String(id) => write!(f, "{id:?}"),
        }
    }
}

impl Serialize for Id {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match &self.0 {
            IdRepr::Unsigned(id) => serializer.serialize_u64(*id),
            IdRepr::Negative(id) => serializer.serialize_i64(*id),
            IdRepr::String(id) => serializer.serialize_str(id),
        }
    }
}

impl Serialize for RpcError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        (&self.code, &self.message, &self.data).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RpcError {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum RpcErrorWire {
            Legacy((i32, String, Option<Value>)),
            LegacyWithoutData((i32, String)),
            Object {
                code: i32,
                message: String,
                data: Option<Value>,
            },
        }

        let (code, message, data) = match RpcErrorWire::deserialize(deserializer)? {
            RpcErrorWire::Legacy((code, message, data))
            | RpcErrorWire::Object {
                code,
                message,
                data,
            } => (code, message, data),
            RpcErrorWire::LegacyWithoutData((code, message)) => (code, message, None),
        };
        Ok(RpcError {
            code,
            message,
            data,
        })
    }
}

impl Serialize for Message {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Message::Request(request) => request.serialize(serializer),
            Message::Response(response) => response.serialize(serializer),
        }
    }
}

impl Serialize for Request {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(3))?;
        map.serialize_entry("id", &self.id)?;
        map.serialize_entry("method", self.method.as_str())?;
        map.serialize_entry("params", &self.params)?;
        map.end()
    }
}

impl Serialize for Response {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let (result, error) = match &self.result {
            Ok(result) => (result, None),
            Err(error) => (&Value::Null, Some(error)),
        };
        let mut map = serializer.serialize_map(Some(3))?;
        map.serialize_entry("id", &self.id)?;
        map.serialize_entry("result", result)?;
        map.serialize_entry("error", &error)?;
        map.end()
    }
}

impl<'de> Deserialize<'de> for Message {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        RawEnvelope::deserialize(deserializer)?
            .classify()
            .map_err(de::Error::custom)
    }
}

/// The reserved envelope members, each recorded only when present.
#[derive(Default)]
struct RawEnvelope {
    id: Option<Value>,
    method: Option<Value>,
    params: Option<Value>,
    result: Option<Value>,
    error: Option<Value>,
}

impl RawEnvelope {
    fn classify(self) -> Result<Message, DecodeError> {
        let id = match self.id {
            None | Some(Value::Null) => None,
            Some(id) => Some(Id::from_value(id).map_err(|reason| {
                DecodeError::about(None, None, DecodeErrorKind::InvalidEnvelope(reason))
            })?),
        };
        let invalid =
            |reason| DecodeError::about(id.clone(), None, DecodeErrorKind::InvalidEnvelope(reason));
        match self.method {
            Some(Value::String(method)) => {
                let method = Method::from(method);
                let Some(params) = self.params else {
                    return Err(DecodeError::about(
                        id,
                        Some(method),
                        DecodeErrorKind::InvalidEnvelope("missing params"),
                    ));
                };
                Ok(Message::Request(Request { id, method, params }))
            }
            None | Some(Value::Null) => {
                let result = match self.error {
                    Some(error) if !error.is_null() => Err(serde_json::from_value(error)
                        .map_err(|_| invalid("error is not an SV1 error"))?),
                    _ => Ok(self.result.ok_or_else(|| invalid("missing result"))?),
                };
                Ok(Message::Response(Response { id, result }))
            }
            Some(_) => Err(invalid("method is not a string")),
        }
    }
}

impl<'de> Deserialize<'de> for RawEnvelope {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(RawEnvelopeVisitor)
    }
}

struct RawEnvelopeVisitor;

impl<'de> Visitor<'de> for RawEnvelopeVisitor {
    type Value = RawEnvelope;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an SV1 message object")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<RawEnvelope, A::Error> {
        fn member<'de, A: MapAccess<'de>>(
            map: &mut A,
            slot: &mut Option<Value>,
            name: &'static str,
        ) -> Result<(), A::Error> {
            if slot.is_some() {
                return Err(de::Error::duplicate_field(name));
            }
            *slot = Some(map.next_value()?);
            Ok(())
        }

        let mut raw = RawEnvelope::default();
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "id" => member(&mut map, &mut raw.id, "id")?,
                "method" => member(&mut map, &mut raw.method, "method")?,
                "params" => member(&mut map, &mut raw.params, "params")?,
                "result" => member(&mut map, &mut raw.result, "result")?,
                "error" => member(&mut map, &mut raw.error, "error")?,
                _ => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(raw)
    }
}

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Message::Request(request) => request.fmt(f),
            Message::Response(response) => response.fmt(f),
        }
    }
}

fn display_id(id: &Option<Id>) -> String {
    id.as_ref().map_or("null".to_string(), Id::to_string)
}

impl fmt::Display for Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{ id: {}, method: {}, params: {} }}",
            display_id(&self.id),
            self.method,
            RedactedParams(self)
        )
    }
}

impl fmt::Debug for Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Request")
            .field("id", &self.id)
            .field("method", &self.method)
            .field("params", &format_args!("{}", RedactedParams(self)))
            .finish()
    }
}

/// `mining.authorize` params carry the worker password, and a configure `info.connection-url`
/// may carry credentials.
struct RedactedParams<'a>(&'a Request);

const CONNECTION_URL: &str = "/1/info.connection-url";

impl fmt::Display for RedactedParams<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let params = &self.0.params;
        match self.0.method {
            Method::Authorize => f.write_str("<redacted>"),
            Method::Configure if params.pointer(CONNECTION_URL).is_some() => {
                let mut params = params.clone();
                if let Some(url) = params.pointer_mut(CONNECTION_URL) {
                    *url = Value::from("<redacted>");
                }
                write!(f, "{params}")
            }
            _ => write!(f, "{params}"),
        }
    }
}

impl fmt::Display for Response {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.result {
            Ok(result) => write!(f, "{{ id: {}, result: {result} }}", display_id(&self.id)),
            Err(error) => write!(
                f,
                "{{ id: {}, error: [{}, {:?}] }}",
                display_id(&self.id),
                error.code,
                error.message
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn decode(line: &str) -> Message {
        Message::decode(line).unwrap_or_else(|e| panic!("{line}: {e}"))
    }

    fn request(line: &str) -> Request {
        match decode(line) {
            Message::Request(request) => request,
            other => panic!("{line}: not a request: {other:?}"),
        }
    }

    fn response(line: &str) -> Response {
        match decode(line) {
            Message::Response(response) => response,
            other => panic!("{line}: not a response: {other:?}"),
        }
    }

    fn reject(line: &str) -> DecodeError {
        match Message::decode(line) {
            Ok(message) => panic!("{line}: unexpectedly decoded as {message:?}"),
            Err(error) => error,
        }
    }

    #[test]
    fn method_classifies_requests() {
        for (line, id) in [
            (
                r#"{"id":1,"method":"mining.subscribe","params":[]}"#,
                Some(Id::from(1u64)),
            ),
            (
                r#"{"jsonrpc":"2.0","id":1,"method":"mining.subscribe","params":[]}"#,
                Some(Id::from(1u64)),
            ),
            (
                r#"{"id":1,"method":"mining.subscribe","params":[],"error":null}"#,
                Some(Id::from(1u64)),
            ),
            (
                r#"{"id":"1","method":"mining.subscribe","params":[]}"#,
                Some(Id::from("1")),
            ),
            (
                r#"{"id":-3,"method":"mining.subscribe","params":[]}"#,
                Some(Id::from(-3i64)),
            ),
            (
                r#"{"id":18446744073709551615,"method":"mining.subscribe","params":[]}"#,
                Some(Id::from(u64::MAX)),
            ),
            (r#"{"method":"mining.set_difficulty","params":[2]}"#, None),
            (
                r#"{"id":null,"method":"mining.set_difficulty","params":[2]}"#,
                None,
            ),
            (
                r#"{"id":null,"method":"mining.set_difficulty","params":[2],"result":null,"error":null}"#,
                None,
            ),
        ] {
            assert_eq!(request(line).id, id, "{line}");
        }
    }

    #[test]
    fn null_params_are_kept() {
        let request = request(r#"{"id":1,"method":"mining.extranonce.subscribe","params":null}"#);
        assert_eq!(request.method, Method::ExtranonceSubscribe);
        assert_eq!(request.params, Value::Null);
    }

    #[test]
    fn responses() {
        for (line, id, result) in [
            (
                r#"{"id":1,"result":true,"error":null}"#,
                Some(1u64),
                Ok(json!(true)),
            ),
            (r#"{"id":1,"result":true}"#, Some(1), Ok(json!(true))),
            (r#"{"id":1,"result":null}"#, Some(1), Ok(Value::Null)),
            (
                r#"{"id":1,"result":true,"error":null,"method":null,"params":null}"#,
                Some(1),
                Ok(json!(true)),
            ),
            (
                r#"{"id":1,"result":null,"error":[22,"Duplicate share",null]}"#,
                Some(1),
                Err(22),
            ),
            (
                r#"{"id":1,"result":null,"error":[22,"Duplicate share"]}"#,
                Some(1),
                Err(22),
            ),
            (
                r#"{"id":1,"result":null,"error":{"code":22,"message":"Duplicate share","data":null}}"#,
                Some(1),
                Err(22),
            ),
            (
                r#"{"id":1,"error":[22,"Duplicate share",null]}"#,
                Some(1),
                Err(22),
            ),
            (
                r#"{"id":null,"result":null,"error":[20,"Other/Unknown",null]}"#,
                None,
                Err(20),
            ),
            (
                r#"{"id":3,"result":false,"error":[20,"Not supported.",null]}"#,
                Some(3),
                Err(20),
            ),
        ] {
            let response = response(line);
            assert_eq!(response.id, id.map(Id::from), "{line}");
            assert_eq!(response.result.map_err(|e| e.code), result, "{line}");
        }
    }

    #[test]
    fn rejects_malformed_envelopes() {
        for line in [
            r#"{"id":1,"method":"mining.subscribe"}"#,
            r#"{"id":1}"#,
            r#"{"id":1,"error":null}"#,
            r#"{"id":1.0,"method":"mining.subscribe","params":[]}"#,
            r#"{"id":true,"method":"mining.subscribe","params":[]}"#,
            r#"{"id":1,"method":5,"params":[]}"#,
            r#"{"id":1,"id":2,"method":"mining.subscribe","params":[]}"#,
            r#"{"id":1,"result":true,"error":[20]}"#,
            r#"[{"id":1,"method":"mining.subscribe","params":[]}]"#,
            r#"{"id":1,"method":"mining.subscribe","params":[]"#,
        ] {
            reject(line);
        }
    }

    #[test]
    fn errors_carry_only_a_validated_id() {
        let error = reject(r#"{"id":7,"method":"mining.submit"}"#);
        assert_eq!(error.id, Some(Id::from(7u64)));
        assert_eq!(error.method, Some(Method::Submit));

        let error = reject(r#"{"id":1.5,"method":"mining.submit","params":[]}"#);
        assert_eq!(error.id, None);
    }

    #[test]
    fn method_names_round_trip() {
        for method in [
            Method::Configure,
            Method::Subscribe,
            Method::Authorize,
            Method::Submit,
            Method::ExtranonceSubscribe,
            Method::SuggestDifficulty,
            Method::Notify,
            Method::SetDifficulty,
            Method::SetExtranonce,
            Method::SetVersionMask,
            Method::Other("client.reconnect".to_string()),
        ] {
            assert_eq!(Method::from(method.as_str()), method);
            assert_eq!(Method::from(method.as_str().to_string()), method);
        }
    }

    #[test]
    fn encoding() {
        let notification = Message::Request(Request {
            id: None,
            method: Method::SetVersionMask,
            params: json!(["1fffe000"]),
        });
        assert_eq!(
            serde_json::to_value(&notification).unwrap(),
            json!({"id": null, "method": "mining.set_version_mask", "params": ["1fffe000"]})
        );

        let success = Message::Response(Response::success(Id::from("a"), true));
        assert_eq!(
            serde_json::to_value(&success).unwrap(),
            json!({"id": "a", "result": true, "error": null})
        );

        let failure = Message::Response(Response::failure(
            Some(Id::from(4u64)),
            RpcError {
                code: 23,
                message: "Low difficulty share".to_string(),
                data: None,
            },
        ));
        assert_eq!(
            serde_json::to_value(&failure).unwrap(),
            json!({"id": 4, "result": null, "error": [23, "Low difficulty share", null]})
        );

        for message in [notification, success, failure] {
            let line = serde_json::to_string(&message).unwrap();
            assert_eq!(decode(&line), message, "{line}");
            assert_eq!(
                serde_json::from_str::<Message>(&line).unwrap(),
                message,
                "{line}"
            );
        }
    }

    #[test]
    fn ids_are_mirrored_exactly() {
        for id in [
            Id::from(1u64),
            Id::from("1"),
            Id::from(-1i64),
            Id::from(u64::MAX),
        ] {
            let line = serde_json::to_string(&Response::success(id.clone(), true)).unwrap();
            assert_eq!(response(&line).id, Some(id), "{line}");
        }
        assert_ne!(Id::from(1u64), Id::from("1"));
        assert_eq!(Id::from(1u64), Id::from(1i64));
    }

    #[test]
    fn result_as_checks_the_error_first() {
        let response = Response::success(Id::from(1u64), json!([[], "08000002", 4]));
        assert_eq!(
            response
                .result_as::<(Vec<(String, String)>, String, usize)>()
                .unwrap(),
            (vec![], "08000002".to_string(), 4)
        );
        assert!(matches!(
            response.result_as::<bool>(),
            Err(ResponseError::InvalidResult(_))
        ));

        let rejected = Response::failure(
            Some(Id::from(1u64)),
            RpcError {
                code: 22,
                message: "Duplicate share".to_string(),
                data: None,
            },
        );
        assert!(matches!(
            rejected.result_as::<bool>(),
            Err(ResponseError::Rpc(RpcError { code: 22, .. }))
        ));
    }

    #[test]
    fn authorize_params_are_redacted() {
        let authorize = request(r#"{"id":2,"method":"mining.authorize","params":["w","hunter2"]}"#);
        assert!(!authorize.to_string().contains("hunter2"));
        assert!(!format!("{authorize:?}").contains("hunter2"));
        assert!(!Message::Request(authorize).to_string().contains("hunter2"));

        let subscribe = request(r#"{"id":1,"method":"mining.subscribe","params":["cgminer"]}"#);
        assert!(subscribe.to_string().contains("cgminer"));
    }
}
