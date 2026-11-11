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
//! [`IsServer`] and [`IsClient`] route decoded messages to one handler per message. Their
//! handlers are required, so an implementor cannot miss a message.
//!
//! References:
//! [https://docs.google.com/document/d/17zHy1SUlhgtCMbypO8cHgpWH73V5iUQKk_0rWvMqSNs/edit?hl=en_US#]
//! [https://braiins.com/stratum-v1/docs]
//! [https://en.bitcoin.it/wiki/Stratum_mining_protocol]
//! [https://en.bitcoin.it/wiki/BIP_0310]
//! [https://docs.google.com/spreadsheets/d/1z8a3S9gFkS8NGhBCxOMUDqs7h9SQltz8-VX3KPHk7Jw/edit#gid=0]

mod client;
pub mod error;
pub mod json_rpc;
mod server;
pub mod utils;

pub use client::{
    Authorize, ClientRequest, Configure, ConfigureExtension, InfoParams, Submit, Subscribe,
    SuggestDifficulty, VersionRolling,
};
use error::Error;
pub use json_rpc::{Id, Message, Method, Response, RpcError};
pub use server::{
    ConfigureResult, Notify, ServerNotification, SetDifficulty, SetExtranonce, SetVersionMask,
    SubmitError, SubmitOutcome, SubscribeResult, VersionRollingResult,
};
use std::convert::TryFrom;
use utils::HexU32Be;

/// Routes requests from SV1 clients to one handler per method.
///
/// `client_id` identifies the connection, so one implementor can serve many clients.
pub trait IsServer {
    type Error;

    /// Calls the handler for `request` and returns the reply, carrying the request's ID.
    ///
    /// A submit reaches [`Self::handle_submit`] only when its worker is authorized, its
    /// extranonce2 has the size returned by [`Self::extranonce2_size`], and its version bits
    /// match the negotiated mask, including whether they must be present. Otherwise the reply
    /// rejects it with [`SubmitError::UnauthorizedWorker`] (24) or [`SubmitError::Other`] (20).
    ///
    /// The request is already decoded, so callers can check session ordering or queue it first.
    fn handle_request(
        &mut self,
        client_id: Option<usize>,
        request: ClientRequest,
    ) -> Result<Option<Response>, Self::Error> {
        Ok(Some(match request {
            ClientRequest::Configure(id, configure) => {
                Response::success(id, self.handle_configure(client_id, &configure)?)
            }
            ClientRequest::Subscribe(id, subscribe) => {
                Response::success(id, self.handle_subscribe(client_id, &subscribe)?)
            }
            ClientRequest::Authorize(id, authorize) => {
                Response::success(id, self.handle_authorize(client_id, &authorize)?)
            }
            ClientRequest::Submit(id, submit) => {
                let outcome = match check_submit(self, client_id, &submit)? {
                    Some(rejection) => SubmitOutcome::Rejected(rejection),
                    None => self.handle_submit(client_id, &submit)?,
                };
                outcome.into_response(id)
            }
            ClientRequest::ExtranonceSubscribe(id) => {
                self.handle_extranonce_subscribe(client_id)?;
                Response::success(id, true)
            }
            ClientRequest::SuggestDifficulty(_, suggest) => {
                self.handle_suggest_difficulty(client_id, &suggest)?;
                return Ok(None);
            }
        }))
    }

    fn handle_configure(
        &mut self,
        client_id: Option<usize>,
        request: &Configure,
    ) -> Result<ConfigureResult, Self::Error>;

    fn handle_subscribe(
        &mut self,
        client_id: Option<usize>,
        request: &Subscribe,
    ) -> Result<SubscribeResult, Self::Error>;

    /// Records the worker when it returns `true`. A worker may be authorized at any time during
    /// the session, so one connection can carry many workers.
    fn handle_authorize(
        &mut self,
        client_id: Option<usize>,
        request: &Authorize,
    ) -> Result<bool, Self::Error>;

    /// Called only after the checks described in [`Self::handle_request`] pass.
    fn handle_submit(
        &mut self,
        client_id: Option<usize>,
        request: &Submit,
    ) -> Result<SubmitOutcome, Self::Error>;

    /// The client supports `mining.set_extranonce`.
    fn handle_extranonce_subscribe(&mut self, client_id: Option<usize>) -> Result<(), Self::Error>;

