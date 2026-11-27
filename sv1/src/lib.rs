//! Stratum V1 application protocol:
//!
//! Messages are newline-delimited JSON objects of two kinds: **requests** and **responses**.
//! A request is either a **notification**, which never gets a reply, or a **standard request**,
//! which does. A typical example of a notification is the broadcasting of a new block.
//!
//! Every request contains three parts:
//! * message ID: integer or string, or null for a notification
//! * remote method: unicode string
//! * parameters: list of parameters
//!
//! ## Standard requests
//! Message ID must be an unique identifier of request during current transport session. It may be
//! integer or some unique string, like UUID. ID must be unique only from one side (it means, both
//! server and clients can initiate request with id “1”). Client or server can choose string/UUID
//! identifier for example in the case when standard “atomic” counter isn’t available.
//!
//! ## Responses
//! Every response contains the following parts
//! * message ID: same ID as in request, for pairing request-response together
//! * result: any json-encoded result object (number, string, list, array, …)
//! * error: null or a three-element list (error code, error message, additional data). The third
//!   element is null when no additional data is provided. For compatibility, deserialization also
//!   accepts two-element lists with absent data and object-form errors.
//!
//! [`ClientToServer`] and [`ServerToClient`] are the typed messages of each direction.
//! [`IsServer`] and [`IsClient`], with their `Async` variants, route them to one handler per
//! message. Their handlers are required, so an implementor cannot miss a message.
//!
//! References:
//! * <https://docs.google.com/document/d/17zHy1SUlhgtCMbypO8cHgpWH73V5iUQKk_0rWvMqSNs/edit?hl=en_US#>
//! * <https://braiins.com/stratum-v1/docs>
//! * <https://en.bitcoin.it/wiki/Stratum_mining_protocol>
//! * <https://en.bitcoin.it/wiki/BIP_0310>
//! * <https://docs.google.com/spreadsheets/d/1z8a3S9gFkS8NGhBCxOMUDqs7h9SQltz8-VX3KPHk7Jw/edit#gid=0>

pub mod error;
pub mod json_rpc;
mod messages;
pub mod utils;

pub use json_rpc::{Id, Message, Method, RpcError};
pub use messages::{
    Authorize, ClientToServer, Configure, ConfigureExtension, ConfigureResult, ExtranonceSubscribe,
    InfoParams, Notify, Request, Response, ServerToClient, SetDifficulty, SetExtranonce,
    SetVersionMask, Submit, SubmitError, SubmitOutcome, Subscribe, SubscribeResult,
    SuggestDifficulty, VersionRolling, VersionRollingResult,
};

/// Routes requests from SV1 clients to one handler per method.
///
/// The router keeps no session state and checks none: a handler acts on its request and sends
/// the reply, which must carry the request's `id`. The reply is [`Response::success`] with the
/// result named on the handler, or [`SubmitOutcome::into_response`] for a submit.
///
/// `client_id` identifies the connection, so one implementor can serve many clients.
pub trait IsServer {
    type Error;

    /// Calls the handler for `request`.
    ///
    /// The request is already decoded, so callers can check session ordering or queue it first.
    fn handle_request(
        &mut self,
        client_id: Option<usize>,
        request: ClientToServer,
    ) -> Result<(), Self::Error> {
        match request {
            ClientToServer::Configure(Request { id, params }) => {
                self.handle_configure(client_id, id, params)
            }
            ClientToServer::Subscribe(Request { id, params }) => {
                self.handle_subscribe(client_id, id, params)
            }
            ClientToServer::Authorize(Request { id, params }) => {
                self.handle_authorize(client_id, id, params)
            }
            ClientToServer::Submit(Request { id, params }) => {
                self.handle_submit(client_id, id, params)
            }
            ClientToServer::ExtranonceSubscribe(Request { id, .. }) => {
                self.handle_extranonce_subscribe(client_id, id)
            }
            ClientToServer::SuggestDifficulty(Request { id, params }) => {
                self.handle_suggest_difficulty(client_id, id, params)
            }
        }
    }

