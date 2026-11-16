//! Typed SV1 messages, grouped by operation.
//!
//! [`ClientToServer`] and [`ServerToClient`] name the direction a message travels. A request and
//! its reply are [`Request`] and [`Response`] envelopes around the operation's own types.

mod authorize;
mod configure;
mod notifications;
mod submit;
mod subscribe;
mod suggest_difficulty;

pub use authorize::Authorize;
pub use configure::{
    Configure, ConfigureExtension, ConfigureResult, InfoParams, VersionRolling,
    VersionRollingResult,
};
pub use notifications::{Notify, SetDifficulty, SetExtranonce, SetVersionMask};
pub use submit::{Submit, SubmitError, SubmitOutcome};
pub use subscribe::{ExtranonceSubscribe, Subscribe, SubscribeResult};
pub use suggest_difficulty::SuggestDifficulty;

use crate::{
    error::{DecodeError, DecodeErrorKind, Error},
    json_rpc::{self, decode_value, Id, Message, Method, RpcError},
};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::convert::TryFrom;

/// A request, with the ID its reply must echo.
#[derive(Clone, Debug, PartialEq)]
pub struct Request<P> {
    pub id: Id,
    pub params: P,
}

/// The reply to a request: its result, or the error the server answered with.
#[derive(Clone, Debug, PartialEq)]
pub struct Response<R> {
    pub id: Id,
    pub result: Result<R, RpcError>,
}

impl<R> Response<R> {
    pub fn success(id: Id, result: R) -> Self {
        Response {
            id,
            result: Ok(result),
        }
    }

    pub fn failure(id: Id, error: RpcError) -> Self {
        Response {
            id,
            result: Err(error),
        }
    }
}

impl<R: DeserializeOwned> Response<R> {
    /// Types the reply to the `method` request with this ID. A malformed result keeps both, so
    /// the caller can tell which request failed.
    fn decode(id: Id, method: &Method, result: &Result<Value, RpcError>) -> Result<Self, Error> {
        let result = match result {
            Ok(value) => match decode_value(value) {
                Ok(result) => Ok(result),
                Err(error) => {
                    return Err(Error::InvalidResult {
                        id,
                        method: method.clone(),
                        error,
                    })
                }
            },
            Err(error) => Err(error.clone()),
        };
        Ok(Response { id, result })
    }
}

impl<R: Into<Value>> From<Response<R>> for json_rpc::Response {
    fn from(response: Response<R>) -> Self {
        json_rpc::Response {
            id: Some(response.id),
            result: response.result.map(Into::into),
        }
    }
}

impl<R: Into<Value>> From<Response<R>> for Message {
    fn from(response: Response<R>) -> Self {
        Message::Response(response.into())
    }
}

/// A message a client sends to a server.
#[derive(Clone, Debug, PartialEq)]
pub enum ClientToServer {
    Configure(Request<Configure>),
    Subscribe(Request<Subscribe>),
    Authorize(Request<Authorize>),
    Submit(Request<Submit>),
    ExtranonceSubscribe(Request<ExtranonceSubscribe>),
    /// Servers are not required to reply.
    SuggestDifficulty(Request<SuggestDifficulty>),
}

impl ClientToServer {
    pub fn method(&self) -> Method {
        match self {
            ClientToServer::Configure(_) => Method::Configure,
            ClientToServer::Subscribe(_) => Method::Subscribe,
            ClientToServer::Authorize(_) => Method::Authorize,
            ClientToServer::Submit(_) => Method::Submit,
            ClientToServer::ExtranonceSubscribe(_) => Method::ExtranonceSubscribe,
            ClientToServer::SuggestDifficulty(_) => Method::SuggestDifficulty,
        }
    }

    pub fn id(&self) -> &Id {
        match self {
            ClientToServer::Configure(Request { id, .. })
            | ClientToServer::Subscribe(Request { id, .. })
            | ClientToServer::Authorize(Request { id, .. })
            | ClientToServer::Submit(Request { id, .. })
            | ClientToServer::ExtranonceSubscribe(Request { id, .. })
            | ClientToServer::SuggestDifficulty(Request { id, .. }) => id,
        }
    }
}

impl TryFrom<json_rpc::Request> for ClientToServer {
    type Error = DecodeError;

    fn try_from(request: json_rpc::Request) -> Result<Self, DecodeError> {
        let json_rpc::Request {
            id,
            method,
            mut params,
        } = request;
        let error = |kind| DecodeError::about(id.clone(), Some(method.clone()), kind);
        let Some(id) = id.clone() else {
            return Err(error(DecodeErrorKind::MissingId));
        };
        let message = match &method {
            Method::Configure => Configure::from_params(&params)
                .map(|params| ClientToServer::Configure(Request { id, params })),
            Method::Subscribe => Subscribe::from_params(&mut params)
                .map(|params| ClientToServer::Subscribe(Request { id, params })),
            Method::Authorize => Authorize::from_params(&mut params)
                .map(|params| ClientToServer::Authorize(Request { id, params })),
            Method::Submit => Submit::from_params(&mut params)
                .map(|params| ClientToServer::Submit(Request { id, params })),
            Method::ExtranonceSubscribe => Ok(ClientToServer::ExtranonceSubscribe(Request {
                id,
                params: ExtranonceSubscribe,
            })),
            Method::SuggestDifficulty => Ok(ClientToServer::SuggestDifficulty(Request {
                id,
                params: SuggestDifficulty::from_params(&params),
            })),
            _ => return Err(error(DecodeErrorKind::UnknownMethod)),
        };
        message.map_err(|e| error(e.into()))
    }
}

