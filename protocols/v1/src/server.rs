// use error::Result;
use crate::{
    error::Error,
    json_rpc,
    methods::{self, client_to_server, server_to_client, Method, MethodError, ParsingMethodError},
    utils::{Extranonce, HexU32Be},
};
use std::convert::{TryFrom, TryInto};
use tracing::debug;

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
            json_rpc::Message::StandardRequest(_) => {
                // handle valid standard request
                self.handle_request(msg)
            }
            json_rpc::Message::Notification(_) => {
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

    fn notify(&mut self) -> Result<json_rpc::Message, Error>;

    fn handle_set_difficulty(&mut self, value: f64) -> Result<json_rpc::Message, Error> {
        let set_difficulty = server_to_client::SetDifficulty { value };
        Ok(set_difficulty.into())
    }
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

        fn notify(&mut self) -> Result<json_rpc::Message, Error> {
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
            Error::Method(MethodError::MethodNotFound(_)) => {}
            other => panic!("Expected MethodNotFound error, got {:?}", other),
        }
    }
}
