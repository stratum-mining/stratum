use crate::{downstream_sv1, error::ProxyResult};
use async_channel::{bounded, Receiver, Sender};
use async_std::{
    io::BufReader,
    net::{TcpListener, TcpStream},
    prelude::*,
    task,
};
use bigint;
use roles_logic_sv2::{
    common_properties::{IsDownstream, IsMiningDownstream},
    mining_sv2::{ExtendedExtranonce, Extranonce},
    utils::Mutex,
};
use std::{net::SocketAddr, sync::Arc};
use v1::{
    client_to_server, json_rpc, server_to_client,
    utils::{self, HexBytes, HexU32Be},
    IsServer,
};

/// Handles the sending and receiving of messages to and from an SV2 Upstream role (most typically
/// a SV2 Pool server).
#[derive(Debug)]
pub struct Downstream {
    authorized_names: Vec<String>,
    extranonce1: Vec<u8>,
    extranonce2: Vec<u8>,
    //extended_extranonce: Extranonce,
    //extranonce1: HexBytes,
    //extranonce2_size: usize,
    version_rolling_mask: Option<HexU32Be>,
    version_rolling_min_bit: Option<HexU32Be>,
    submit_sender: Sender<(v1::client_to_server::Submit, Vec<u8>)>,
    sender_outgoing: Sender<json_rpc::Message>,
    target: Arc<Mutex<Vec<u8>>>,
    first_job_received: bool,
}

impl Downstream {
    pub async fn new(
        stream: TcpStream,
        submit_sender: Sender<(v1::client_to_server::Submit, Vec<u8>)>,
        mining_notify_receiver: Receiver<server_to_client::Notify>,
        extranonce2_size: usize,
        extranonce: Extranonce,
        last_notify: Arc<Mutex<Option<server_to_client::Notify>>>,
        target: Arc<Mutex<Vec<u8>>>,
    ) -> ProxyResult<Arc<Mutex<Self>>> {
        let stream = std::sync::Arc::new(stream);

        // Reads and writes from Downstream SV1 Mining Device Client
        let (socket_reader, socket_writer) = (stream.clone(), stream);
        let (sender_outgoing, receiver_outgoing) = bounded(10);

        let socket_writer_clone = socket_writer.clone();
        let socket_writer_set_difficulty_clone = socket_writer.clone();
        // Used to send SV1 `mining.notify` messages to the Downstreams
        let socket_writer_notify = socket_writer;

        let extranonce: Vec<u8> = extranonce.try_into().unwrap();
        let (extranonce1, extranonce2) = extranonce.split_at(extranonce.len() - extranonce2_size);

        let downstream = Arc::new(Mutex::new(Downstream {
            authorized_names: vec![],
            extranonce1: extranonce1.to_vec(),
            extranonce2: extranonce2.to_vec(),
            version_rolling_mask: None,
            version_rolling_min_bit: None,
            submit_sender,
            sender_outgoing,
            target: target.clone(),
            first_job_received: false,
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
                    let incoming =
                        incoming.expect("Err reading next incoming message from SV1 Downstream");
                    println!("\nDOWN INCOMING: {:?}", &incoming);
                    let incoming: Result<json_rpc::Message, _> = serde_json::from_str(&incoming);
                    let incoming = incoming.expect("Err serializing incoming message from SV1 Downstream into JSON from `String`");
                    //println!("TD RECV MSG FROM DOWNSTREAM: {:?}", &incoming);
                    // Handle what to do with message
                    Self::handle_incoming_sv1(self_.clone(), incoming).await;
                }
            }
        });

        // Wait for SV1 responses that do not need to go through the Translator, but can be sent
        // back the SV1 Mining Device directly
        task::spawn(async move {
            loop {
                let to_send = receiver_outgoing.recv().await.unwrap();
                // TODO: Use `Error::bad_serde_json`
                let to_send = format!(
                    "{}\n",
                    serde_json::to_string(&to_send)
                        .expect("Err deserializing JSON message for SV1 Downstream into `String`")
                );
                println!("\nDOWN SEND: {:?}", &to_send);
                (&*socket_writer_clone)
                    .write_all(to_send.as_bytes())
                    .await
                    .unwrap();
            }
        });

