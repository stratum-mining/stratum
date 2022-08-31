use crate::{downstream_sv1::DownstreamConnection, error::ProxyResult};
use async_channel::{bounded, Receiver, Sender};
use async_std::{
    io::BufReader,
    net::{TcpListener, TcpStream},
    prelude::*,
    task,
};
use roles_logic_sv2::{
    common_properties::{IsDownstream, IsMiningDownstream},
    utils::Mutex,
};
use std::sync::Arc;
use v1::{
    client_to_server,
    error::Error as V1Error,
    json_rpc, methods, server_to_client,
    utils::{self, HexBytes, HexU32Be},
    IsServer,
};

/// Handles the sending and receiving of messages to and from an SV2 Upstream role (most typically
/// a SV2 Pool server).
#[derive(Debug)]
pub(crate) struct Downstream {
    authorized_names: Vec<String>,
    extranonce1: HexBytes,
    extranonce2_size: usize,
    version_rolling_mask: Option<HexU32Be>,
    version_rolling_min_bit: Option<HexU32Be>,
    connection: DownstreamConnection,
}

impl Downstream {
    pub async fn new(
        stream: TcpStream,
        sender_upstream: Sender<json_rpc::Message>,
        receiver_upstream: Receiver<json_rpc::Message>,
    ) -> ProxyResult<Arc<Mutex<Self>>> {
        let stream = std::sync::Arc::new(stream);

        // Reads and writes from Downstream SV1 Mining Device Client
        let (socket_reader, socket_writer) = (stream.clone(), stream);
        let (sender_outgoing, receiver_outgoing) = bounded(10);

        let receiver_outgoing_clone = receiver_outgoing.clone();
        let socket_writer_clone = socket_writer.clone();

        let connection = DownstreamConnection::new(
            sender_outgoing,
            receiver_outgoing,
            sender_upstream,
            receiver_upstream,
        );
        let receiver_upstream_clone = connection.receiver_upstream.clone();

        let downstream = Arc::new(Mutex::new(Downstream {
            authorized_names: vec![],
            extranonce1: "00000000".try_into().unwrap(),
            extranonce2_size: 2,
            version_rolling_mask: None,
            version_rolling_min_bit: None,
            connection,
        }));
        let self_ = downstream.clone();

        // Task to read from SV1 Mining Device Client socket via `socket_reader`. Parses received
        // message as `json_rpc::Message` + sends to upstream `Translator.receiver_for_downstream`
        // via `sender_upstream`
        task::spawn(async move {
            loop {
                // Read message from SV1 Mining Device Client socket
                let mut messages = BufReader::new(&*socket_reader).lines();
                // On message receive, parse to `json_rpc:Message` and send to Upstream
                // `Translator.receive_downstream` via `sender_upstream` done in
                // `send_message_upstream`.
                while let Some(incoming) = messages.next().await {
                    let incoming = incoming.unwrap();
                    let incoming: Result<json_rpc::Message, _> = serde_json::from_str(&incoming);
                    println!(
                        "TD RECV MSG FROM DOWNSTREAM: {:?}",
                        incoming.as_ref().unwrap()
                    );
                    // Handle what to do with message
                    Self::handle_incoming_sv1(self_.clone(), incoming.unwrap()).await;
                    // let message_sv1 = Self::handle_incoming_sv1(self_.clone(), incoming.unwrap());
                    // if let Some(message_to_translate) = message_sv1 {
                    //     Self::send_message_upstream(self_.clone(), message_to_translate).await;
                    // }
                }
            }
        });

        // Task to loop over the `receiver_upstream` waiting to receive `json_rpc::Message`
        // messages from `Translator.sender_to_downstream` to be written to the SV1 Mining Device
        // Client socket.
        task::spawn(async move {
            loop {
                let to_send = receiver_upstream_clone.recv().await.unwrap();
                // TODO: Use `Error::bad_serde_json`
                let to_send = format!("{}\n", serde_json::to_string(&to_send).unwrap());
                (&*socket_writer)
                    .write_all(to_send.as_bytes())
                    .await
                    .unwrap();
            }
        });

        // Wait for SV1 responses that do not need to go through the Translator, but can be sent
        // back the SV1 Mining Device directly
        task::spawn(async move {
            loop {
                let to_send = receiver_outgoing_clone.recv().await.unwrap();
                // TODO: Use `Error::bad_serde_json`
                let to_send = format!("{}\n", serde_json::to_string(&to_send).unwrap());
                (&*socket_writer_clone)
                    .write_all(to_send.as_bytes())
                    .await
                    .unwrap();
            }
        });

        Ok(downstream)
    }

