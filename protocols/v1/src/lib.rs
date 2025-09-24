#![allow(clippy::result_unit_err)]
//! Stratum V1 application protocol:
//!
//! json-rpc has two types of messages: **request** and **response**.
//! A request message can be either a **notification** or a **standard message**.
//! Standard messages expect a response, notifications do not. A typical example of a notification
//! is the broadcasting of a new block.
//!
//! Every RPC request contains three parts:
//! * message ID: integer or string
//! * remote method: unicode string
//! * parameters: list of parameters
//!
//! ## Standard requests
//! Message ID must be an unique identifier of request during current transport session. It may be
//! integer or some unique string, like UUID. ID must be unique only from one side (it means, both
//! server and clients can initiate request with id “1”). Client or server can choose string/UUID
//! identifier for example in the case when standard “atomic” counter isn’t available.
//!
//! ## Notifications
//! Notifications are like Request, but it does not expect any response and message ID is always
//! null:
//! * message ID: null
//! * remote method: unicode string
//! * parameters: list of parameters
//!
//! ## Responses
//! Every response contains the following parts
//! * message ID: same ID as in request, for pairing request-response together
//! * result: any json-encoded result object (number, string, list, array, …)
//! * error: null or list (error code, error message)
//!
//! References:
//! [https://docs.google.com/document/d/17zHy1SUlhgtCMbypO8cHgpWH73V5iUQKk_0rWvMqSNs/edit?hl=en_US#]
//! [https://braiins.com/stratum-v1/docs]
//! [https://en.bitcoin.it/wiki/Stratum_mining_protocol]
//! [https://en.bitcoin.it/wiki/BIP_0310]
//! [https://docs.google.com/spreadsheets/d/1z8a3S9gFkS8NGhBCxOMUDqs7h9SQltz8-VX3KPHk7Jw/edit#gid=0]

pub mod error;
pub mod json_rpc;
pub mod methods;
pub mod utils;

use std::convert::{TryFrom, TryInto};
use tracing::debug;

// use error::Result;
use error::Error;
pub use json_rpc::Message;
pub use methods::{client_to_server, server_to_client, Method, MethodError, ParsingMethodError};
use utils::{Extranonce, HexU32Be};