    /// No reply is sent. Servers are not required to honour the suggestion.
    fn handle_suggest_difficulty(
        &mut self,
        client_id: Option<usize>,
        request: &SuggestDifficulty,
    ) -> Result<(), Self::Error>;

    fn is_authorized(&self, client_id: Option<usize>, name: &str) -> Result<bool, Self::Error>;

    fn extranonce2_size(&self, client_id: Option<usize>) -> Result<usize, Self::Error>;

    /// The negotiated mask, or `None` when version rolling is not in use.
    fn version_rolling_mask(
        &self,
        client_id: Option<usize>,
    ) -> Result<Option<HexU32Be>, Self::Error>;
}

/// The session checks every submit passes before [`IsServer::handle_submit`].
fn check_submit<S: IsServer + ?Sized>(
    server: &S,
    client_id: Option<usize>,
    submit: &Submit,
) -> Result<Option<SubmitError>, S::Error> {
    let has_valid_version_bits = match (
        &submit.version_bits,
        server.version_rolling_mask(client_id)?,
    ) {
        (Some(bits), Some(mask)) => mask.check_mask(bits),
        (Some(_), None) => false,
        (None, mask) => mask.is_none(),
    };
    if !server.is_authorized(client_id, &submit.worker_name)? {
        return Ok(Some(SubmitError::UnauthorizedWorker));
    }
    if server.extranonce2_size(client_id)? != submit.extranonce2.len() || !has_valid_version_bits {
        return Ok(Some(SubmitError::Other));
    }
    Ok(None)
}

/// Routes messages from an SV1 server to one handler per notification and result.
///
/// A response does not name the method it answers, so the router asks
/// [`Self::request_method`] for the method of the outstanding request with that ID.
pub trait IsClient {
    type Error: From<Error>;

    /// Calls the handler for `message`.
    ///
    /// A malformed notification, or a result that does not match its request's method, is
    /// returned as an error and no handler runs.
    fn handle_message(
        &mut self,
        server_id: Option<usize>,
        message: Message,
    ) -> Result<(), Self::Error> {
        match message {
            Message::Request(request) => {
                if let Method::Other(_) = request.method {
                    return self.handle_unknown_request(server_id, request);
                }
                match ServerNotification::try_from(request).map_err(Error::from)? {
                    ServerNotification::Notify(m) => self.handle_notify(server_id, m),
                    ServerNotification::SetDifficulty(m) => {
                        self.handle_set_difficulty(server_id, m)
                    }
                    ServerNotification::SetExtranonce(m) => {
                        self.handle_set_extranonce(server_id, m)
                    }
                    ServerNotification::SetVersionMask(m) => {
                        self.handle_set_version_mask(server_id, m)
                    }
                }
            }
            Message::Response(response) => {
                let method = match &response.id {
                    Some(id) => self.request_method(server_id, id)?,
                    None => None,
                };
                let (Some(id), Some(method)) = (response.id.clone(), method) else {
                    return self.handle_unknown_response(server_id, response);
                };
                if !matches!(
                    method,
                    Method::Configure
                        | Method::Subscribe
                        | Method::Authorize
                        | Method::Submit
                        | Method::ExtranonceSubscribe
                ) {
                    return self.handle_unknown_response(server_id, response);
                }
                if let Err(error) = &response.result {
                    return self.handle_error_response(server_id, &id, method, error.clone());
                }
                match method {
                    Method::Configure => self.handle_configure_result(
                        server_id,
                        &id,
                        response.result_as().map_err(Error::from)?,
                    ),
                    Method::Subscribe => self.handle_subscribe_result(
                        server_id,
                        &id,
                        response.result_as().map_err(Error::from)?,
                    ),
                    Method::Authorize => self.handle_authorize_result(
                        server_id,
                        &id,
                        response.result_as().map_err(Error::from)?,
                    ),
                    Method::Submit => self.handle_submit_result(
                        server_id,
                        &id,
                        response.result_as().map_err(Error::from)?,
                    ),
                    _ => self.handle_extranonce_subscribe_result(
                        server_id,
                        &id,
                        response.result_as().map_err(Error::from)?,
                    ),
                }
            }
        }
    }

