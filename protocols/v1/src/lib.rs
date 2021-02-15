//! Startum V1 application protocol: TODO
//! json-rpc has two types of messages: **request** and **response**.
//! A request message can be either a **notification** or a **standard message**.
//! Standard messegaes expect a response notifications no. A typical example of a notification
//! is the broadcasting of a new block.
//!
//! Every RPC request contains three parts:
//! * message ID: integer or string
//! * remote method: unicode string
//! * parameters: list of parameters
//!
//! ## Standard requests
//! Mmessage ID must be an unique identifier of request during current transport session. It may be
//! integer or some unique string, like UUID. ID must be unique only from one side (it means, both
//! server and clients can initiate request with id “1”). Client or server can choose string/UUID
//! identifier for example in the case when standard “atomic” counter isn’t available.
//!
//! ## Notifications
//! Notifications are like Request, but it does not expect any response and message ID is always null:
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
//! TODO dire qlks sul fatto che configure non e' tarttata come parte estensibile del protocollo
//! perche bla bla bla
//!
//! References:
//! [https://docs.google.com/document/d/17zHy1SUlhgtCMbypO8cHgpWH73V5iUQKk_0rWvMqSNs/edit?hl=en_US#]
//! [https://braiins.com/stratum-v1/docs]
//! [https://en.bitcoin.it/wiki/Stratum_mining_protocol]
//! [https://en.bitcoin.it/wiki/BIP_0310]
//! [https://docs.google.com/spreadsheets/d/1z8a3S9gFkS8NGhBCxOMUDqs7h9SQltz8-VX3KPHk7Jw/edit#gid=0]

pub mod error;
pub mod json_rpc;
mod methods;
pub mod utils;

use std::convert::TryInto;

// use error::Result;
use error::Error;
pub use json_rpc::Message;
pub use methods::client_to_server;
pub use methods::server_to_client;
pub use methods::MethodError;
use utils::{HexBytes, HexU32Be};

// pub trait Handler {
//     fn respond(request: methods::Message) -> Option<methods::Message>;
// }

/// Trait implemented by a startum v1 server (proxy or pool)
/// bla bla bla TODO
///
/// json_rpc Response are not handled cause startum v1 do not have any request from a server to a
/// client
///
/// A stratum v1 server rapresent a single connection with a client
///
pub trait IsServer {
    /// Deserialize a [raw json_rpc message][a] into a [stratum v1 message][b] and handle the
    /// result.
    ///
    /// [a]: TODO // crate::...
    /// [b]: TODO
    ///
    fn handle_message(
        &mut self,
        msg: json_rpc::Message,
    ) -> Result<Option<json_rpc::Response>, Error>
    where
        Self: std::marker::Sized,
    {
        // Server shoudln't receive json_rpc responses
        if msg.is_response() {
            Err(Error::InvalidJsonRpcMessageKind)
        } else {
            self.handle_request(msg.try_into()?)
        }
    }