/// json_rpc Response are not handled because stratum v1 does not have any request from a server to
/// a client
/// TODO: Should update to accommodate miner requesting a difficulty change
///
/// A stratum v1 server represent a single connection with a client
pub trait IsServer<'a> {
    /// handle the received message and return a response if the message is a request or
    /// notification.
    fn handle_message(
        &mut self,
        msg: json_rpc::Message,
    ) -> Result<Option<json_rpc::Response>, Error<'a>>
    where
        Self: std::marker::Sized,
    {
        match msg {
            Message::StandardRequest(_) => {
                // handle valid standard request
                self.handle_request(msg)
            }
            Message::Notification(_) => {
                // handle valid server notification
                self.handle_request(msg)
            }
            _ => {
                // Server shouldn't receive json_rpc responses
                Err(Error::InvalidJsonRpcMessageKind)
            }
        }
    }

    /// Call the right handler according with the called method
    fn handle_request(
        &mut self,
        msg: json_rpc::Message,
    ) -> Result<Option<json_rpc::Response>, Error<'a>>
    where
        Self: std::marker::Sized,
    {
        let request = msg.try_into()?;

        match request {
            // TODO: Handle suggested difficulty
            methods::Client2Server::SuggestDifficulty() => Ok(None),
            methods::Client2Server::Authorize(authorize) => {
                let authorized = self.handle_authorize(&authorize);
                if authorized {
                    self.authorize(&authorize.name);
                }
                Ok(Some(authorize.respond(authorized)))
            }
            methods::Client2Server::Configure(configure) => {
                debug!("{:?}", configure);
                self.set_version_rolling_mask(configure.version_rolling_mask());
                self.set_version_rolling_min_bit(configure.version_rolling_min_bit_count());
                let (version_rolling, min_diff) = self.handle_configure(&configure);
                Ok(Some(configure.respond(version_rolling, min_diff)))
            }
            methods::Client2Server::ExtranonceSubscribe(_) => {
                self.handle_extranonce_subscribe();
                Ok(None)
            }
            methods::Client2Server::Submit(submit) => {
                let has_valid_version_bits = match &submit.version_bits {
                    Some(a) => {
                        if let Some(version_rolling_mask) = self.version_rolling_mask() {
                            version_rolling_mask.check_mask(a)
                        } else {
                            false
                        }
                    }
                    None => self.version_rolling_mask().is_none(),
                };

                let is_valid_submission = self.is_authorized(&submit.user_name)
                    && self.extranonce2_size() == submit.extra_nonce2.len()
                    && has_valid_version_bits;

                if is_valid_submission {
                    let accepted = self.handle_submit(&submit);
                    Ok(Some(submit.respond(accepted)))
                } else {
                    Err(Error::InvalidSubmission)
                }
            }
            methods::Client2Server::Subscribe(subscribe) => {
                let subscriptions = self.handle_subscribe(&subscribe);
                let extra_n1 = self.set_extranonce1(None);
                let extra_n2_size = self.set_extranonce2_size(None);
                Ok(Some(subscribe.respond(
                    subscriptions,
                    extra_n1,
                    extra_n2_size,
                )))
            }
        }
    }

    /// This message (JSON RPC Request) SHOULD be the first message sent by the miner after the
    /// connection with the server is established.
    fn handle_configure(
        &mut self,
        request: &client_to_server::Configure,
    ) -> (Option<server_to_client::VersionRollingParams>, Option<bool>);

    /// On the beginning of the session, client subscribes current connection for receiving mining
    /// jobs.
    ///
    /// The client can specify [mining.notify][a] job_id the client wishes to resume working with
    ///
    /// The result contains three items:
    /// * Subscriptions details: 2-tuple with name of subscribed notification and subscription ID.
    ///   Teoretically it may be used for unsubscribing, but obviously miners won't use it.
    /// * Extranonce1 - Hex-encoded, per-connection unique string which will be used for coinbase
    ///   serialization later. Keep it safe!
    /// * Extranonce2_size - Represents expected length of extranonce2 which will be generated by
    ///   the miner.
    ///
    /// Almost instantly after the subscription server start to send [jobs][a]
    ///
    /// This function return the first item of the result (2 tuple with name of subscibed ...)
    ///
    /// [a]: crate::methods::server_to_client::Notify
    fn handle_subscribe(&self, request: &client_to_server::Subscribe) -> Vec<(String, String)>;

    /// You can authorize as many workers as you wish and at any
    /// time during the session. In this way, you can handle big basement of independent mining rigs
    /// just by one Stratum connection.
    ///
    /// https://bitcoin.stackexchange.com/questions/29416/how-do-pool-servers-handle-multiple-workers-sharing-one-connection-with-stratum
    fn handle_authorize(&self, request: &client_to_server::Authorize) -> bool;

    /// When miner find the job which meets requested difficulty, it can submit share to the server.
    /// Only [Submit](client_to_server::Submit) requests for authorized user names can be submitted.
    fn handle_submit(&self, request: &client_to_server::Submit<'a>) -> bool;

    /// Indicates to the server that the client supports the mining.set_extranonce method.
    fn handle_extranonce_subscribe(&self);

    fn is_authorized(&self, name: &str) -> bool;

    fn authorize(&mut self, name: &str);

    /// Set extranonce1 to extranonce1 if provided. If not create a new one and set it.
    fn set_extranonce1(&mut self, extranonce1: Option<Extranonce<'a>>) -> Extranonce<'a>;

    fn extranonce1(&self) -> Extranonce<'a>;

    /// Set extranonce2_size to extranonce2_size if provided. If not create a new one and set it.
    fn set_extranonce2_size(&mut self, extra_nonce2_size: Option<usize>) -> usize;

    fn extranonce2_size(&self) -> usize;

    fn version_rolling_mask(&self) -> Option<HexU32Be>;

    fn set_version_rolling_mask(&mut self, mask: Option<HexU32Be>);

    fn set_version_rolling_min_bit(&mut self, mask: Option<HexU32Be>);

    fn update_extranonce(
        &mut self,
        extra_nonce1: Extranonce<'a>,
        extra_nonce2_size: usize,
    ) -> Result<json_rpc::Message, Error<'a>> {
        self.set_extranonce1(Some(extra_nonce1.clone()));
        self.set_extranonce2_size(Some(extra_nonce2_size));

        Ok(server_to_client::SetExtranonce {
            extra_nonce1,
            extra_nonce2_size,
        }
        .into())
    }
    // {"params":["00003000"], "id":null, "method": "mining.set_version_mask"}
    // fn update_version_rolling_mask

    fn notify(&mut self) -> Result<json_rpc::Message, Error<'_>>;

    fn handle_set_difficulty(&mut self, value: f64) -> Result<json_rpc::Message, Error<'_>> {
        let set_difficulty = server_to_client::SetDifficulty { value };
        Ok(set_difficulty.into())
    }
}