    /// The method of the outstanding request with this ID, or `None` if there is none.
    ///
    /// Clients record each request's method when sending it, and drop the entry in the result or
    /// error handler for that ID.
    fn request_method(
        &self,
        server_id: Option<usize>,
        id: &Id,
    ) -> Result<Option<Method>, Self::Error>;

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

    /// The server rejected the outstanding request with this ID.
    fn handle_error_response(
        &mut self,
        server_id: Option<usize>,
        id: &Id,
        method: Method,
        error: RpcError,
    ) -> Result<(), Self::Error>;

    /// A response with a null ID, or an ID that matches no outstanding request.
    fn handle_unknown_response(
        &mut self,
        server_id: Option<usize>,
        response: Response,
    ) -> Result<(), Self::Error>;

    /// A request whose method this crate does not model, such as `client.reconnect`.
    fn handle_unknown_request(
        &mut self,
        server_id: Option<usize>,
        request: json_rpc::Request,
    ) -> Result<(), Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::{Extranonce, VERSION_ROLLING_MASK};
    use serde_json::json;
    use std::collections::HashMap;

    fn request(line: &str) -> ClientRequest {
        let Message::Request(request) = Message::decode(line).unwrap() else {
            panic!("{line}: not a request");
        };
        ClientRequest::try_from(request).unwrap()
    }

    fn reply(server: &mut TestServer, line: &str) -> Option<serde_json::Value> {
        server
            .handle_request(Some(1), request(line))
            .unwrap()
            .map(|response| serde_json::to_value(Message::Response(response)).unwrap())
    }

    #[derive(Default)]
    struct TestServer {
        workers: Vec<String>,
        mask: Option<HexU32Be>,
        submits: usize,
        extranonce_subscribed: bool,
        suggested: Option<f64>,
    }

    impl IsServer for TestServer {
        type Error = Error;

        fn handle_configure(
            &mut self,
            _: Option<usize>,
            request: &Configure,
        ) -> Result<ConfigureResult, Error> {
            self.mask = request
                .version_rolling_mask()
                .map(|mask| HexU32Be(mask.0 & VERSION_ROLLING_MASK));
            Ok(ConfigureResult {
                version_rolling: self
                    .mask
                    .clone()
                    .map(|mask| VersionRollingResult::new(mask, 0)),
                minimum_difficulty: None,
            })
        }

        fn handle_subscribe(
            &mut self,
            _: Option<usize>,
            _: &Subscribe,
        ) -> Result<SubscribeResult, Error> {
            Ok(SubscribeResult {
                subscriptions: vec![("mining.notify".to_string(), "1".to_string())],
                extranonce1: Extranonce::try_from(vec![8, 0, 0, 2]).unwrap(),
                extranonce2_size: 4,
            })
        }

        fn handle_authorize(
            &mut self,
            _: Option<usize>,
            request: &Authorize,
        ) -> Result<bool, Error> {
            let authorized = request.password == "x";
            if authorized {
                self.workers.push(request.worker_name.clone());
            }
            Ok(authorized)
        }

        fn handle_submit(&mut self, _: Option<usize>, _: &Submit) -> Result<SubmitOutcome, Error> {
            self.submits += 1;
            Ok(SubmitOutcome::Rejected(SubmitError::LowDifficultyShare))
        }

        fn handle_extranonce_subscribe(&mut self, _: Option<usize>) -> Result<(), Error> {
            self.extranonce_subscribed = true;
            Ok(())
        }

        fn handle_suggest_difficulty(
            &mut self,
            _: Option<usize>,
            request: &SuggestDifficulty,
        ) -> Result<(), Error> {
            self.suggested = request.difficulty;
            Ok(())
        }

        fn is_authorized(&self, _: Option<usize>, name: &str) -> Result<bool, Error> {
            Ok(self.workers.iter().any(|worker| worker == name))
        }

        fn extranonce2_size(&self, _: Option<usize>) -> Result<usize, Error> {
            Ok(4)
        }

        fn version_rolling_mask(&self, _: Option<usize>) -> Result<Option<HexU32Be>, Error> {
            Ok(self.mask.clone())
        }
    }

    const SUBMIT: &str =
        r#"{"id":"s","method":"mining.submit","params":["w","bf","00000000","647025b5","1d2e3f40""#;