    /// Replies with a [`ConfigureResult`].
    fn handle_configure(
        &mut self,
        client_id: Option<usize>,
        id: Id,
        request: Configure,
    ) -> Result<(), Self::Error>;

    /// Replies with a [`SubscribeResult`].
    fn handle_subscribe(
        &mut self,
        client_id: Option<usize>,
        id: Id,
        request: Subscribe,
    ) -> Result<(), Self::Error>;

    /// Replies with whether the worker is authorized. A worker may be authorized at any time
    /// during the session, so one connection can carry many workers.
    fn handle_authorize(
        &mut self,
        client_id: Option<usize>,
        id: Id,
        request: Authorize,
    ) -> Result<(), Self::Error>;

    /// Replies with a [`SubmitOutcome`].
    ///
    /// Every submit reaches this handler, so it is the one that rejects a submit from an
    /// unauthorized worker, with an extranonce2 of the wrong size, or with version bits the
    /// negotiated mask does not allow.
    fn handle_submit(
        &mut self,
        client_id: Option<usize>,
        id: Id,
        request: Submit,
    ) -> Result<(), Self::Error>;

    /// The client supports `mining.set_extranonce`. Replies with `true`.
    fn handle_extranonce_subscribe(
        &mut self,
        client_id: Option<usize>,
        id: Id,
    ) -> Result<(), Self::Error>;

    /// Servers are not required to honour the suggestion, nor to reply.
    fn handle_suggest_difficulty(
        &mut self,
        client_id: Option<usize>,
        id: Id,
        request: SuggestDifficulty,
    ) -> Result<(), Self::Error>;
}

/// [`IsServer`] for handlers that wait, such as on a channel to the client.
#[trait_variant::make(Send)]
pub trait IsServerAsync {
    type Error;

    /// Calls the handler for `request`.
    ///
    /// The request is already decoded, so callers can check session ordering or queue it first.
    async fn handle_request(
        &mut self,
        client_id: Option<usize>,
        request: ClientToServer,
    ) -> Result<(), Self::Error> {
        async move {
            match request {
                ClientToServer::Configure(Request { id, params }) => {
                    self.handle_configure(client_id, id, params).await
                }
                ClientToServer::Subscribe(Request { id, params }) => {
                    self.handle_subscribe(client_id, id, params).await
                }
                ClientToServer::Authorize(Request { id, params }) => {
                    self.handle_authorize(client_id, id, params).await
                }
                ClientToServer::Submit(Request { id, params }) => {
                    self.handle_submit(client_id, id, params).await
                }
                ClientToServer::ExtranonceSubscribe(Request { id, .. }) => {
                    self.handle_extranonce_subscribe(client_id, id).await
                }
                ClientToServer::SuggestDifficulty(Request { id, params }) => {
                    self.handle_suggest_difficulty(client_id, id, params).await
                }
            }
        }
    }

    /// Replies with a [`ConfigureResult`].
    async fn handle_configure(
        &mut self,
        client_id: Option<usize>,
        id: Id,
        request: Configure,
    ) -> Result<(), Self::Error>;

    /// Replies with a [`SubscribeResult`].
    async fn handle_subscribe(
        &mut self,
        client_id: Option<usize>,
        id: Id,
        request: Subscribe,
    ) -> Result<(), Self::Error>;

    /// Replies with whether the worker is authorized. A worker may be authorized at any time
    /// during the session, so one connection can carry many workers.
    async fn handle_authorize(
        &mut self,
        client_id: Option<usize>,
        id: Id,
        request: Authorize,
    ) -> Result<(), Self::Error>;

    /// Replies with a [`SubmitOutcome`].
    ///
    /// Every submit reaches this handler, so it is the one that rejects a submit from an
    /// unauthorized worker, with an extranonce2 of the wrong size, or with version bits the
    /// negotiated mask does not allow.
    async fn handle_submit(
        &mut self,
        client_id: Option<usize>,
        id: Id,
        request: Submit,
    ) -> Result<(), Self::Error>;

