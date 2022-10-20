<<<<<<< HEAD
use crate::{downstream_sv1, error::ProxyResult};
=======
use crate::{downstream_sv1, ProxyResult};
>>>>>>> e45f4adc1ef1104031c71ef3519f1cfaaff130c3
use async_channel::{bounded, Receiver, Sender};
use async_std::{
    io::BufReader,
    net::{TcpListener, TcpStream},
    prelude::*,
    task,
};
use roles_logic_sv2::{
<<<<<<< HEAD
    common_properties::{IsDownstream, IsMiningDownstream},
    utils::Mutex,
};
use std::{net::SocketAddr, sync::Arc};
use v1::{
    client_to_server, json_rpc, server_to_client,
    utils::{self, HexBytes, HexU32Be},
=======
    bitcoin::util::uint::Uint256,
    common_properties::{IsDownstream, IsMiningDownstream},
    mining_sv2::ExtendedExtranonce,
    utils::Mutex,
};
use std::{net::SocketAddr, ops::Div, sync::Arc};
use v1::{
    client_to_server, json_rpc, server_to_client,
    utils::{HexBytes, HexU32Be},
>>>>>>> e45f4adc1ef1104031c71ef3519f1cfaaff130c3
    IsServer,
};

/// Handles the sending and receiving of messages to and from an SV2 Upstream role (most typically
/// a SV2 Pool server).
#[derive(Debug)]
pub struct Downstream {
<<<<<<< HEAD
    authorized_names: Vec<String>,
    extranonce1: HexBytes,
    extranonce2_size: usize,
    version_rolling_mask: Option<HexU32Be>,
    version_rolling_min_bit: Option<HexU32Be>,
    submit_sender: Sender<v1::client_to_server::Submit>,
    sender_outgoing: Sender<json_rpc::Message>,
}

impl Downstream {
    pub async fn new(
        stream: TcpStream,
        submit_sender: Sender<v1::client_to_server::Submit>,
        mining_notify_receiver: Receiver<server_to_client::Notify>,
=======
    /// List of authorized Downstream Mining Devices.
    authorized_names: Vec<String>,
    extranonce: ExtendedExtranonce,
    /// `extranonce1` to be sent to the Downstream in the SV1 `mining.subscribe` message response.
    //extranonce1: Vec<u8>,
    //extranonce2_size: usize,
    /// Version rolling mask bits
    version_rolling_mask: Option<HexU32Be>,
    /// Minimum version rolling mask bits size
    version_rolling_min_bit: Option<HexU32Be>,
    /// Sends SV1 `mining.submit` message received from the SV1 Downstream to the Bridge for
    /// translation into a SV2 `SubmitSharesExtended`.
    submit_sender: Sender<(v1::client_to_server::Submit, ExtendedExtranonce)>,
    /// Sends message to the SV1 Downstream role.
    sender_outgoing: Sender<json_rpc::Message>,
    /// Difficulty target for SV1 Downstream.
    target: Arc<Mutex<Vec<u8>>>,
    /// True if this is the first job received from `Upstream`.
    first_job_received: bool,
}

impl Downstream {
    /// Instantiate a new `Downstream`.
    pub async fn new(
        stream: TcpStream,
        submit_sender: Sender<(v1::client_to_server::Submit, ExtendedExtranonce)>,
        mining_notify_receiver: Receiver<server_to_client::Notify>,
        extranonce: ExtendedExtranonce,
        last_notify: Arc<Mutex<Option<server_to_client::Notify>>>,
        target: Arc<Mutex<Vec<u8>>>,
>>>>>>> e45f4adc1ef1104031c71ef3519f1cfaaff130c3
    ) -> ProxyResult<Arc<Mutex<Self>>> {
        let stream = std::sync::Arc::new(stream);

        // Reads and writes from Downstream SV1 Mining Device Client
        let (socket_reader, socket_writer) = (stream.clone(), stream);
        let (sender_outgoing, receiver_outgoing) = bounded(10);

        let socket_writer_clone = socket_writer.clone();
<<<<<<< HEAD
        let socket_writer_set_difficulty_clone = socket_writer.clone();
        // Used to send SV1 `mining.notify` messages to the Downstreams
        let socket_writer_notify = socket_writer;

        let downstream = Arc::new(Mutex::new(Downstream {
            authorized_names: vec![],
            extranonce1: "00000000".try_into()?,
            extranonce2_size: 2,
=======
        let _socket_writer_set_difficulty_clone = socket_writer.clone();
        // Used to send SV1 `mining.notify` messages to the Downstreams
        let _socket_writer_notify = socket_writer;

        //let extranonce: Vec<u8> = extranonce.try_into().unwrap();
        //let (extranonce1, _) = extranonce.split_at(extranonce.len() - extranonce2_size);

        let downstream = Arc::new(Mutex::new(Downstream {
            authorized_names: vec![],
            extranonce,
            //extranonce1: extranonce1.to_vec(),
>>>>>>> e45f4adc1ef1104031c71ef3519f1cfaaff130c3
            version_rolling_mask: None,
            version_rolling_min_bit: None,
            submit_sender,
            sender_outgoing,
<<<<<<< HEAD
        }));
        let self_ = downstream.clone();

        // Task to read from SV1 Mining Device Client socket via `socket_reader`. Parses received
        // message as `json_rpc::Message` + sends to upstream `Translator.receiver_for_downstream`
        // via `sender_upstream`
=======
            target: target.clone(),
            first_job_received: false,
        }));
        let self_ = downstream.clone();

        // Task to read from SV1 Mining Device Client socket via `socket_reader`. Depending on the
        // SV1 message received, a message response is sent directly back to the SV1 Downstream
        // role, or the message is sent upwards to the Bridge for translation into a SV2 message
        // and then sent to the SV2 Upstream role.
>>>>>>> e45f4adc1ef1104031c71ef3519f1cfaaff130c3
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
<<<<<<< HEAD
                    let incoming: Result<json_rpc::Message, _> = serde_json::from_str(&incoming);
                    let incoming = incoming.expect("Err serializing incoming message from SV1 Downstream into JSON from `String`");
                    println!("TD RECV MSG FROM DOWNSTREAM: {:?}", &incoming);
=======
                    //println!("\nInfo:: Down: Receiving: {:?}", &incoming);
                    let incoming: Result<json_rpc::Message, _> = serde_json::from_str(&incoming);
                    let incoming = incoming.expect("Err serializing incoming message from SV1 Downstream into JSON from `String`");
>>>>>>> e45f4adc1ef1104031c71ef3519f1cfaaff130c3
                    // Handle what to do with message
                    Self::handle_incoming_sv1(self_.clone(), incoming).await;
                }
            }
        });