        let downstream_clone = downstream.clone();
        // RR TODO
        task::spawn(async move {
            let mut first_sent = false;
            loop {
                // Get receiver
                let is_a: bool = downstream_clone
                    .safe_lock(|d| !d.authorized_names.is_empty())
                    .unwrap();

                if is_a && !first_sent {
                    let target = target.safe_lock(|t| t.clone()).unwrap().to_vec();
                    let messsage = Self::get_set_difficulty(target);
                    // let target_2: bigint::U256 = target.safe_lock(|t| t.clone()).unwrap()[..]
                    //     .try_into()
                    //     .unwrap();
                    // let messsage = Self::get_set_difficulty(target_2);
                    Downstream::send_message_downstream(downstream_clone.clone(), messsage).await;

                    let sv1_mining_notify_msg =
                        last_notify.safe_lock(|s| s.clone()).unwrap().unwrap();
                    let messsage: json_rpc::Message = sv1_mining_notify_msg.try_into().unwrap();
                    Downstream::send_message_downstream(downstream_clone.clone(), messsage).await;
                    downstream_clone
                        .clone()
                        .safe_lock(|s| {
                            s.first_job_received = true;
                        })
                        .unwrap();
                    first_sent = true;
                } else if is_a {
                    let sv1_mining_notify_msg =
                        mining_notify_receiver.clone().recv().await.unwrap();
                    let messsage: json_rpc::Message = sv1_mining_notify_msg.try_into().unwrap();
                    Downstream::send_message_downstream(downstream_clone.clone(), messsage).await;
                }
            }
        });
        // Update target
        let downstream_clone = downstream.clone();
        task::spawn(async move {
            let target = downstream_clone.safe_lock(|t| t.target.clone()).unwrap();
            let mut last_target = target.safe_lock(|t| t.clone()).unwrap();
            loop {
                let target = downstream_clone
                    .clone()
                    .safe_lock(|t| t.target.clone())
                    .unwrap();
                let target = target.safe_lock(|t| t.clone()).unwrap();
                if target != last_target {
                    last_target = target;
                    let target_2 = last_target.to_vec();
                    let message = Self::get_set_difficulty(target_2);
                    // let target_2: bigint::U256 = last_target[..].try_into().unwrap();
                    // let message = Self::get_set_difficulty(target_2);
                    Downstream::send_message_downstream(downstream_clone.clone(), message).await;
                }
            }
        });

        Ok(downstream)
    }

    fn difficulty_from_target(target: Vec<u8>) -> f64 {
        // Convert target from Vec<u8> to U256 decimal representation (LE)
        let hex_strs: Vec<String> = target.iter().map(|b| format!("{:02X}", b)).collect();
        let target_hex_str = hex_strs.connect("");
        let target_u256 = bigint::U256::from_little_endian(&target);

        // pdiff: 0x00000000FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF
        // https://en.bitcoin.it/wiki/Difficulty
        let pdiff = bigint::U256::from_dec_str(
            "26959946667150639794667015087019630673637144422540572481103610249215",
        )
        .unwrap();

        let diff = pdiff.overflowing_div(target_u256);
        let diff = diff.0.to_string();
        let diff: f64 = diff.parse().unwrap();
        println!("SET DIFFICULTY TO: {}", diff);
        diff
    }

    fn get_set_difficulty(target: Vec<u8>) -> json_rpc::Message {
        let value = Downstream::difficulty_from_target(target);
        let set_target = v1::methods::server_to_client::SetDifficulty { value };
        let message: json_rpc::Message = set_target.try_into().unwrap();
        message
    }

    /// Accept connections from one or more SV1 Downstream roles (SV1 Mining Devices).
    /// Before creating new Downstream
    /// If we create Downstream + right after the Downstream sends configure, auth, subscribe
    /// Now we have next_mining_notify, when we create Downsteram, we have to spawn a new task that
    /// will take an Arc::Mutex<Downstream> + listen for a new next_mining_notify message via a
    /// channel.
    /// It will be a loop.
    /// When there is a new NextMiningNotify, this loop locks the mutex, changes the field in the
    /// downstream, so we have downstream w updated next_minig_notify field
    /// In the new loop, listen for new NextMiningNotify
    /// We add a field to Downstream called is_authorized: bool. If false, this loop just updates
    /// the NextMiningNotify.  This loop changes the NMN field to prepare for when it is authorized
    /// If True, loop updates field and sends message to downstream.
    /// is_authorized in v1/protocols
    pub async fn accept_connections(
        downstream_addr: SocketAddr,
        submit_sender: Sender<(v1::client_to_server::Submit, Vec<u8>)>,
        receiver_mining_notify: Receiver<server_to_client::Notify>,
        extranonce2_size: usize,
        mut extended_extranonce: ExtendedExtranonce,
        last_notify: Arc<Mutex<Option<server_to_client::Notify>>>,
        target: Arc<Mutex<Vec<u8>>>,
    ) {
        let downstream_listener = TcpListener::bind(downstream_addr).await.unwrap();
        let mut downstream_incoming = downstream_listener.incoming();
        while let Some(stream) = downstream_incoming.next().await {
            let stream = stream.expect("Err on SV1 Downstream connection stream");
            println!(
                "\nPROXY SERVER - ACCEPTING FROM DOWNSTREAM: {}\n",
                stream.peer_addr().unwrap()
            );
            let server = Downstream::new(
                stream,
                submit_sender.clone(),
                receiver_mining_notify.clone(),
                extranonce2_size,
                extended_extranonce.next_extended(extranonce2_size).unwrap(),
                last_notify.clone(),
                target.clone(),
            )
            .await
            .unwrap();
            Arc::new(Mutex::new(server));
        }
    }

    /// As SV1 messages come in, determines if the message response needs to be translated to SV2
    /// and sent to the `Upstream`, or if a direct response can be sent back by the `Translator`
    /// (SV1 and SV2 protocol messages are NOT 1-to-1).
    async fn handle_incoming_sv1(self_: Arc<Mutex<Self>>, message_sv1: json_rpc::Message) {
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
                    Self::send_message_downstream(self_, r.into()).await;
                } else {
                    // If None response is received, indicates this SV1 message received from the
                    // Downstream MD is passed to the `Translator` for translation into SV2
                }
            }
            Err(e) => {
                // Err(Error::V1Error(e))
                panic!("`{:?}`", e);
            }
        }
    }

    /// Send SV1 Response that is generated by `Downstream` (not received by upstream `Translator`)
    /// to be written to the SV1 Downstream Mining Device socket
    async fn send_message_downstream(self_: Arc<Mutex<Self>>, response: json_rpc::Message) {
        //println!("DT SEND SV1 MSG TO DOWNSTREAM: {:?}", &response);
        let sender = self_.safe_lock(|s| s.sender_outgoing.clone()).unwrap();
        sender.send(response).await.unwrap();
    }
}