    /// The client supports `mining.set_extranonce`. Replies with `true`.
    async fn handle_extranonce_subscribe(
        &mut self,
        client_id: Option<usize>,
        id: Id,
    ) -> Result<(), Self::Error>;

    /// Servers are not required to honour the suggestion, nor to reply.
    async fn handle_suggest_difficulty(
        &mut self,
        client_id: Option<usize>,
        id: Id,
        request: SuggestDifficulty,
    ) -> Result<(), Self::Error>;
}

/// Routes messages from an SV1 server to one handler per notification and result.
///
/// The router keeps no session state and checks none. The caller types each message: a
/// notification with `ServerToClient::try_from`, and a reply with
/// [`ServerToClient::from_response`], naming the method of the request that reply answers.
pub trait IsClient {
    type Error;

    /// Calls the handler for `message`.
    fn handle_message(
        &mut self,
        server_id: Option<usize>,
        message: ServerToClient,
    ) -> Result<(), Self::Error> {
        let method = message.method();
        match message {
            ServerToClient::Notify(notify) => self.handle_notify(server_id, notify),
            ServerToClient::SetDifficulty(set) => self.handle_set_difficulty(server_id, set),
            ServerToClient::SetExtranonce(set) => self.handle_set_extranonce(server_id, set),
            ServerToClient::SetVersionMask(set) => self.handle_set_version_mask(server_id, set),
            ServerToClient::Configure(Response {
                id,
                result: Ok(result),
            }) => self.handle_configure_result(server_id, &id, result),
            ServerToClient::Subscribe(Response {
                id,
                result: Ok(result),
            }) => self.handle_subscribe_result(server_id, &id, result),
            ServerToClient::Authorize(Response {
                id,
                result: Ok(authorized),
            }) => self.handle_authorize_result(server_id, &id, authorized),
            ServerToClient::Submit(Response {
                id,
                result: Ok(accepted),
            }) => self.handle_submit_result(server_id, &id, accepted),
            ServerToClient::ExtranonceSubscribe(Response {
                id,
                result: Ok(acknowledged),
            }) => self.handle_extranonce_subscribe_result(server_id, &id, acknowledged),
            ServerToClient::Configure(Response {
                id,
                result: Err(error),
            })
            | ServerToClient::Subscribe(Response {
                id,
                result: Err(error),
            })
            | ServerToClient::Authorize(Response {
                id,
                result: Err(error),
            })
            | ServerToClient::Submit(Response {
                id,
                result: Err(error),
            })
            | ServerToClient::ExtranonceSubscribe(Response {
                id,
                result: Err(error),
            }) => self.handle_error_response(server_id, &id, method, error),
        }
    }

    fn handle_notify(
        &mut self,
        server_id: Option<usize>,
        notify: Notify,
    ) -> Result<(), Self::Error>;

    fn handle_set_difficulty(
        &mut self,
        server_id: Option<usize>,
        set: SetDifficulty,
    ) -> Result<(), Self::Error>;

    fn handle_set_extranonce(
        &mut self,
        server_id: Option<usize>,
        set: SetExtranonce,
    ) -> Result<(), Self::Error>;

    fn handle_set_version_mask(
        &mut self,
        server_id: Option<usize>,
        set: SetVersionMask,
    ) -> Result<(), Self::Error>;

    fn handle_configure_result(
        &mut self,
        server_id: Option<usize>,
        id: &Id,
        result: ConfigureResult,
    ) -> Result<(), Self::Error>;

    fn handle_subscribe_result(
        &mut self,
        server_id: Option<usize>,
        id: &Id,
        result: SubscribeResult,
    ) -> Result<(), Self::Error>;

    fn handle_authorize_result(
        &mut self,
        server_id: Option<usize>,
        id: &Id,
        authorized: bool,
    ) -> Result<(), Self::Error>;

    fn handle_submit_result(
        &mut self,
        server_id: Option<usize>,
        id: &Id,
        accepted: bool,
    ) -> Result<(), Self::Error>;

    fn handle_extranonce_subscribe_result(
        &mut self,
        server_id: Option<usize>,
        id: &Id,
        acknowledged: bool,
    ) -> Result<(), Self::Error>;