<<<<<<< HEAD
        // Wait for SV1 responses that do not need to go through the Translator, but can be sent
        // back the SV1 Mining Device directly
        task::spawn(async move {
            loop {
                let to_send = receiver_outgoing.recv().await.unwrap();
                // TODO: Use `Error::bad_serde_json`
=======
        // Task to receive SV1 message responses to SV1 messages that do NOT need translation.
        // These response messages are sent directly to the SV1 Downstream role.
        task::spawn(async move {
            loop {
                let to_send = receiver_outgoing.recv().await.unwrap();
>>>>>>> e45f4adc1ef1104031c71ef3519f1cfaaff130c3
                let to_send = format!(
                    "{}\n",
                    serde_json::to_string(&to_send)
                        .expect("Err deserializing JSON message for SV1 Downstream into `String`")
                );
<<<<<<< HEAD
=======
                //println!("\nInfo:: Down: Sending: {:?}", &to_send);
>>>>>>> e45f4adc1ef1104031c71ef3519f1cfaaff130c3
                (&*socket_writer_clone)
                    .write_all(to_send.as_bytes())
                    .await
                    .unwrap();
            }
        });

        let downstream_clone = downstream.clone();
<<<<<<< HEAD
        // RR TODO
        task::spawn(async move {
            loop {
                // Get receiver
                let is_a: bool = downstream_clone
                    .safe_lock(|d| d.is_authorized("user"))
                    .unwrap();
                if is_a {
                    let set_difficulty = downstream_clone
                        .safe_lock(|d| {
                            d.handle_set_difficulty(downstream_sv1::new_difficulty())
                                .unwrap()
                        })
                        .unwrap();
                    let to_send = format!(
                        "{}\n",
                        serde_json::to_string(&set_difficulty).expect(
                            "Err deserializing JSON message for SV1 Downstream into `String`"
                        )
                    );
                    (&*socket_writer_set_difficulty_clone)
                        .write_all(to_send.as_bytes())
                        .await
                        .unwrap();

                    let sv1_mining_notify_msg =
                        mining_notify_receiver.clone().recv().await.unwrap();
                    let to_send: json_rpc::Message = sv1_mining_notify_msg.try_into().expect(
                        "Err serializing `Notify` as `json_rpc::Message` for the SV1 Downstream",
                    );
                    let to_send = format!(
                        "{}\n",
                        serde_json::to_string(&to_send).expect(
                            "Err deserializing JSON message for SV1 Downstream into `String`"
                        )
                    );
                    (&*socket_writer_notify)
                        .write_all(to_send.as_bytes())
                        .await
                        .unwrap();
=======
        task::spawn(async move {
            let mut first_sent = false;
            loop {
                // Get receiver
                let is_a: bool = downstream_clone
                    .safe_lock(|d| !d.authorized_names.is_empty())
                    .unwrap();

                if is_a && !first_sent {
                    let target = target.safe_lock(|t| t.clone()).unwrap();
                    let messsage = Self::get_set_difficulty(target);
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

        // Task to update the target and send a new `mining.set_difficulty` to the SV1 Downstream
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
                    Downstream::send_message_downstream(downstream_clone.clone(), message).await;
>>>>>>> e45f4adc1ef1104031c71ef3519f1cfaaff130c3
                }
            }
        });

        Ok(downstream)
    }

<<<<<<< HEAD
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
    pub fn accept_connections(
        downstream_addr: SocketAddr,
        submit_sender: Sender<v1::client_to_server::Submit>,
        receiver_mining_notify: Receiver<server_to_client::Notify>,
    ) {
        task::spawn(async move {
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
                )
                .await
                .unwrap();
                Arc::new(Mutex::new(server));
            }
        });
=======
    /// Helper function to check if target is set to zero for some reason (typically happens when
    /// Downstream role first connects).
    /// https://stackoverflow.com/questions/65367552/checking-a-vecu8-to-see-if-its-all-zero
    fn is_zero(buf: &[u8]) -> bool {
        let (prefix, aligned, suffix) = unsafe { buf.align_to::<u128>() };

        prefix.iter().all(|&x| x == 0)
            && suffix.iter().all(|&x| x == 0)
            && aligned.iter().all(|&x| x == 0)
    }

    /// Converts target received by the `SetTarget` SV2 message from the Upstream role into the
    /// difficulty for the Downstream role sent via the SV1 `mining.set_difficulty` message.
    fn difficulty_from_target(mut target: Vec<u8>) -> f64 {
        target.reverse();
        let target = target.as_slice();

        // If received target is 0, return 0
        if Downstream::is_zero(target) {
            return 0.0;
        }
        let target = Uint256::from_be_slice(target).unwrap();
        let pdiff: [u8; 32] = [
            0, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
        ];
        let pdiff = Uint256::from_be_bytes(pdiff);

        let diff = pdiff.div(target);
        diff.low_u64() as f64
    }

    /// Converts target received by the `SetTarget` SV2 message from the Upstream role into the
    /// difficulty for the Downstream role and creates the SV1 `mining.set_difficulty` message to
    /// be sent to the Downstream role.
    fn get_set_difficulty(target: Vec<u8>) -> json_rpc::Message {
        let value = Downstream::difficulty_from_target(target);
        let set_target = v1::methods::server_to_client::SetDifficulty { value };
        let message: json_rpc::Message = set_target.try_into().unwrap();
        message
    }

    /// Accept connections from one or more SV1 Downstream roles (SV1 Mining Devices) and create a
    /// new `Downstream` for each connection.
    pub async fn accept_connections(
        downstream_addr: SocketAddr,
        submit_sender: Sender<(v1::client_to_server::Submit, ExtendedExtranonce)>,
        receiver_mining_notify: Receiver<server_to_client::Notify>,
        mut extended_extranonce: ExtendedExtranonce,
        last_notify: Arc<Mutex<Option<server_to_client::Notify>>>,
        target: Arc<Mutex<Vec<u8>>>,
    ) {
        let downstream_listener = TcpListener::bind(downstream_addr).await.unwrap();
        let mut downstream_incoming = downstream_listener.incoming();
        while let Some(stream) = downstream_incoming.next().await {
            let stream = stream.expect("Err on SV1 Downstream connection stream");
            extended_extranonce.next_extended(0).unwrap();
            let extended_extranonce = extended_extranonce.clone();
            println!(
                "\nPROXY SERVER - ACCEPTING FROM DOWNSTREAM: {}\n",
                stream.peer_addr().unwrap()
            );
            let server = Downstream::new(
                stream,
                submit_sender.clone(),
                receiver_mining_notify.clone(),
                extended_extranonce,
                last_notify.clone(),
                target.clone(),
            )
            .await
            .unwrap();
            Arc::new(Mutex::new(server));
        }
>>>>>>> e45f4adc1ef1104031c71ef3519f1cfaaff130c3
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
<<<<<<< HEAD
                // Err(Error::V1Error(e))
                panic!(
                    "Error::InvalidJsonRpcMessageKind, sever shouldnt receive json_rpc responsese: `{:?}`",
                    e);
=======
                panic!("`{:?}`", e);
>>>>>>> e45f4adc1ef1104031c71ef3519f1cfaaff130c3
            }
        }
    }

<<<<<<< HEAD
    /// Send SV1 Response that is generated by `Downstream` (not received by upstream `Translator`)
    /// to be written to the SV1 Downstream Mining Device socket
    async fn send_message_downstream(self_: Arc<Mutex<Self>>, response: json_rpc::Message) {
        println!("DT SEND SV1 MSG TO DOWNSTREAM: {:?}", &response);
=======
    /// Send SV1 response message that is generated by `Downstream` (as opposed to being received
    /// by `Bridge`) to be written to the SV1 Downstream role.
    async fn send_message_downstream(self_: Arc<Mutex<Self>>, response: json_rpc::Message) {
>>>>>>> e45f4adc1ef1104031c71ef3519f1cfaaff130c3
        let sender = self_.safe_lock(|s| s.sender_outgoing.clone()).unwrap();
        sender.send(response).await.unwrap();
    }
}

/// Implements `IsServer` for `Downstream` to handle the SV1 messages.
impl IsServer for Downstream {
<<<<<<< HEAD
    fn handle_configure(
        &mut self,
        _request: &client_to_server::Configure,
    ) -> (Option<server_to_client::VersionRollingParams>, Option<bool>) {
        self.version_rolling_mask = self
            .version_rolling_mask
            .clone()
            .map_or(Some(downstream_sv1::new_version_rolling_mask()), Some);
        self.version_rolling_min_bit = self
            .version_rolling_mask
            .clone()
            .map_or(Some(downstream_sv1::new_version_rolling_min()), Some);
=======
    /// Handle the incoming `mining.configure` message which is received after a Downstream role is
    /// subscribed and authorized. Contains the version rolling mask parameters.
    fn handle_configure(
        &mut self,
        request: &client_to_server::Configure,
    ) -> (Option<server_to_client::VersionRollingParams>, Option<bool>) {
        println!("\nInfo:: Down: Configuring");
        println!("Debug:: Down: Handling mining.configure: {:?}", &request);
        self.version_rolling_mask = Some(downstream_sv1::new_version_rolling_mask());
        self.version_rolling_min_bit = Some(downstream_sv1::new_version_rolling_min());
>>>>>>> e45f4adc1ef1104031c71ef3519f1cfaaff130c3
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
<<<<<<< HEAD
    fn handle_subscribe(&self, _request: &client_to_server::Subscribe) -> Vec<(String, String)> {
=======
    fn handle_subscribe(&self, request: &client_to_server::Subscribe) -> Vec<(String, String)> {
        println!("\nInfo:: Down: Subscribing");
        println!("Debug:: Down: Handling mining.subscribe: {:?}", &request);

>>>>>>> e45f4adc1ef1104031c71ef3519f1cfaaff130c3
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

<<<<<<< HEAD
    fn handle_authorize(&self, _request: &client_to_server::Authorize) -> bool {
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
        self.submit_sender.try_send(request.clone()).unwrap();
=======
    /// Any numbers of workers may be authorized at any time during the session. In this way, a
    /// large number of independent Mining Devices can be handled with a single SV1 connection.
    /// https://bitcoin.stackexchange.com/questions/29416/how-do-pool-servers-handle-multiple-workers-sharing-one-connection-with-stratum
    fn handle_authorize(&self, request: &client_to_server::Authorize) -> bool {
        println!("\nInfo:: Down: Authorizing");
        println!("Debug:: Down: Handling mining.authorize: {:?}", &request);
        true
    }

    /// When miner find the job which meets requested difficulty, it can submit share to the server.
    /// Only [Submit](client_to_server::Submit) requests for authorized user names can be submitted.
    fn handle_submit(&self, request: &client_to_server::Submit) -> bool {
        //println!("\nInfo:: Down: Submitting Share");
        //println!("Debug:: Down: Handling mining.submit: {:?}", &request);

        // TODO: Check if receiving valid shares by adding diff field to Downstream

        if self.first_job_received {
            let to_send = (request.clone(), self.extranonce.clone());
            self.submit_sender.try_send(to_send).unwrap();
        };
>>>>>>> e45f4adc1ef1104031c71ef3519f1cfaaff130c3
        true
    }

    /// Indicates to the server that the client supports the mining.set_extranonce method.
    fn handle_extranonce_subscribe(&self) {}

<<<<<<< HEAD
=======
    /// Checks if a Downstream role is authorized.
>>>>>>> e45f4adc1ef1104031c71ef3519f1cfaaff130c3
    fn is_authorized(&self, name: &str) -> bool {
        self.authorized_names.contains(&name.to_string())
    }

<<<<<<< HEAD
    fn authorize(&mut self, name: &str) {
        self.authorized_names.push(name.to_string())
    }

    /// Set extranonce1 to extranonce1 if provided. If not create a new one and set it.
    fn set_extranonce1(&mut self, extranonce1: Option<HexBytes>) -> HexBytes {
        self.extranonce1 = extranonce1.unwrap_or_else(downstream_sv1::new_extranonce);
        self.extranonce1.clone()
    }

    fn extranonce1(&self) -> HexBytes {
        self.extranonce1.clone()
    }

    /// Set extranonce2_size to extranonce2_size if provided. If not create a new one and set it.
    fn set_extranonce2_size(&mut self, extra_nonce2_size: Option<usize>) -> usize {
        self.extranonce2_size =
            extra_nonce2_size.unwrap_or_else(downstream_sv1::new_extranonce2_size);
        self.extranonce2_size
    }

    fn extranonce2_size(&self) -> usize {
        self.extranonce2_size
    }

=======
    /// Authorizes a Downstream role.
    fn authorize(&mut self, name: &str) {
        self.authorized_names.push(name.to_string());
    }

    /// Sets the `extranonce1` field sent in the SV1 `mining.notify` message to the value specified
    /// by the SV2 `OpenExtendedMiningChannelSuccess` message sent from the Upstream role.
    fn set_extranonce1(&mut self, _extranonce1: Option<HexBytes>) -> HexBytes {
        let extranonce1: Vec<u8> = self.extranonce.upstream_part().try_into().unwrap();
        extranonce1.try_into().unwrap()
    }

    /// Returns the `Downstream`'s `extranonce1` value.
    fn extranonce1(&self) -> HexBytes {
        let downstream_ext: Vec<u8> = self
            .extranonce
            .without_upstream_part(None)
            .unwrap()
            .try_into()
            .unwrap();
        downstream_ext.try_into().unwrap()
    }

    /// Sets the `extranonce2_size` field sent in the SV1 `mining.notify` message to the value
    /// specified by the SV2 `OpenExtendedMiningChannelSuccess` message sent from the Upstream role.
    fn set_extranonce2_size(&mut self, _extra_nonce2_size: Option<usize>) -> usize {
        self.extranonce.get_range2_len()
    }

    /// Returns the `Downstream`'s `extranonce2_size` value.
    fn extranonce2_size(&self) -> usize {
        self.extranonce.get_range2_len()
    }

    /// Returns the version rolling mask.
>>>>>>> e45f4adc1ef1104031c71ef3519f1cfaaff130c3
    fn version_rolling_mask(&self) -> Option<HexU32Be> {
        self.version_rolling_mask.clone()
    }

<<<<<<< HEAD
=======
    /// Sets the version rolling mask.
>>>>>>> e45f4adc1ef1104031c71ef3519f1cfaaff130c3
    fn set_version_rolling_mask(&mut self, mask: Option<HexU32Be>) {
        self.version_rolling_mask = mask;
    }

<<<<<<< HEAD
=======
    /// Sets the minimum version rolling bit.
>>>>>>> e45f4adc1ef1104031c71ef3519f1cfaaff130c3
    fn set_version_rolling_min_bit(&mut self, mask: Option<HexU32Be>) {
        self.version_rolling_min_bit = mask
    }

    fn notify(&mut self) -> Result<json_rpc::Message, v1::error::Error> {
<<<<<<< HEAD
        server_to_client::Notify {
            job_id: "deadbeef".to_string(),
            prev_hash: utils::PrevHash(vec![3_u8, 4, 5, 6]),
            coin_base1: "ffff".try_into()?,
            coin_base2: "ffff".try_into()?,
            merkle_branch: vec!["fff".try_into()?],
            version: utils::HexU32Be(5667),
            bits: utils::HexU32Be(5678),
            time: utils::HexU32Be(5609),
            clean_jobs: true,
        }
        .try_into()
=======
        unreachable!()
>>>>>>> e45f4adc1ef1104031c71ef3519f1cfaaff130c3
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
<<<<<<< HEAD
=======

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gets_difficulty_from_target() {
        let target = vec![
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 128, 255, 127,
            0, 0, 0, 0, 0,
        ];
        println!("HERE");
        let actual = Downstream::difficulty_from_target(target);
        let expect = 512.0;
        assert_eq!(actual, expect);
    }
}
>>>>>>> e45f4adc1ef1104031c71ef3519f1cfaaff130c3