    /// Accept connections from one or more SV1 Downstream roles (SV1 Mining Devices).
    pub(crate) fn accept_connections(
        sender_for_downstream: Sender<json_rpc::Message>,
        receiver_for_downstream: Receiver<json_rpc::Message>,
    ) {
        task::spawn(async move {
            let downstream_listener = TcpListener::bind(crate::LISTEN_ADDR).await.unwrap();
            let mut downstream_incoming = downstream_listener.incoming();
            while let Some(stream) = downstream_incoming.next().await {
                let stream = stream.unwrap();
                println!(
                    "\nPROXY SERVER - ACCEPTING FROM DOWNSTREAM: {}\n",
                    stream.peer_addr().unwrap()
                );
                let server = Downstream::new(
                    stream,
                    sender_for_downstream.clone(),
                    receiver_for_downstream.clone(),
                )
                .await
                .unwrap();
                Arc::new(Mutex::new(server));
            }
        });
    }

    /// As SV1 messages come in, determines if the message response needs to be translated to SV2
    /// and sent to the `Upstream`, or if a direct response can be sent back by the `Translator`
    /// (SV1 and SV2 protocol messages are NOT 1-to-1).
    async fn handle_incoming_sv1(self_: Arc<Mutex<Self>>, message_sv1: json_rpc::Message) {
        let message_sv1_clone = message_sv1.clone();
        // `handle_message` in `IsServer` trait + calls `handle_request`
        // TODO: Map err from V1Error to Error::V1Error
        let response = self_.safe_lock(|s| s.handle_message(message_sv1)).unwrap();
        match response {
            Ok(res) => {
                if let Some(r) = res {
                    // If some response is received, indicates no messages translation is needed
                    // and response should be sent directly to the SV1 Downstream. Otherwise,
                    // message will be sent to the upstream Translator to be translated to SV2 and
                    // forwarded to the `Upstream`
                    // let sender = self_.safe_lock(|s| s.connection.sender_upstream)
                    Self::send_message_downstream(self_, r).await;
                } else {
                    // If None response is received, indicates this SV1 message received from the
                    // Downstream MD is passed to the `Translator` for translation into SV2
                    Self::send_message_upstream(self_, message_sv1_clone).await;
                }
            }
            Err(e) => {
                // Err(Error::V1Error(e))
                panic!(
                    "Error::InvalidJsonRpcMessageKind, sever shouldnt receive json_rpc responsese: `{:?}`",
                    e);
            }
        }
    }

    /// Send SV1 Response that is generated by `Downstream` (not received by upstream `Translator`)
    /// to be written to the SV1 Downstream Mining Device socket
    async fn send_message_downstream(self_: Arc<Mutex<Self>>, response: json_rpc::Response) {
        println!("DT SEND SV1 MSG TO DOWNSTREAM: {:?}", &response);
        let sender = self_
            .safe_lock(|s| s.connection.sender_outgoing.clone())
            .unwrap();
        sender.send(response).await.unwrap();
    }

    /// Sends SV1 message to the Upstream Translator to be translated to SV2 and sent to the
    /// Upstream role (most typically a SV2 Pool).
    async fn send_message_upstream(self_: Arc<Mutex<Self>>, msg: json_rpc::Message) {
        println!("DT SEND SV1 MSG TO TP: {:?}", &msg);
        let sender = self_
            .safe_lock(|s| s.connection.sender_upstream.clone())
            .unwrap();
        sender.send(msg).await.unwrap()
    }
}