    /// The server rejected the `method` request with this ID.
    fn handle_error_response(
        &mut self,
        server_id: Option<usize>,
        id: &Id,
        method: Method,
        error: RpcError,
    ) -> Result<(), Self::Error>;
}

/// [`IsClient`] for handlers that wait.
#[trait_variant::make(Send)]
pub trait IsClientAsync {
    type Error;

    /// Calls the handler for `message`.
    async fn handle_message(
        &mut self,
        server_id: Option<usize>,
        message: ServerToClient,
    ) -> Result<(), Self::Error> {
        async move {
            let method = message.method();
            match message {
                ServerToClient::Notify(notify) => self.handle_notify(server_id, notify).await,
                ServerToClient::SetDifficulty(set) => {
                    self.handle_set_difficulty(server_id, set).await
                }
                ServerToClient::SetExtranonce(set) => {
                    self.handle_set_extranonce(server_id, set).await
                }
                ServerToClient::SetVersionMask(set) => {
                    self.handle_set_version_mask(server_id, set).await
                }
                ServerToClient::Configure(Response {
                    id,
                    result: Ok(result),
                }) => self.handle_configure_result(server_id, &id, result).await,
                ServerToClient::Subscribe(Response {
                    id,
                    result: Ok(result),
                }) => self.handle_subscribe_result(server_id, &id, result).await,
                ServerToClient::Authorize(Response {
                    id,
                    result: Ok(authorized),
                }) => {
                    self.handle_authorize_result(server_id, &id, authorized)
                        .await
                }
                ServerToClient::Submit(Response {
                    id,
                    result: Ok(accepted),
                }) => self.handle_submit_result(server_id, &id, accepted).await,
                ServerToClient::ExtranonceSubscribe(Response {
                    id,
                    result: Ok(acknowledged),
                }) => {
                    self.handle_extranonce_subscribe_result(server_id, &id, acknowledged)
                        .await
                }
                ServerToClient::Configure(Response {
                    id,
                    result: Err(error),
                })
                | ServerToClient::Subscribe(Response {
                    id,
                    result: Err(error),
                })
                | ServerToClient::Authorize(Response {
                    id,
                    result: Err(error),
                })
                | ServerToClient::Submit(Response {
                    id,
                    result: Err(error),
                })
                | ServerToClient::ExtranonceSubscribe(Response {
                    id,
                    result: Err(error),
                }) => {
                    self.handle_error_response(server_id, &id, method, error)
                        .await
                }
            }
        }
    }

    async fn handle_notify(
        &mut self,
        server_id: Option<usize>,
        notify: Notify,
    ) -> Result<(), Self::Error>;

    async fn handle_set_difficulty(
        &mut self,
        server_id: Option<usize>,
        set: SetDifficulty,
    ) -> Result<(), Self::Error>;

    async fn handle_set_extranonce(
        &mut self,
        server_id: Option<usize>,
        set: SetExtranonce,
    ) -> Result<(), Self::Error>;

    async fn handle_set_version_mask(
        &mut self,
        server_id: Option<usize>,
        set: SetVersionMask,
    ) -> Result<(), Self::Error>;

    async fn handle_configure_result(
        &mut self,
        server_id: Option<usize>,
        id: &Id,
        result: ConfigureResult,
    ) -> Result<(), Self::Error>;

    async fn handle_subscribe_result(
        &mut self,
        server_id: Option<usize>,
        id: &Id,
        result: SubscribeResult,
    ) -> Result<(), Self::Error>;

    async fn handle_authorize_result(
        &mut self,
        server_id: Option<usize>,
        id: &Id,
        authorized: bool,
    ) -> Result<(), Self::Error>;

    async fn handle_submit_result(
        &mut self,
        server_id: Option<usize>,
        id: &Id,
        accepted: bool,
    ) -> Result<(), Self::Error>;

    async fn handle_extranonce_subscribe_result(
        &mut self,
        server_id: Option<usize>,
        id: &Id,
        acknowledged: bool,
    ) -> Result<(), Self::Error>;