/// A response is an error: only requests are expected.
impl TryFrom<Message> for ClientToServer {
    type Error = DecodeError;

    fn try_from(message: Message) -> Result<Self, DecodeError> {
        match message {
            Message::Request(request) => ClientToServer::try_from(request),
            Message::Response(response) => Err(DecodeError::about(
                response.id,
                None,
                DecodeErrorKind::UnexpectedResponse,
            )),
        }
    }
}

impl From<ClientToServer> for Message {
    fn from(message: ClientToServer) -> Self {
        let method = message.method();
        let (id, params) = match message {
            ClientToServer::Configure(Request { id, params }) => (id, params.to_params()),
            ClientToServer::Subscribe(Request { id, params }) => (id, params.to_params()),
            ClientToServer::Authorize(Request { id, params }) => (id, params.to_params()),
            ClientToServer::Submit(Request { id, params }) => (id, params.to_params()),
            ClientToServer::ExtranonceSubscribe(Request { id, .. }) => (id, Value::Array(vec![])),
            ClientToServer::SuggestDifficulty(Request { id, params }) => (id, params.to_params()),
        };
        Message::request(Some(id), method, params)
    }
}

/// A message a server sends to a client: the reply to a request, or a notification.
#[derive(Clone, Debug, PartialEq)]
pub enum ServerToClient {
    Configure(Response<ConfigureResult>),
    Subscribe(Response<SubscribeResult>),
    Authorize(Response<bool>),
    Submit(Response<bool>),
    ExtranonceSubscribe(Response<bool>),
    Notify(Notify),
    SetDifficulty(SetDifficulty),
    SetExtranonce(SetExtranonce),
    SetVersionMask(SetVersionMask),
}

impl ServerToClient {
    /// The method of the notification, or of the request a reply answers.
    pub fn method(&self) -> Method {
        match self {
            ServerToClient::Configure(_) => Method::Configure,
            ServerToClient::Subscribe(_) => Method::Subscribe,
            ServerToClient::Authorize(_) => Method::Authorize,
            ServerToClient::Submit(_) => Method::Submit,
            ServerToClient::ExtranonceSubscribe(_) => Method::ExtranonceSubscribe,
            ServerToClient::Notify(_) => Method::Notify,
            ServerToClient::SetDifficulty(_) => Method::SetDifficulty,
            ServerToClient::SetExtranonce(_) => Method::SetExtranonce,
            ServerToClient::SetVersionMask(_) => Method::SetVersionMask,
        }
    }

    /// Types `response` as the reply to a `method` request. A response does not name the method
    /// it answers, so the caller supplies it.
    ///
    /// Returns `None` when the response has no ID, or `method` is not one that has a result.
    pub fn from_response(
        response: &json_rpc::Response,
        method: &Method,
    ) -> Result<Option<Self>, Error> {
        let Some(id) = response.id.clone() else {
            return Ok(None);
        };
        let result = &response.result;
        Ok(Some(match method {
            Method::Configure => ServerToClient::Configure(Response::decode(id, method, result)?),
            Method::Subscribe => ServerToClient::Subscribe(Response::decode(id, method, result)?),
            Method::Authorize => ServerToClient::Authorize(Response::decode(id, method, result)?),
            Method::Submit => ServerToClient::Submit(Response::decode(id, method, result)?),
            Method::ExtranonceSubscribe => {
                ServerToClient::ExtranonceSubscribe(Response::decode(id, method, result)?)
            }
            _ => return Ok(None),
        }))
    }
}

/// Types a notification. A reply is typed with [`ServerToClient::from_response`].
///
/// Any request ID is ignored, so a server that attaches one to a notification is still
/// understood.
impl TryFrom<json_rpc::Request> for ServerToClient {
    type Error = DecodeError;

    fn try_from(mut request: json_rpc::Request) -> Result<Self, DecodeError> {
        let error =
            |kind| DecodeError::about(request.id.clone(), Some(request.method.clone()), kind);
        let message = match &request.method {
            Method::Notify => Notify::from_params(&mut request.params).map(ServerToClient::Notify),
            Method::SetDifficulty => {
                SetDifficulty::from_params(&request.params).map(ServerToClient::SetDifficulty)
            }
            Method::SetExtranonce => {
                SetExtranonce::from_params(&request.params).map(ServerToClient::SetExtranonce)
            }
            Method::SetVersionMask => {
                SetVersionMask::from_params(&request.params).map(ServerToClient::SetVersionMask)
            }
            _ => return Err(error(DecodeErrorKind::UnknownMethod)),
        };
        message.map_err(|e| error(e.into()))
    }
}

impl From<ServerToClient> for Message {
    fn from(message: ServerToClient) -> Self {
        match message {
            ServerToClient::Configure(response) => response.into(),
            ServerToClient::Subscribe(response) => response.into(),
            ServerToClient::Authorize(response)
            | ServerToClient::Submit(response)
            | ServerToClient::ExtranonceSubscribe(response) => response.into(),
            ServerToClient::Notify(notify) => notify.into(),
            ServerToClient::SetDifficulty(set) => set.into(),
            ServerToClient::SetExtranonce(set) => set.into(),
            ServerToClient::SetVersionMask(set) => set.into(),
        }
    }
}