    /// Call the right handler according with the called method
    ///
    fn handle_request(
        &mut self,
        request: methods::Client2Server,
    ) -> Result<Option<json_rpc::Response>, Error>
    where
        Self: std::marker::Sized,
    {
        match request {
            methods::Client2Server::Authorize(authorize) => {
                let authorized = self.handle_authorize(&authorize);
                if authorized {
                    self.authorize(&authorize.name);
                }
                Ok(Some(authorize.respond(authorized)))
            }
            methods::Client2Server::Configure(configure) => {
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
                        self.version_rolling_mask().is_some()
                            && self.version_rolling_mask().unwrap().check_mask(a)
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
    ///
    fn handle_subscribe(&self, request: &client_to_server::Subscribe) -> Vec<(String, String)>;

    /// You can authorize as many workers as you wish and at any
    /// time during the session. In this way, you can handle big basement of independent mining rigs
    /// just by one Stratum connection.
    ///
    /// https://bitcoin.stackexchange.com/questions/29416/how-do-pool-servers-handle-multiple-workers-sharing-one-connection-with-stratum
    ///
    fn handle_authorize(&self, request: &client_to_server::Authorize) -> bool;

    /// When miner find the job which meets requested difficulty, it can submit share to the server.
    /// Only [Submit](client_to_server::Submit) requests for authorized user names can be submitted.
    ///
    fn handle_submit(&self, request: &client_to_server::Submit) -> bool;

    /// Indicates to the server that the client supports the mining.set_extranonce method.
    fn handle_extranonce_subscribe(&self);

    fn is_authorized(&self, name: &str) -> bool;

    fn authorize(&mut self, name: &str);

    /// Set extranonce1 to extranonce1 if provided. If not create a new one and set it.
    fn set_extranonce1(&mut self, extranonce1: Option<HexBytes>) -> HexBytes;

    fn extranonce1(&self) -> HexBytes;

    /// Set extranonce2_size to extranonce2_size if provided. If not create a new one and set it.
    fn set_extranonce2_size(&mut self, extra_nonce2_size: Option<usize>) -> usize;

    fn extranonce2_size(&self) -> usize;

    fn version_rolling_mask(&self) -> Option<HexU32Be>;

    fn set_version_rolling_mask(&mut self, mask: Option<HexU32Be>);

    fn set_version_rolling_min_bit(&mut self, mask: Option<HexU32Be>);

    fn update_extranonce(
        &mut self,
        extra_nonce1: HexBytes,
        extra_nonce2_size: usize,
    ) -> Result<json_rpc::Message, ()> {
        self.set_extranonce1(Some(extra_nonce1.clone()));
        self.set_extranonce2_size(Some(extra_nonce2_size));
        Ok((server_to_client::SetExtranonce {
            extra_nonce1,
            extra_nonce2_size,
        })
        .try_into()
        .map_err(|_| ())?)
    }
    // {"params":["00003000"], "id":null, "method": "mining.set_version_mask"}
    // TODO fn update_version_rolling_mask

    fn notify(&mut self) -> Result<json_rpc::Message, ()> {
        // TODO
        Ok(server_to_client::Notify {
            job_id: "ciao".to_string(),
            prev_hash: utils::PrevHash(vec![3 as u8, 4, 5, 6]),
            coin_base1: "ffff".try_into().unwrap(),
            coin_base2: "ffff".try_into().unwrap(),
            merkle_branch: vec!["fff".try_into().unwrap()],
            version: utils::HexU32Be(5667),
            bits: utils::HexU32Be(5678),
            time: utils::HexU32Be(5609),
            clean_jobs: true,
        }
        .try_into()?)
    }
}

pub trait IsClient {
    /// Deserialize a [raw json_rpc message][a] into a [stratum v1 message][b] and handle the
    /// result.
    ///
    /// [a]: TODO // crate::...
    /// [b]: TODO
    ///
    fn handle_message(
        &mut self,
        msg: json_rpc::Message,
    ) -> Result<Option<json_rpc::Response>, Error>
    where
        Self: std::marker::Sized,
    {
        if msg.is_response() {
            // A respnse can be an error if ...
            if msg.error().is_some() {
                todo!();
            }

            // A response can be either a Server2ClientResponse in the cases ... or a general
            // response in the cases ...
            match TryInto::<methods::Server2ClientResponse>::try_into(msg.clone()) {
                Ok(a) => self.handle_response(a).map(|_| None),
                Err(_) => match msg {
                    json_rpc::Message::Response(msg) => {
                        let response = self.response_from_id(msg)?;
                        self.handle_response(response).map(|_| None)
                    }
                    _ => panic!(), // TODO this case should be impossible
                },
            }
        } else {
            self.handle_request(msg.try_into()?)
        }
    }

    /// Call the right handler according with the called method
    ///
    fn handle_request(
        &mut self,
        request: methods::Server2Client,
    ) -> Result<Option<json_rpc::Response>, Error>
    where
        Self: std::marker::Sized,
    {
        match request {
            methods::Server2Client::Notify(notify) => {
                self.handle_notify(notify)?;
                Ok(None)
            }
            methods::Server2Client::SetDifficulty(_set_diff) => todo!(),
            methods::Server2Client::SetExtranonce(_set_extra_nonce) => todo!(),
            methods::Server2Client::SetVersionMask(_set_version_mask) => todo!(),
        }
    }

    /// Check if the given response has an id that correspond to an already sent request, if so it
    /// return the rispective response if not fail
    ///
    fn response_from_id(
        &mut self,
        response: json_rpc::Response,
    ) -> Result<methods::Server2ClientResponse, Error> {
        if self.id_is_authorize(&response.id).is_some() {
            let name = self.id_is_authorize(&response.id).unwrap();
            let resp = server_to_client::Authorize(response, name);
            Ok(methods::Server2ClientResponse::Authorize(resp))
        } else if self.id_is_submit(&response.id) {
            let resp = server_to_client::Submit(response);
            Ok(methods::Server2ClientResponse::Submit(resp))
        } else {
            Err(Error::UnknownID)
        }
    }

    fn handle_response(&mut self, response: methods::Server2ClientResponse) -> Result<(), Error>
    where
        Self: std::marker::Sized,
    {
        match response {
            methods::Server2ClientResponse::Configure(mut configure) => {
                self.handle_configure(&mut configure)?;
                self.set_version_rolling_mask(configure.version_rolling_mask());
                self.set_version_rolling_min_bit(configure.version_rolling_min_bit());
                self.set_status(ClientStatus::Configured);
                Ok(())
            }
            methods::Server2ClientResponse::Subscribe(subscribe) => {
                self.handle_subscribe(&subscribe)?;
                self.set_extranonce1(subscribe.extra_nonce1);
                self.set_extranonce2_size(subscribe.extra_nonce2_size);
                self.set_status(ClientStatus::Subscribed);
                Ok(())
            }
            methods::Server2ClientResponse::Authorize(authorize) => {
                if authorize.is_ok() {
                    self.authorize_user_name(authorize.user_name());
                };
                Ok(())
            }
            methods::Server2ClientResponse::Submit(_) => Ok(()),
        }
    }

    /// Check if the client sent an Authorize request with the given id, if so it return the
    /// authorized name
    fn id_is_authorize(&mut self, id: &str) -> Option<String>;

    /// Check if the client sent a Submit request with the given id
    fn id_is_submit(&mut self, id: &str) -> bool;

    //// TODO qui error si deve poter espandere ... /////
    fn handle_notify(&mut self, notify: server_to_client::Notify) -> Result<(), Error>;

    //// TODO qui error si deve poter espandere ... /////
    fn handle_configure(&self, conf: &mut server_to_client::Configure) -> Result<(), Error>;

    //// TODO qui error si deve poter espandere ... /////
    fn handle_subscribe(&mut self, subscribe: &server_to_client::Subscribe) -> Result<(), Error>;

    fn set_extranonce1(&mut self, extranonce1: HexBytes);

    fn extranonce1(&self) -> HexBytes;

    fn set_extranonce2_size(&mut self, extra_nonce2_size: usize);

    fn extranonce2_size(&self) -> usize;

    fn version_rolling_mask(&self) -> Option<HexU32Be>;

    fn set_version_rolling_mask(&mut self, mask: Option<HexU32Be>);

    fn set_version_rolling_min_bit(&mut self, min: Option<HexU32Be>);

    fn version_rolling_min_bit(&mut self) -> Option<HexU32Be>;

    fn set_status(&mut self, status: ClientStatus);

    fn signature(&self) -> String;

    fn status(&self) -> ClientStatus;

    fn last_notify(&self) -> Option<server_to_client::Notify>;

    /// Check if the given user_name has been authorized by the server
    fn is_authorized(&self, name: &String) -> bool;

    /// Register the given user_name has authorized by the server
    fn authorize_user_name(&mut self, name: String);

    fn configure(&mut self, id: String) -> json_rpc::Message {
        client_to_server::Configure::new(
            id,
            self.version_rolling_mask(),
            self.version_rolling_min_bit(),
        )
        .into()
    }

    fn subscribe(
        &mut self,
        id: String,
        extranonce1: Option<HexBytes>,
    ) -> Result<json_rpc::Message, ()> {
        match self.status() {
            ClientStatus::Init => Err(()),
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
        id: String,
        name: String,
        password: String,
    ) -> Result<json_rpc::Message, ()> {
        match self.status() {
            ClientStatus::Init => Err(()),
            _ => Ok(client_to_server::Authorize { id, name, password }.into()),
        }
    }

    fn submit(
        &mut self,
        id: String,
        user_name: String,
        extra_nonce2: HexBytes,
        time: i64,
        nonce: i64,
        version_bits: Option<HexU32Be>,
    ) -> Result<json_rpc::Message, ()> {
        match self.status() {
            ClientStatus::Init => Err(()),
            _ => {
                // TODO check if version_bits is set
                if self.last_notify().is_none() {
                    Err(())
                } else if self.is_authorized(&user_name) {
                    Ok(client_to_server::Submit {
                        job_id: self.last_notify().unwrap().job_id,
                        user_name,
                        extra_nonce2,
                        time,
                        nonce,
                        version_bits,
                        id,
                    }
                    .into())
                } else {
                    Err(())
                }
            }
        }
    }
}

#[derive(Copy, Clone)]
pub enum ClientStatus {
    Init,
    Configured,
    Subscribed,
}