    fn submit(server: &mut TestServer, rest: &str) -> serde_json::Value {
        reply(server, &format!("{SUBMIT}{rest}]}}")).unwrap()
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
                "version-rolling.min-bit-count": "00000000",
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
    fn submit_checks_run_before_the_handler() {
        let mut server = TestServer::default();
        let rejected = |code: i32, message: &str| json!({"id": "s", "result": null, "error": [code, message, null]});

        assert_eq!(submit(&mut server, ""), rejected(24, "Unauthorized worker"));
        server.workers.push("w".to_string());
        assert_eq!(
            submit(&mut server, r#","00002000""#),
            rejected(20, "Other/Unknown")
        );
        assert_eq!(
            reply(
                &mut server,
                r#"{"id":"s","method":"mining.submit","params":["w","bf","00","647025b5","1d2e3f40"]}"#
            ),
            Some(rejected(20, "Other/Unknown"))
        );

        server.mask = Some(HexU32Be(0x1fffe000));
        assert_eq!(submit(&mut server, ""), rejected(20, "Other/Unknown"));
        assert_eq!(
            submit(&mut server, r#","e0000000""#),
            rejected(20, "Other/Unknown")
        );
        assert_eq!(server.submits, 0);

        assert_eq!(
            submit(&mut server, r#","00002000""#),
            rejected(23, "Low difficulty share")
        );
        server.mask = None;
        assert_eq!(
            submit(&mut server, ""),
            rejected(23, "Low difficulty share")
        );
        assert_eq!(server.submits, 2);
    }

    #[test]
    fn unauthorized_worker_takes_precedence() {
        let mut server = TestServer {
            mask: Some(HexU32Be(0x1fffe000)),
            ..TestServer::default()
        };
        assert_eq!(submit(&mut server, r#","e0000000""#)["error"][0], json!(24));
    }

    #[derive(Default)]
    struct TestClient {
        pending: HashMap<Id, Method>,
        events: Vec<String>,
    }

    impl TestClient {
        fn send(&mut self, id: Id, method: Method) {
            self.pending.insert(id, method);
        }

        fn complete(&mut self, id: &Id, event: String) -> Result<(), Error> {
            self.pending.remove(id);
            self.events.push(event);
            Ok(())
        }
    }

    impl IsClient for TestClient {
        type Error = Error;

        fn request_method(&self, _: Option<usize>, id: &Id) -> Result<Option<Method>, Error> {
            Ok(self.pending.get(id).cloned())
        }

        fn handle_notify(&mut self, _: Option<usize>, notify: Notify) -> Result<(), Error> {
            self.events.push(format!("notify {}", notify.job_id));
            Ok(())
        }

        fn handle_set_difficulty(
            &mut self,
            _: Option<usize>,
            set: SetDifficulty,
        ) -> Result<(), Error> {
            self.events.push(format!("difficulty {}", set.difficulty));
            Ok(())
        }

        fn handle_set_extranonce(
            &mut self,
            _: Option<usize>,
            set: SetExtranonce,
        ) -> Result<(), Error> {
            self.events
                .push(format!("extranonce {}", set.extranonce2_size));
            Ok(())
        }

        fn handle_set_version_mask(
            &mut self,
            _: Option<usize>,
            set: SetVersionMask,
        ) -> Result<(), Error> {
            self.events.push(format!("mask {}", set.mask));
            Ok(())
        }

        fn handle_configure_result(
            &mut self,
            _: Option<usize>,
            id: &Id,
            result: ConfigureResult,
        ) -> Result<(), Error> {
            self.complete(
                id,
                format!("configure {}", result.version_rolling.is_some()),
            )
        }

        fn handle_subscribe_result(
            &mut self,
            _: Option<usize>,
            id: &Id,
            result: SubscribeResult,
        ) -> Result<(), Error> {
            self.complete(id, format!("subscribe {}", result.extranonce2_size))
        }

        fn handle_authorize_result(
            &mut self,
            _: Option<usize>,
            id: &Id,
            authorized: bool,
        ) -> Result<(), Error> {
            self.complete(id, format!("authorize {id} {authorized}"))
        }

        fn handle_submit_result(
            &mut self,
            _: Option<usize>,
            id: &Id,
            accepted: bool,
        ) -> Result<(), Error> {
            self.complete(id, format!("submit {id} {accepted}"))
        }

        fn handle_extranonce_subscribe_result(
            &mut self,
            _: Option<usize>,
            id: &Id,
            acknowledged: bool,
        ) -> Result<(), Error> {
            self.complete(id, format!("extranonce_subscribe {acknowledged}"))
        }

        fn handle_error_response(
            &mut self,
            _: Option<usize>,
            id: &Id,
            method: Method,
            error: RpcError,
        ) -> Result<(), Error> {
            self.complete(id, format!("error {method} {}", error.code))
        }

        fn handle_unknown_response(
            &mut self,
            _: Option<usize>,
            response: Response,
        ) -> Result<(), Error> {
            let id = response.id.map_or("null".to_string(), |id| id.to_string());
            self.events.push(format!("unknown response {id}"));
            Ok(())
        }

        fn handle_unknown_request(
            &mut self,
            _: Option<usize>,
            request: json_rpc::Request,
        ) -> Result<(), Error> {
            self.events
                .push(format!("unknown request {}", request.method));
            Ok(())
        }
    }

    fn receive(client: &mut TestClient, line: &str) -> Result<(), Error> {
        client.handle_message(None, Message::decode(line).unwrap())
    }

    #[test]
    fn responses_are_routed_by_their_request_method() {
        let mut client = TestClient::default();
        client.send(Id::from(1u64), Method::Configure);
        client.send(Id::from(2u64), Method::Subscribe);
        client.send(Id::from(3u64), Method::Authorize);
        client.send(Id::from(4u64), Method::Submit);
        client.send(Id::from("4"), Method::Submit);
        client.send(Id::from(5u64), Method::ExtranonceSubscribe);

        for line in [
            r#"{"id":"4","result":false,"error":null}"#,
            r#"{"id":4,"result":true,"error":null}"#,
            r#"{"id":3,"result":true,"error":null}"#,
            r#"{"id":5,"result":true,"error":null}"#,
            r#"{"id":2,"result":[[],"08000002",4],"error":null}"#,
            r#"{"id":1,"result":{},"error":null}"#,
            r#"{"id":4,"result":true,"error":null}"#,
            r#"{"id":null,"result":null,"error":[20,"Other/Unknown",null]}"#,
        ] {
            receive(&mut client, line).unwrap();
        }
        assert_eq!(
            client.events,
            [
                "submit \"4\" false",
                "submit 4 true",
                "authorize 3 true",
                "extranonce_subscribe true",
                "subscribe 4",
                "configure false",
                "unknown response 4",
                "unknown response null",
            ]
        );
        assert!(client.pending.is_empty());
    }

    #[test]
    fn error_responses_complete_their_request() {
        let mut client = TestClient::default();
        client.send(Id::from(4u64), Method::Submit);
        receive(
            &mut client,
            r#"{"id":4,"result":null,"error":[23,"Low difficulty share",null]}"#,
        )
        .unwrap();
        assert_eq!(client.events, ["error mining.submit 23"]);
        assert!(client.pending.is_empty());
    }

    #[test]
    fn mismatched_results_are_errors() {
        let mut client = TestClient::default();
        client.send(Id::from(2u64), Method::Subscribe);
        assert!(matches!(
            receive(&mut client, r#"{"id":2,"result":true,"error":null}"#),
            Err(Error::Response(_))
        ));
        assert!(client.events.is_empty());
    }

    #[test]
    fn notifications_are_routed() {
        let mut client = TestClient::default();
        for line in [
            r#"{"id":null,"method":"mining.set_difficulty","params":[2]}"#,
            r#"{"id":null,"method":"mining.set_extranonce","params":["08000002",4]}"#,
            r#"{"id":null,"method":"mining.set_version_mask","params":["1fffe000"]}"#,
            r#"{"id":null,"method":"mining.notify","params":["bf","000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f","01","02",[],"20000000","1705ae3a","647025b5",true]}"#,
            r#"{"id":9,"method":"client.reconnect","params":[]}"#,
        ] {
            receive(&mut client, line).unwrap();
        }
        assert_eq!(
            client.events,
            [
                "difficulty 2",
                "extranonce 4",
                "mask 1fffe000",
                "notify bf",
                "unknown request \"client.reconnect\"",
            ]
        );
    }

    #[test]
    fn client_requests_are_not_notifications() {
        let mut client = TestClient::default();
        assert!(matches!(
            receive(
                &mut client,
                r#"{"id":1,"method":"mining.submit","params":[]}"#
            ),
            Err(Error::Decode(_))
        ));
    }
}