    /// The server rejected the `method` request with this ID.
    async fn handle_error_response(
        &mut self,
        server_id: Option<usize>,
        id: &Id,
        method: Method,
        error: RpcError,
    ) -> Result<(), Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        error::Error,
        utils::{Extranonce, HexU32Be},
    };
    use serde_json::json;
    use std::convert::TryFrom;

    fn request(line: &str) -> ClientToServer {
        let Message::Request(request) = Message::decode(line).unwrap() else {
            panic!("{line}: not a request");
        };
        ClientToServer::try_from(request).unwrap()
    }

    fn reply(server: &mut TestServer, line: &str) -> Option<serde_json::Value> {
        server.handle_request(Some(1), request(line)).unwrap();
        let reply = server.replies.pop();
        assert!(server.replies.is_empty());
        reply.map(|reply| serde_json::to_value(reply).unwrap())
    }

    #[derive(Default)]
    struct TestServer {
        submits: usize,
        extranonce_subscribed: bool,
        suggested: Option<f64>,
        replies: Vec<Message>,
    }

    impl IsServer for TestServer {
        type Error = Error;

        fn handle_configure(
            &mut self,
            _: Option<usize>,
            id: Id,
            request: Configure,
        ) -> Result<(), Error> {
            let result = ConfigureResult {
                version_rolling: request
                    .version_rolling_mask()
                    .map(VersionRollingResult::new),
                ..ConfigureResult::default()
            };
            self.replies.push(Response::success(id, result).into());
            Ok(())
        }

        fn handle_subscribe(
            &mut self,
            _: Option<usize>,
            id: Id,
            _: Subscribe,
        ) -> Result<(), Error> {
            let result = SubscribeResult {
                subscriptions: vec![("mining.notify".to_string(), "1".to_string())],
                extranonce1: Extranonce::try_from(vec![8, 0, 0, 2]).unwrap(),
                extranonce2_size: 4,
            };
            self.replies.push(Response::success(id, result).into());
            Ok(())
        }

        fn handle_authorize(
            &mut self,
            _: Option<usize>,
            id: Id,
            request: Authorize,
        ) -> Result<(), Error> {
            self.replies
                .push(Response::success(id, request.password == "x").into());
            Ok(())
        }

        fn handle_submit(&mut self, _: Option<usize>, id: Id, _: Submit) -> Result<(), Error> {
            self.submits += 1;
            let outcome = SubmitOutcome::Rejected(SubmitError::LowDifficultyShare);
            self.replies.push(outcome.into_response(id).into());
            Ok(())
        }

        fn handle_extranonce_subscribe(&mut self, _: Option<usize>, id: Id) -> Result<(), Error> {
            self.extranonce_subscribed = true;
            self.replies.push(Response::success(id, true).into());
            Ok(())
        }

        fn handle_suggest_difficulty(
            &mut self,
            _: Option<usize>,
            _: Id,
            request: SuggestDifficulty,
        ) -> Result<(), Error> {
            self.suggested = request.difficulty;
            Ok(())
        }
    }

    #[test]
    fn replies_carry_the_request_id() {
        let mut server = TestServer::default();
        assert_eq!(
            reply(
                &mut server,
                r#"{"id":"a","method":"mining.authorize","params":["w","x"]}"#
            ),
            Some(json!({"id": "a", "result": true, "error": null}))
        );
        assert_eq!(
            reply(
                &mut server,
                r#"{"id":7,"method":"mining.authorize","params":["w","bad"]}"#
            ),
            Some(json!({"id": 7, "result": false, "error": null}))
        );
        assert_eq!(
            reply(
                &mut server,
                r#"{"id":2,"method":"mining.subscribe","params":[]}"#
            ),
            Some(
                json!({"id": 2, "result": [[["mining.notify", "1"]], "08000002", 4], "error": null})
            )
        );
        assert_eq!(
            reply(
                &mut server,
                r#"{"id":3,"method":"mining.configure","params":[["version-rolling"],{"version-rolling.mask":"ffffffff"}]}"#
            ),
            Some(json!({"id": 3, "result": {
                "version-rolling": true,
                "version-rolling.mask": "1fffffe0",
            }, "error": null}))
        );
        assert_eq!(
            reply(
                &mut server,
                r#"{"id":4,"method":"mining.extranonce.subscribe","params":[]}"#
            ),
            Some(json!({"id": 4, "result": true, "error": null}))
        );
        assert!(server.extranonce_subscribed);
    }

    #[test]
    fn suggest_difficulty_gets_no_reply() {
        let mut server = TestServer::default();
        assert_eq!(
            reply(
                &mut server,
                r#"{"id":5,"method":"mining.suggest_difficulty","params":[512]}"#
            ),
            None
        );
        assert_eq!(server.suggested, Some(512.0));
    }

    #[test]
    fn every_submit_reaches_its_handler() {
        let mut server = TestServer::default();
        assert_eq!(
            reply(
                &mut server,
                r#"{"id":"s","method":"mining.submit","params":["w","bf","00","647025b5","1d2e3f40","e0000000"]}"#
            ),
            Some(json!({"id": "s", "result": null, "error": [23, "Low difficulty share", null]}))
        );
        assert_eq!(server.submits, 1);
    }

    /// Records which handler ran, to check the async router against the sync one.
    #[derive(Default)]
    struct AsyncTestServer {
        events: Vec<String>,
    }

    impl IsServerAsync for AsyncTestServer {
        type Error = Error;

        async fn handle_configure(
            &mut self,
            _: Option<usize>,
            id: Id,
            _: Configure,
        ) -> Result<(), Error> {
            self.events.push(format!("configure {id}"));
            Ok(())
        }

        async fn handle_subscribe(
            &mut self,
            _: Option<usize>,
            id: Id,
            _: Subscribe,
        ) -> Result<(), Error> {
            self.events.push(format!("subscribe {id}"));
            Ok(())
        }

        async fn handle_authorize(
            &mut self,
            _: Option<usize>,
            id: Id,
            _: Authorize,
        ) -> Result<(), Error> {
            self.events.push(format!("authorize {id}"));
            Ok(())
        }

        async fn handle_submit(
            &mut self,
            _: Option<usize>,
            id: Id,
            _: Submit,
        ) -> Result<(), Error> {
            self.events.push(format!("submit {id}"));
            Ok(())
        }

        async fn handle_extranonce_subscribe(
            &mut self,
            _: Option<usize>,
            id: Id,
        ) -> Result<(), Error> {
            self.events.push(format!("extranonce_subscribe {id}"));
            Ok(())
        }

        async fn handle_suggest_difficulty(
            &mut self,
            _: Option<usize>,
            id: Id,
            _: SuggestDifficulty,
        ) -> Result<(), Error> {
            self.events.push(format!("suggest_difficulty {id}"));
            Ok(())
        }
    }

    /// Runs a future that never waits.
    fn run<F: std::future::Future>(future: F) -> F::Output {
        struct NoopWaker;
        impl std::task::Wake for NoopWaker {
            fn wake(self: std::sync::Arc<Self>) {}
        }
        let waker = std::task::Waker::from(std::sync::Arc::new(NoopWaker));
        let mut future = std::pin::pin!(future);
        match future
            .as_mut()
            .poll(&mut std::task::Context::from_waker(&waker))
        {
            std::task::Poll::Ready(output) => output,
            std::task::Poll::Pending => panic!("the future waited"),
        }
    }

    #[test]
    fn async_requests_are_routed() {
        let mut server = AsyncTestServer::default();
        for line in [
            r#"{"id":1,"method":"mining.configure","params":[[]]}"#,
            r#"{"id":2,"method":"mining.subscribe","params":[]}"#,
            r#"{"id":3,"method":"mining.authorize","params":["w","x"]}"#,
            r#"{"id":4,"method":"mining.submit","params":["w","bf","00000000","647025b5","1d2e3f40"]}"#,
            r#"{"id":5,"method":"mining.extranonce.subscribe","params":[]}"#,
            r#"{"id":6,"method":"mining.suggest_difficulty","params":[512]}"#,
        ] {
            run(server.handle_request(Some(1), request(line))).unwrap();
        }
        assert_eq!(
            server.events,
            [
                "configure 1",
                "subscribe 2",
                "authorize 3",
                "submit 4",
                "extranonce_subscribe 5",
                "suggest_difficulty 6",
            ]
        );
    }

    fn client_impl_events() -> Vec<ServerToClient> {
        let notify = match Message::decode(
            r#"{"id":null,"method":"mining.notify","params":["bf","000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f","01","02",[],"20000000","1705ae3a","647025b5",true]}"#,
        )
        .unwrap()
        {
            Message::Request(request) => ServerToClient::try_from(request).unwrap(),
            Message::Response(_) => panic!("not a notification"),
        };
        vec![
            ServerToClient::Configure(Response::success(
                Id::from(1u64),
                ConfigureResult::default(),
            )),
            ServerToClient::Subscribe(Response::success(
                Id::from(2u64),
                SubscribeResult {
                    subscriptions: vec![],
                    extranonce1: Extranonce::try_from(vec![8, 0, 0, 2]).unwrap(),
                    extranonce2_size: 4,
                },
            )),
            ServerToClient::Authorize(Response::success(Id::from(3u64), true)),
            ServerToClient::Submit(Response::success(Id::from(4u64), false)),
            ServerToClient::Submit(Response::success(Id::from("4"), true)),
            ServerToClient::ExtranonceSubscribe(Response::success(Id::from(5u64), true)),
            ServerToClient::Submit(Response::failure(
                Id::from(6u64),
                SubmitError::DuplicateShare.into(),
            )),
            ServerToClient::Authorize(Response::failure(Id::from(7u64), SubmitError::Other.into())),
            notify,
            ServerToClient::SetDifficulty(SetDifficulty::new(2.0).unwrap()),
            ServerToClient::SetExtranonce(SetExtranonce {
                extranonce1: Extranonce::try_from(vec![8, 0, 0, 2]).unwrap(),
                extranonce2_size: 4,
            }),
            ServerToClient::SetVersionMask(SetVersionMask::new(HexU32Be(0x1fffe000))),
        ]
    }

    const CLIENT_EVENTS: [&str; 12] = [
        "configure 1",
        "subscribe 2",
        "authorize 3 true",
        "submit 4 false",
        "submit \"4\" true",
        "extranonce_subscribe 5 true",
        "error 6 mining.submit 22",
        "error 7 mining.authorize 20",
        "notify bf",
        "difficulty 2",
        "extranonce 4",
        "mask 1fffe000",
    ];

    #[derive(Default)]
    struct TestClient {
        events: Vec<String>,
    }

    impl TestClient {
        fn record(&mut self, event: String) -> Result<(), Error> {
            self.events.push(event);
            Ok(())
        }
    }

    impl IsClient for TestClient {
        type Error = Error;

        fn handle_notify(&mut self, _: Option<usize>, notify: Notify) -> Result<(), Error> {
            self.record(format!("notify {}", notify.job_id))
        }

        fn handle_set_difficulty(
            &mut self,
            _: Option<usize>,
            set: SetDifficulty,
        ) -> Result<(), Error> {
            self.record(format!("difficulty {}", set.difficulty()))
        }

        fn handle_set_extranonce(
            &mut self,
            _: Option<usize>,
            set: SetExtranonce,
        ) -> Result<(), Error> {
            self.record(format!("extranonce {}", set.extranonce2_size))
        }

        fn handle_set_version_mask(
            &mut self,
            _: Option<usize>,
            set: SetVersionMask,
        ) -> Result<(), Error> {
            self.record(format!("mask {}", set.mask()))
        }

        fn handle_configure_result(
            &mut self,
            _: Option<usize>,
            id: &Id,
            _: ConfigureResult,
        ) -> Result<(), Error> {
            self.record(format!("configure {id}"))
        }

        fn handle_subscribe_result(
            &mut self,
            _: Option<usize>,
            id: &Id,
            _: SubscribeResult,
        ) -> Result<(), Error> {
            self.record(format!("subscribe {id}"))
        }

        fn handle_authorize_result(
            &mut self,
            _: Option<usize>,
            id: &Id,
            authorized: bool,
        ) -> Result<(), Error> {
            self.record(format!("authorize {id} {authorized}"))
        }

        fn handle_submit_result(
            &mut self,
            _: Option<usize>,
            id: &Id,
            accepted: bool,
        ) -> Result<(), Error> {
            self.record(format!("submit {id} {accepted}"))
        }

        fn handle_extranonce_subscribe_result(
            &mut self,
            _: Option<usize>,
            id: &Id,
            acknowledged: bool,
        ) -> Result<(), Error> {
            self.record(format!("extranonce_subscribe {id} {acknowledged}"))
        }

        fn handle_error_response(
            &mut self,
            _: Option<usize>,
            id: &Id,
            method: Method,
            error: RpcError,
        ) -> Result<(), Error> {
            self.record(format!("error {id} {method} {}", error.code))
        }
    }

    #[test]
    fn server_messages_are_routed() {
        let mut client = TestClient::default();
        for message in client_impl_events() {
            client.handle_message(None, message).unwrap();
        }
        assert_eq!(client.events, CLIENT_EVENTS);
    }

    /// Records which handler ran, to check the async router against the sync one.
    #[derive(Default)]
    struct AsyncTestClient {
        events: Vec<String>,
    }

    impl AsyncTestClient {
        fn record(&mut self, event: String) -> Result<(), Error> {
            self.events.push(event);
            Ok(())
        }
    }

    impl IsClientAsync for AsyncTestClient {
        type Error = Error;

        async fn handle_notify(&mut self, _: Option<usize>, notify: Notify) -> Result<(), Error> {
            self.record(format!("notify {}", notify.job_id))
        }

        async fn handle_set_difficulty(
            &mut self,
            _: Option<usize>,
            set: SetDifficulty,
        ) -> Result<(), Error> {
            self.record(format!("difficulty {}", set.difficulty()))
        }

        async fn handle_set_extranonce(
            &mut self,
            _: Option<usize>,
            set: SetExtranonce,
        ) -> Result<(), Error> {
            self.record(format!("extranonce {}", set.extranonce2_size))
        }

        async fn handle_set_version_mask(
            &mut self,
            _: Option<usize>,
            set: SetVersionMask,
        ) -> Result<(), Error> {
            self.record(format!("mask {}", set.mask()))
        }

        async fn handle_configure_result(
            &mut self,
            _: Option<usize>,
            id: &Id,
            _: ConfigureResult,
        ) -> Result<(), Error> {
            self.record(format!("configure {id}"))
        }

        async fn handle_subscribe_result(
            &mut self,
            _: Option<usize>,
            id: &Id,
            _: SubscribeResult,
        ) -> Result<(), Error> {
            self.record(format!("subscribe {id}"))
        }

        async fn handle_authorize_result(
            &mut self,
            _: Option<usize>,
            id: &Id,
            authorized: bool,
        ) -> Result<(), Error> {
            self.record(format!("authorize {id} {authorized}"))
        }

        async fn handle_submit_result(
            &mut self,
            _: Option<usize>,
            id: &Id,
            accepted: bool,
        ) -> Result<(), Error> {
            self.record(format!("submit {id} {accepted}"))
        }

        async fn handle_extranonce_subscribe_result(
            &mut self,
            _: Option<usize>,
            id: &Id,
            acknowledged: bool,
        ) -> Result<(), Error> {
            self.record(format!("extranonce_subscribe {id} {acknowledged}"))
        }

        async fn handle_error_response(
            &mut self,
            _: Option<usize>,
            id: &Id,
            method: Method,
            error: RpcError,
        ) -> Result<(), Error> {
            self.record(format!("error {id} {method} {}", error.code))
        }
    }

    #[test]
    fn async_server_messages_are_routed() {
        let mut client = AsyncTestClient::default();
        for message in client_impl_events() {
            run(client.handle_message(None, message)).unwrap();
        }
        assert_eq!(client.events, CLIENT_EVENTS);
    }
}