pub trait IsClient<'a> {
    /// Deserialize a [raw json_rpc message][a] into a [stratum v1 message][b] and handle the
    /// result.
    ///
    /// [a]: crate::...
    /// [b]:
    fn handle_message(
        &mut self,
        msg: json_rpc::Message,
    ) -> Result<Option<json_rpc::Message>, Error<'a>>
    where
        Self: std::marker::Sized,
    {
        let method: Result<Method<'a>, MethodError<'a>> = msg.try_into();

        match method {
            Ok(m) => match m {
                Method::Server2ClientResponse(response) => {
                    let response = self.update_response(response)?;
                    self.handle_response(response)
                }
                Method::Server2Client(request) => self.handle_request(request),
                Method::Client2Server(_) => Err(Error::InvalidReceiver(m.into())),
                Method::ErrorMessage(msg) => self.handle_error_message(msg),
            },
            Err(e) => Err(e.into()),
        }
    }

    fn update_response(
        &mut self,
        response: methods::Server2ClientResponse<'a>,
    ) -> Result<methods::Server2ClientResponse<'a>, Error<'a>> {
        match &response {
            methods::Server2ClientResponse::GeneralResponse(general) => {
                let is_authorize = self.id_is_authorize(&general.id);
                let is_submit = self.id_is_submit(&general.id);
                match (is_authorize, is_submit) {
                    (Some(prev_name), false) => {
                        let authorize = general.clone().into_authorize(prev_name);
                        Ok(methods::Server2ClientResponse::Authorize(authorize))
                    }
                    (None, false) => Ok(methods::Server2ClientResponse::Submit(
                        general.clone().into_submit(),
                    )),
                    _ => Err(Error::UnknownID(general.id)),
                }
            }
            _ => Ok(response),
        }
    }

    /// Call the right handler according with the called method
    fn handle_request(
        &mut self,
        request: methods::Server2Client<'a>,
    ) -> Result<Option<json_rpc::Message>, Error<'a>>
    where
        Self: std::marker::Sized,
    {
        match request {
            methods::Server2Client::Notify(notify) => {
                self.handle_notify(notify)?;
                Ok(None)
            }
            methods::Server2Client::SetDifficulty(mut set_diff) => {
                self.handle_set_difficulty(&mut set_diff)?;
                Ok(None)
            }
            methods::Server2Client::SetExtranonce(mut set_extra_nonce) => {
                self.handle_set_extranonce(&mut set_extra_nonce)?;
                Ok(None)
            }
            methods::Server2Client::SetVersionMask(mut set_version_mask) => {
                self.handle_set_version_mask(&mut set_version_mask)?;
                Ok(None)
            }
        }
    }

    fn handle_response(
        &mut self,
        response: methods::Server2ClientResponse<'a>,
    ) -> Result<Option<json_rpc::Message>, Error<'a>>
    where
        Self: std::marker::Sized,
    {
        match response {
            methods::Server2ClientResponse::Configure(mut configure) => {
                self.handle_configure(&mut configure)?;
                self.set_version_rolling_mask(configure.version_rolling_mask());
                self.set_version_rolling_min_bit(configure.version_rolling_min_bit());
                self.set_status(ClientStatus::Configured);

                //in sv1 the mining.configure message should be the first message to come in before
                // the subscribe - the subscribe response is where the server hands out the
                // extranonce so it doesnt really matter what the server sets the
                // extranonce to in the mining.configure handler
                debug!("NOTICE: Subscribe extranonce is hardcoded by server");
                let subscribe = self
                    .subscribe(
                        configure.id,
                        Some(Extranonce::try_from(hex::decode("08000002")?)?),
                    )
                    .ok();
                Ok(subscribe)
            }
            methods::Server2ClientResponse::Subscribe(subscribe) => {
                self.handle_subscribe(&subscribe)?;
                self.set_extranonce1(subscribe.extra_nonce1);
                self.set_extranonce2_size(subscribe.extra_nonce2_size);
                self.set_status(ClientStatus::Subscribed);
                Ok(None)
            }
            methods::Server2ClientResponse::Authorize(authorize) => {
                if authorize.is_ok() {
                    self.authorize_user_name(authorize.user_name());
                };
                Ok(None)
            }
            methods::Server2ClientResponse::Submit(_) => Ok(None),
            // impossible state
            methods::Server2ClientResponse::GeneralResponse(_) => panic!(),
            methods::Server2ClientResponse::SetDifficulty(_) => Ok(None),
        }
    }

    fn handle_error_message(
        &mut self,
        message: Message,
    ) -> Result<Option<json_rpc::Message>, Error<'a>>;

    /// Check if the client sent an Authorize request with the given id, if so it return the
    /// authorized name
    fn id_is_authorize(&mut self, id: &u64) -> Option<String>;

    /// Check if the client sent a Submit request with the given id
    fn id_is_submit(&mut self, id: &u64) -> bool;

    fn handle_notify(&mut self, notify: server_to_client::Notify<'a>) -> Result<(), Error<'a>>;

    fn handle_configure(&mut self, conf: &mut server_to_client::Configure)
        -> Result<(), Error<'a>>;

    fn handle_set_difficulty(
        &mut self,
        m: &mut server_to_client::SetDifficulty,
    ) -> Result<(), Error<'a>>;

    fn handle_set_extranonce(
        &mut self,
        m: &mut server_to_client::SetExtranonce,
    ) -> Result<(), Error<'a>>;

    fn handle_set_version_mask(
        &mut self,
        m: &mut server_to_client::SetVersionMask,
    ) -> Result<(), Error<'a>>;

    fn handle_subscribe(
        &mut self,
        subscribe: &server_to_client::Subscribe<'a>,
    ) -> Result<(), Error<'a>>;

    fn set_extranonce1(&mut self, extranonce1: Extranonce<'a>);

    fn extranonce1(&self) -> Extranonce<'a>;

    fn set_extranonce2_size(&mut self, extra_nonce2_size: usize);

    fn extranonce2_size(&self) -> usize;

    fn version_rolling_mask(&self) -> Option<HexU32Be>;

    fn set_version_rolling_mask(&mut self, mask: Option<HexU32Be>);

    fn set_version_rolling_min_bit(&mut self, min: Option<HexU32Be>);

    fn version_rolling_min_bit(&mut self) -> Option<HexU32Be>;

    fn set_status(&mut self, status: ClientStatus);

    fn signature(&self) -> String;

    fn status(&self) -> ClientStatus;

    fn last_notify(&self) -> Option<server_to_client::Notify<'_>>;

    /// Check if the given user_name has been authorized by the server
    #[allow(clippy::ptr_arg)]
    fn is_authorized(&self, name: &String) -> bool;

    /// Register the given user_name has authorized by the server
    fn authorize_user_name(&mut self, name: String);

    fn configure(&mut self, id: u64) -> json_rpc::Message {
        if self.version_rolling_min_bit().is_none() && self.version_rolling_mask().is_none() {
            client_to_server::Configure::void(id).into()
        } else {
            client_to_server::Configure::new(
                id,
                self.version_rolling_mask(),
                self.version_rolling_min_bit(),
            )
            .into()
        }
    }

    fn subscribe(
        &mut self,
        id: u64,
        extranonce1: Option<Extranonce<'a>>,
    ) -> Result<json_rpc::Message, Error<'a>> {
        match self.status() {
            ClientStatus::Init => Err(Error::IncorrectClientStatus("mining.subscribe".to_string())),
            _ => Ok(client_to_server::Subscribe {
                id,
                agent_signature: self.signature(),
                extranonce1,
            }
            .try_into()?),
        }
    }

    fn authorize(
        &mut self,
        id: u64,
        name: String,
        password: String,
    ) -> Result<json_rpc::Message, Error<'_>> {
        match self.status() {
            ClientStatus::Init => Err(Error::IncorrectClientStatus("mining.authorize".to_string())),
            _ => Ok(client_to_server::Authorize { id, name, password }.into()),
        }
    }

    fn submit(
        &mut self,
        id: u64,
        user_name: String,
        extra_nonce2: Extranonce<'a>,
        time: i64,
        nonce: i64,
        version_bits: Option<HexU32Be>,
    ) -> Result<json_rpc::Message, Error<'a>> {
        match self.status() {
            ClientStatus::Init => Err(Error::IncorrectClientStatus("mining.submit".to_string())),
            _ => {
                if let Some(notify) = self.last_notify() {
                    if !self.is_authorized(&user_name) {
                        return Err(Error::UnauthorizedClient(user_name));
                    }
                    Ok(client_to_server::Submit {
                        job_id: notify.job_id,
                        user_name,
                        extra_nonce2,
                        time: HexU32Be(time as u32),
                        nonce: HexU32Be(nonce as u32),
                        version_bits,
                        id,
                    }
                    .into())
                } else {
                    Err(Error::IncorrectClientStatus(
                        "No Notify instance found".to_string(),
                    ))
                }
            }
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ClientStatus {
    Init,
    Configured,
    Subscribed,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    // A minimal implementation of IsServer trait for testing
    struct TestServer<'a> {
        authorized_users: HashSet<String>,
        extranonce1: Extranonce<'a>,
        extranonce2_size: usize,
        version_rolling_mask: Option<HexU32Be>,
        version_rolling_min_bit: Option<HexU32Be>,
    }

    impl<'a> TestServer<'a> {
        fn new(extranonce1: Extranonce<'a>, extranonce2_size: usize) -> Self {
            Self {
                authorized_users: HashSet::new(),
                extranonce1,
                extranonce2_size,
                version_rolling_mask: None,
                version_rolling_min_bit: None,
            }
        }
    }

    impl<'a> IsServer<'a> for TestServer<'a> {
        fn handle_configure(
            &mut self,
            _request: &client_to_server::Configure,
        ) -> (Option<server_to_client::VersionRollingParams>, Option<bool>) {
            (None, None)
        }

        fn handle_subscribe(
            &self,
            _request: &client_to_server::Subscribe,
        ) -> Vec<(String, String)> {
            vec![("mining.notify".to_string(), "1".to_string())]
        }

        fn handle_authorize(&self, _request: &client_to_server::Authorize) -> bool {
            true
        }

        fn notify(&mut self) -> Result<json_rpc::Message, Error<'_>> {
            Ok(json_rpc::Message::StandardRequest(
                json_rpc::StandardRequest {
                    id: 1,
                    method: "mining.notify".to_string(),
                    params: serde_json::json!([]),
                },
            ))
        }

        fn handle_submit(&self, _request: &client_to_server::Submit<'a>) -> bool {
            true
        }

        fn handle_extranonce_subscribe(&self) {}

        fn is_authorized(&self, name: &str) -> bool {
            self.authorized_users.contains(name)
        }

        fn authorize(&mut self, name: &str) {
            self.authorized_users.insert(name.to_string());
        }

        fn set_extranonce1(&mut self, extranonce1: Option<Extranonce<'a>>) -> Extranonce<'a> {
            if let Some(extranonce1) = extranonce1 {
                self.extranonce1 = extranonce1;
            }
            self.extranonce1.clone()
        }

        fn extranonce1(&self) -> Extranonce<'a> {
            self.extranonce1.clone()
        }

        fn set_extranonce2_size(&mut self, extra_nonce2_size: Option<usize>) -> usize {
            if let Some(extra_nonce2_size) = extra_nonce2_size {
                self.extranonce2_size = extra_nonce2_size;
            }
            self.extranonce2_size
        }

        fn extranonce2_size(&self) -> usize {
            self.extranonce2_size
        }

        fn version_rolling_mask(&self) -> Option<HexU32Be> {
            None
        }

        fn set_version_rolling_mask(&mut self, mask: Option<HexU32Be>) {
            self.version_rolling_mask = mask;
        }

        fn set_version_rolling_min_bit(&mut self, mask: Option<HexU32Be>) {
            self.version_rolling_min_bit = mask;
        }
    }

    #[test]
    fn test_server_handle_invalid_message() {
        let extranonce1 = Extranonce::try_from(hex::decode("08000002").unwrap()).unwrap();
        let mut server = TestServer::new(extranonce1, 4);

        // Create an invalid message (response)
        let request_message = json_rpc::Message::StandardRequest(json_rpc::StandardRequest {
            id: 42,
            method: "mining.subscribe_bad".to_string(),
            params: serde_json::json!([]),
        });

        let result = server.handle_message(request_message);

        assert!(result.is_err());
        match result.unwrap_err() {
            Error::Method(inner) => match *inner {
                MethodError::MethodNotFound(_) => {}
                other => panic!("Expected MethodNotFound error, got {:?}", other),
            },
            other => panic!("Expected Error::Method, got {:?}", other),
        }
    }
}