/// Implements `IsServer` for `Downstream` to handle the SV1 messages.
impl IsServer for Downstream {
    fn handle_request(
        &mut self,
        request: methods::Client2Server,
    ) -> Result<Option<json_rpc::Response>, V1Error>
    where
        Self: std::marker::Sized,
    {
        match request {
            methods::Client2Server::Authorize(authorize) => {
                println!("DT HANDLE AUTHORIZE");
                let authorized = self.handle_authorize(&authorize);
                if authorized {
                    self.authorize(&authorize.name);
                }
                Ok(Some(authorize.respond(authorized)))
            }
            methods::Client2Server::Configure(configure) => {
                println!("DT HANDLE CONFIGURE");
                self.set_version_rolling_mask(configure.version_rolling_mask());
                self.set_version_rolling_min_bit(configure.version_rolling_min_bit_count());
                let (version_rolling, min_diff) = self.handle_configure(&configure);
                Ok(Some(configure.respond(version_rolling, min_diff)))
            }
            methods::Client2Server::ExtranonceSubscribe(_) => {
                todo!()
                // println!("DT HANDLE EXTRANONCESUBSCRIBE");
                // self.handle_extranonce_subscribe();
                // Ok(None)
            }
            methods::Client2Server::Submit(_submit) => {
                todo!()
                // println!("DT HANDLE SUBMIT");
                // let has_valid_version_bits = match &submit.version_bits {
                //     Some(a) => {
                //         if let Some(version_rolling_mask) = self.version_rolling_mask() {
                //             version_rolling_mask.check_mask(a)
                //         } else {
                //             false
                //         }
                //     }
                //     None => self.version_rolling_mask().is_none(),
                // };
                //
                // let is_valid_submission = self.is_authorized(&submit.user_name)
                //     && self.extranonce2_size() == submit.extra_nonce2.len()
                //     && has_valid_version_bits;
                //
                // if is_valid_submission {
                //     let accepted = self.handle_submit(&submit);
                //     Ok(Some(submit.respond(accepted)))
                // } else {
                //     Err(V1Error::InvalidSubmission)
                // }
            }
            methods::Client2Server::Subscribe(subscribe) => {
                // On the receive of SV1 Subscribe, need to format a response with set_difficulty
                // + mining.notify from the SV2 SetNewPrevHash + NewExtendedMiningJob
                println!("DT HANDLE SUBSCRIBE");
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

    fn handle_configure(
        &mut self,
        _request: &client_to_server::Configure,
    ) -> (Option<server_to_client::VersionRollingParams>, Option<bool>) {
        self.version_rolling_mask = self.version_rolling_mask.clone().map_or(
            Some(crate::downstream_sv1::new_version_rolling_mask()),
            Some,
        );
        self.version_rolling_min_bit = self
            .version_rolling_mask
            .clone()
            .map_or(Some(crate::downstream_sv1::new_version_rolling_min()), Some);
        (
            Some(server_to_client::VersionRollingParams::new(
                self.version_rolling_mask.clone().unwrap(),
                self.version_rolling_min_bit.clone().unwrap(),
            )),
            Some(false),
        )
    }

    fn handle_subscribe(&self, _request: &client_to_server::Subscribe) -> Vec<(String, String)> {
        vec![]
    }

    fn handle_authorize(&self, _request: &client_to_server::Authorize) -> bool {
        true
    }

    fn handle_submit(&self, _request: &client_to_server::Submit) -> bool {
        true
    }

    /// Indicates to the server that the client supports the mining.set_extranonce method.
    fn handle_extranonce_subscribe(&self) {}

    fn is_authorized(&self, _name: &str) -> bool {
        true
    }

    fn authorize(&mut self, name: &str) {
        self.authorized_names.push(name.to_string())
    }

    /// Set extranonce1 to extranonce1 if provided. If not create a new one and set it.
    fn set_extranonce1(&mut self, extranonce1: Option<HexBytes>) -> HexBytes {
        self.extranonce1 = extranonce1.unwrap_or_else(crate::downstream_sv1::new_extranonce);
        self.extranonce1.clone()
    }

    fn extranonce1(&self) -> HexBytes {
        self.extranonce1.clone()
    }

    /// Set extranonce2_size to extranonce2_size if provided. If not create a new one and set it.
    fn set_extranonce2_size(&mut self, extra_nonce2_size: Option<usize>) -> usize {
        self.extranonce2_size =
            extra_nonce2_size.unwrap_or_else(crate::downstream_sv1::new_extranonce2_size);
        self.extranonce2_size
    }

    fn extranonce2_size(&self) -> usize {
        self.extranonce2_size
    }

    fn version_rolling_mask(&self) -> Option<HexU32Be> {
        self.version_rolling_mask.clone()
    }

    fn set_version_rolling_mask(&mut self, mask: Option<HexU32Be>) {
        self.version_rolling_mask = mask;
    }

    fn set_version_rolling_min_bit(&mut self, mask: Option<HexU32Be>) {
        self.version_rolling_min_bit = mask
    }

    fn notify(&mut self) -> Result<json_rpc::Message, ()> {
        server_to_client::Notify {
            job_id: "ciao".to_string(),
            prev_hash: utils::PrevHash(vec![3_u8, 4, 5, 6]),
            coin_base1: "ffff".try_into().unwrap(),
            coin_base2: "ffff".try_into().unwrap(),
            merkle_branch: vec!["fff".try_into().unwrap()],
            version: utils::HexU32Be(5667),
            bits: utils::HexU32Be(5678),
            time: utils::HexU32Be(5609),
            clean_jobs: true,
        }
        .try_into()
    }
}

impl IsMiningDownstream for Downstream {}

impl IsDownstream for Downstream {
    fn get_downstream_mining_data(
        &self,
    ) -> roles_logic_sv2::common_properties::CommonDownstreamData {
        todo!()
    }
}