/// Implements `IsServer` for `Downstream` to handle the SV1 messages.
impl IsServer for Downstream {
    fn handle_configure(
        &mut self,
        _request: &client_to_server::Configure,
    ) -> (Option<server_to_client::VersionRollingParams>, Option<bool>) {
        println!("CONFIGURING DOWNSTREAM");
        self.version_rolling_mask = Some(downstream_sv1::new_version_rolling_mask());
        self.version_rolling_min_bit = Some(downstream_sv1::new_version_rolling_min());
        (
            Some(server_to_client::VersionRollingParams::new(
                self.version_rolling_mask.clone().unwrap(),
                self.version_rolling_min_bit.clone().unwrap(),
            )),
            Some(false),
        )
    }

    /// Handle the response to a `mining.subscribe` message received from the client.
    /// The subscription messages are erroneous and just used to conform the SV1 protocol spec.
    /// Because no one unsubscribed in practice, they just unplug their machine.
    fn handle_subscribe(&self, _request: &client_to_server::Subscribe) -> Vec<(String, String)> {
        println!("SUBSCRIBING DOWNSTREAM");
        let set_difficulty_sub = (
            "mining.set_difficulty".to_string(),
            downstream_sv1::new_subscription_id(),
        );
        let notify_sub = (
            "mining.notify".to_string(),
            "ae6812eb4cd7735a302a8a9dd95cf71f".to_string(),
        );

        vec![set_difficulty_sub, notify_sub]
    }

    fn handle_authorize(&self, _request: &client_to_server::Authorize) -> bool {
        println!("AUTHORIZING DOWNSTREAM");
        true
    }

    fn handle_submit(&self, request: &client_to_server::Submit) -> bool {
        // 1. Check if receiving valid shares by adding diff field to Downstream
        // 2. Have access to &self, use safe_lock to access sender_submit to the Bridge
        // If we need a multiple ref, we can put the channel in a Arc<Mutex<>> or change the
        // IsServer trait handle_submit def
        // Concern that the channel my become full. Max 10 messages. If full, we unwrap and it
        // panics.
        // Can use an unbounded channel.
        // Another reason for a potential panic: The channel would close if the Bridge thread
        // panics.
        if self.first_job_received {
            let to_send = (request.clone(), self.extranonce1.clone());
            self.submit_sender.try_send(to_send).unwrap();
        };
        true
    }

    /// Indicates to the server that the client supports the mining.set_extranonce method.
    fn handle_extranonce_subscribe(&self) {}

    fn is_authorized(&self, name: &str) -> bool {
        self.authorized_names.contains(&name.to_string())
    }

    fn authorize(&mut self, name: &str) {
        self.authorized_names.push(name.to_string());
    }

    /// Set extranonce1 to extranonce1 if provided. If not create a new one and set it.
    fn set_extranonce1(&mut self, _extranonce1: Option<HexBytes>) -> HexBytes {
        self.extranonce1.clone().try_into().unwrap()
    }

    fn extranonce1(&self) -> HexBytes {
        self.extranonce1.clone().try_into().unwrap()
    }

    /// Set extranonce2_size to extranonce2_size provided by the SV2 Upstream in the SV2
    /// `OpenExtendedMiningChannelSuccess` message.
    fn set_extranonce2_size(&mut self, _extra_nonce2_size: Option<usize>) -> usize {
        self.extranonce2.len()
    }

    fn extranonce2_size(&self) -> usize {
        self.extranonce2.len()
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

    fn notify(&mut self) -> Result<json_rpc::Message, v1::error::Error> {
        unreachable!()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gets_difficulty_from_target() {
        let target = vec![
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 128, 255, 127,
            0, 0, 0, 0, 0,
        ];
        let actual = Downstream::difficulty_from_target(target);
        let expect = 512.0;
        assert_eq!(actual, expect);
    }
}
