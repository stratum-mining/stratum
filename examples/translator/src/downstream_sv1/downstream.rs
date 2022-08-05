use crate::downstream_sv1::DownstreamConnection;
use async_channel::{Receiver, Sender};
use async_std::{io::BufReader, net::TcpStream, prelude::*, task};
use roles_logic_sv2::{
    common_properties::{IsDownstream, IsMiningDownstream},
    utils::Mutex,
};
use std::sync::Arc;
use v1::{
    client_to_server, json_rpc, server_to_client,
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
    // receiver_incoming: Receiver<String>,
    // sender_outgoing: Sender<String>,
    connection: DownstreamConnection,
}

impl Downstream {
    pub async fn new(
        stream: TcpStream,
        sender_upstream: Sender<json_rpc::Message>,
        receiver_upstream: Receiver<json_rpc::Message>,
    ) -> Arc<Mutex<Self>> {
        let stream = std::sync::Arc::new(stream);

        // Reads and writes from Downstream SV1 Mining Device Client
        let (socket_reader, socket_writer) = (stream.clone(), stream);

        let connection = DownstreamConnection {
            sender_upstream,
            receiver_upstream,
        };
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
        // message as `json_rpc::Message` + sends to upstream `Translator.receive_downstream` via
        // `sender_upstream` (done in `send_message_upstream`.
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
                    println!("DOWNSTREAM RECV: {:?}", &incoming);
                    Self::send_message_upstream(self_.clone(), incoming.unwrap()).await;
                }
            }
        });

        // Task to loop over the `receiver_outgoing` waiting to receive `json_rpc::Message`
        // messages from `Translator.sender_downstream` to be written to the SV1 Mining Device
        // Client socket.
        // RR NOTE: may need to have another task to receive from `Translator.sender_downstream`
        // into `receiver_upstream`, then pass that to `sender_outgoing` which then sends to
        // `receiver_outgoing` which should replace the `receiver_upstream` here.
        task::spawn(async move {
            loop {
                let to_send = receiver_upstream_clone.recv().await.unwrap();
                let to_send = format!("{}\n", serde_json::to_string(&to_send).unwrap());
                (&*socket_writer)
                    .write_all(to_send.as_bytes())
                    .await
                    .unwrap();
            }
        });

        downstream
    }

    /// Sends SV1 message to the Upstream Translator to be translated to SV2 and sent to the
    /// Upstream role (most typically a SV2 Pool).
    async fn send_message_upstream(self_: Arc<Mutex<Self>>, msg: json_rpc::Message) {
        println!("RRMSG: {:?}", &msg);
        let sender = self_
            .safe_lock(|s| s.connection.sender_upstream.clone())
            .unwrap();
        sender.send(msg).await.unwrap()
    }
}

impl IsServer for Downstream {
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
