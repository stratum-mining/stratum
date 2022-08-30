use async_std::net::TcpStream;
use std::convert::TryInto;

use bitcoin::util::uint::Uint256;

use async_channel::{bounded, Receiver, Sender};

use async_std::{io::BufReader, prelude::*, task};
use roles_logic_sv2::utils::Mutex;
use std::{sync::Arc, time};

use v1::{
    client_to_server,
    error::Error,
    json_rpc, server_to_client,
    utils::{HexBytes, HexU32Be},
    ClientStatus, IsClient,
};

use crate::{job::Job, miner::Miner};
const ADDR: &str = "127.0.0.1:34255";

/// Represents the Mining Device client which is connected to a Upstream node (either a SV1 Pool
/// server or a SV1 <-> SV2 Translator Proxy server).
pub(crate) struct Client {
    client_id: u32,
    extranonce1: HexBytes,
    extranonce2_size: usize,
    version_rolling_mask: Option<HexU32Be>,
    version_rolling_min_bit: Option<HexU32Be>,
    pub(crate) status: ClientStatus,
    sented_authorize_request: Vec<(String, String)>, // (id, user_name)
    authorized: Vec<String>,
    /// Receives incoming messages from the SV1 Upstream node.
    receiver_incoming: Receiver<String>,
    /// Sends outgoing messages to the SV1 Upstream node.
    sender_outgoing: Sender<String>,
    /// Representation of the Mining Devices
    miner: Arc<Mutex<Miner>>,
}

impl Client {
    /// Outgoing channels are used to send messages to the Upstream
    /// Incoming channels are used to receive messages from the Upstream
    /// There are three separate channels, the first two are responsible for receiving and sending
    /// messages to the Upstream, and the third is responsible for pass valid job submissions to
    /// the first set of channels:
    /// 1. `(sender_incoming, receiver_incoming)`:
    ///     `sender_incoming` listens on the socket where messages are being sent from the Upstream
    ///     node. From the socket, it reads the incoming bytes from the Upstream into a
    ///     `BufReader`. The incoming bytes represent a message from the Upstream, and each new
    ///     line is a new message. When it gets this line (a message) from the Upstream, it sends
    ///     them to the `receiver_incoming` which is listening in a loop. The message line received
    ///     by the `receiver_incoming` are then parsed by the `Client` in the `parse_message`
    ///     method to be handled.
    /// 2. `(sender_outgoing, receiver_outgoing)`:
    ///    When the `parse_message` method on the `Client` is called, it handles the message and
    ///    formats the a new message to be sent to the Upstream in response. It sends the response
    ///    message via the `sender_outgoing` to the `receiver_outgoing` which is waiting to receive
    ///    a message in its own task. When the `receiver_outgoing` receives the response message
    ///    from the the `sender_outgoing`, it writes this message to the socket connected to the
    ///    Upstream via `write_all`.
    /// 3. `(sender_share, receiver_share)`:
    ///    A new thread is spawned to mock the act of a Miner hashing over a candidate block
    ///    without blocking the rest of the program. Since this in its own thread, we need a
    ///    channel to communicate with it, which is `(sender_share, receiver_share)`. In this thread, on
    ///    each new share, `sender_share` sends the pertinent information to create a `mining.submit`
    ///    message to the `receiver_share` that is waiting to receive this information in a separate
    ///    task. In this task, once `receiver_share` gets the information from `sender_share`, it is
    ///    formatted as a `v1::client_to_server::Submit` and then serialized into a json message
    ///    that is sent to the Upstream via `sender_outgoing`.
    pub(crate) async fn new(client_id: u32) {
        let stream = std::sync::Arc::new(TcpStream::connect(ADDR).await.unwrap());
        let (reader, writer) = (stream.clone(), stream);

        // `sender_incoming` listens on socket for incoming messages from the Upstream and sends
        // messages to the `receiver_incoming` to be parsed and handled by the `Client`
        let (sender_incoming, receiver_incoming) = bounded(10);
        // `sender_outgoing` sends the message parsed by the `Client` to the `receiver_outgoing`
        // which writes the messages to the socket to the Upstream
        let (sender_outgoing, receiver_outgoing) = bounded(10);
        // `sender_share` sends job share results to the `receiver_share` where the job share results are
        // formated into a "mining.submit" messages that is then sent to the Upstream via
        // `sender_outgoing`
        let (sender_share, receiver_share) = bounded(10);

        // Instantiates a new `Miner` (a mock of an actual Mining Device) with a job id of 0.
        let miner = Arc::new(Mutex::new(Miner::new(0)));

        // Sets an initial target for the `Miner`.
        // TODO: This is hard coded for the purposes of a demo, should be set by the SV1
        // `mining.set_difficulty` message received from the Upstream role
        let default_target: Uint256 = Uint256::from_u64(45_u64).unwrap();
        miner.safe_lock(|m| m.new_target(default_target)).unwrap();

        let miner_cloned = miner.clone();

        // Reads messages sent by the Upstream from the socket to be passed to the
        // `receiver_incoming`
        task::spawn(async move {
            let mut messages = BufReader::new(&*reader).lines();
            while let Some(message) = messages.next().await {
                let message = message.unwrap();
                sender_incoming.send(message).await.unwrap();
            }
        });

        // Waits to receive a message from `sender_outgoing` and writes it to the socket for the
        // Upstream to receive
        task::spawn(async move {
            loop {
                let message: String = receiver_outgoing.recv().await.unwrap();
                (&*writer).write_all(message.as_bytes()).await.unwrap();
            }
        });

        // Clone the sender to the Upstream node to use it in another task below as
        // `sender_outgoing` is consumed by the initialization of `Client`
        let sender_outgoing_clone = sender_outgoing.clone();

        // Initialize Client
        let mut client = Client {
            client_id,
            extranonce1: "00000000".try_into().unwrap(),
            extranonce2_size: 2,
            version_rolling_mask: None,
            version_rolling_min_bit: None,
            status: ClientStatus::Init,
            sented_authorize_request: vec![],
            authorized: vec![],
            receiver_incoming,
            sender_outgoing,
            miner,
        };

        //let line = client.receiver_incoming.recv().await.unwrap();
        //println!("CLIENT {} - Received: {}", client_id, line);
        //let message: json_rpc::Message = serde_json::from_str(&line).unwrap();
        //match client.handle_message(message).unwrap() {
        //    Some(m) => {
        //        if m.is_subscribe() {
        //            client.send_message(m).await;
        //        } else {
        //           panic!("unexpected response from upstream");
        //        }
        //    }
        //    None => panic!("unexpected response from upstream"),
        //};
        //task::spawn(async move {
        //    client.send_authorize().await;
        //});

        // Gets the latest candidate block header hash from the `Miner` by calling the `next_share`
        // method. Mocks the act of the `Miner` incrementing the nonce. Performs this in a loop,
        // incrementing the nonce each time, to mimic a Mining Device generating continuous hashes.
        // For each generated block header, sends to the `receiver_share` the relevant values that
        // generated the candidate block header needed to then format and send as a "mining.submit"
        // message to the Upstream node.
        // Is a separate thread as it can be CPU intensive and we do not want to block the reading
        // and writing of messages to the socket.
        std::thread::spawn(move || loop {
            if miner_cloned.safe_lock(|m| m.next_share()).unwrap().is_ok() {
                let nonce = miner_cloned.safe_lock(|m| m.header.unwrap().nonce).unwrap();
                let time = miner_cloned.safe_lock(|m| m.header.unwrap().time).unwrap();
                let job_id = miner_cloned.safe_lock(|m| m.job_id).unwrap();
                let version = miner_cloned.safe_lock(|m| m.version).unwrap();
                // Sends relevant candidate block header values needed to construct a
                // `mining.submit` message to the `receiver_share` in the task that is responsible for
                // sending messages to the Upstream node.
                sender_share
                    .try_send((nonce, job_id.unwrap(), version.unwrap(), time))
                    .unwrap();
            }
            miner_cloned
                .safe_lock(|m| m.header.as_mut().map(|h| h.nonce += 1))
                .unwrap();
        });

        // Task to receive relevant candidate block header values needed to construct a
        // `mining.submit` message. This message is contructed as a `client_to_server::Submit` and
        // then serialized into json to be sent to the Upstream via the `sender_outgoing` sender.
        task::spawn(async move {
            let recv = receiver_share.clone();
            loop {
                let (nonce, job_id, version, ntime) = recv.recv().await.unwrap();
                let extra_nonce2: HexBytes = "0x0000000000000000".try_into().unwrap();
                let version = Some(HexU32Be(version));
                let submit = client_to_server::Submit {
                    id: "TODO: ID".into(),
                    user_name: "TODO: USER NAME".into(),
                    job_id: "TODO: job_id as String".into(),
                    extra_nonce2,
                    time: ntime.into(),
                    nonce: nonce.into(),
                    version_bits: version,
                };
                let message: json_rpc::Message = submit.into();
                let message = format!("{}\n", serde_json::to_string(&message).unwrap());
                sender_outgoing_clone.send(message).await.unwrap();
            }
        });

        // configure subscribe and authorize
        client.send_configure().await;
        loop {
            match client.status {
                ClientStatus::Init => panic!("impossible state"),
                ClientStatus::Configured => {
                    let incoming = client.receiver_incoming.recv().await.unwrap();
                    client.parse_message(Ok(incoming)).await;
                }
                ClientStatus::Subscribed => {
                    client.send_authorize().await;
                    break;
                }
            }
        }
        // Waits for the `sender_incoming` to get message line from socket to be parsed by the
        // `Client`
        loop {
            let incoming = client.receiver_incoming.recv().await.unwrap();
            client.parse_message(Ok(incoming)).await;
        }
    }

    /// Parse SV1 messages received from the Upstream node.
    async fn parse_message(
        &mut self,
        incoming_message: Result<String, async_channel::TryRecvError>,
    ) {
        // If we have a line (1 line represents 1 sv1 incoming message), then handle that message
        if let Ok(line) = incoming_message {
            println!("CLIENT {} - Received: {}", self.client_id, line);
            let message: json_rpc::Message = serde_json::from_str(&line).unwrap();
            // If has a message, it sends it back
            match self.handle_message(message).unwrap() {
                Some(m) => {
                    self.send_message(m).await;
                }
                None => (),
            }
        };
    }

    /// Send SV1 messages to the receiver_outgoing which writes to the socket (aka Upstream node)
    async fn send_message(&mut self, msg: json_rpc::Message) {
        let msg = format!("{}\n", serde_json::to_string(&msg).unwrap());
        println!("CLIENT {} - Send: {}", self.client_id, &msg);
        self.sender_outgoing.send(msg).await.unwrap();
    }

    pub(crate) async fn send_configure(&mut self) {
        // This loop is probably unnecessary as the first state is `Init`
        loop {
            if let ClientStatus::Init = self.status {
                break;
            }
        }
        let id = time::SystemTime::now()
            .duration_since(time::SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            .to_string();
        let configure = self.configure(id);
        self.send_message(configure).await;
        // Update status as configured
        self.status = ClientStatus::Configured;
    }

    pub async fn send_authorize(&mut self) {
        let id = time::SystemTime::now()
            .duration_since(time::SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            .to_string();
        let authorize = self
            .authorize(id.clone(), "user".to_string(), "password".to_string())
            .unwrap();
        self.sented_authorize_request.push((id, "user".to_string()));
        self.send_message(authorize).await;
    }
}

impl IsClient for Client {
    /// Updates miner with new job
    fn handle_notify(&mut self, notify: server_to_client::Notify) -> Result<(), Error> {
        let new_job: Job = notify.into();
        self.miner.safe_lock(|m| m.new_header(new_job)).unwrap();
        Ok(())
    }

    fn handle_configure(&self, _conf: &mut server_to_client::Configure) -> Result<(), Error> {
        Ok(())
    }

    fn handle_subscribe(&mut self, _subscribe: &server_to_client::Subscribe) -> Result<(), Error> {
        Ok(())
    }

    fn set_extranonce1(&mut self, extranonce1: HexBytes) {
        self.extranonce1 = extranonce1;
    }

    fn extranonce1(&self) -> HexBytes {
        self.extranonce1.clone()
    }

    fn set_extranonce2_size(&mut self, extra_nonce2_size: usize) {
        self.extranonce2_size = extra_nonce2_size;
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

    fn set_version_rolling_min_bit(&mut self, min: Option<HexU32Be>) {
        self.version_rolling_min_bit = min;
    }

    fn set_status(&mut self, status: ClientStatus) {
        self.status = status;
    }

    fn signature(&self) -> String {
        format!("{}", self.client_id)
    }

    fn status(&self) -> ClientStatus {
        self.status
    }

    fn version_rolling_min_bit(&mut self) -> Option<HexU32Be> {
        self.version_rolling_min_bit.clone()
    }

    fn id_is_authorize(&mut self, id: &str) -> Option<String> {
        let req: Vec<&(String, String)> = self
            .sented_authorize_request
            .iter()
            .filter(|x| x.0 == id)
            .collect();
        match req.len() {
            0 => None,
            _ => Some(req[0].1.clone()),
        }
    }

    fn id_is_submit(&mut self, _: &str) -> bool {
        false
    }

    fn authorize_user_name(&mut self, name: String) {
        self.authorized.push(name)
    }

    fn is_authorized(&self, name: &String) -> bool {
        self.authorized.contains(name)
    }

    fn last_notify(&self) -> Option<server_to_client::Notify> {
        None
    }

    fn handle_error_message(
        &mut self,
        message: v1::Message,
    ) -> Result<Option<json_rpc::Message>, Error> {
        println!("{:?}", message);
        Ok(None)
    }
}

/// Represents a new outgoing `mining.submit` solution submission to be sent to the Upstream
/// server.
struct Submit {
    // worker_name: String,
    /// ID of the job used while submitting share generated from this job.
    /// TODO: Currently is `u32` and is hardcoded, but should be String and set by the incoming
    /// `mining.notify` message.
    job_id: u32,
    // /// TODO: Hard coded for demo
    // extranonce_2: u32,
    /// Current time
    ntime: u32,
    /// Nonce
    /// TODO: Hard coded for the demo
    nonce: u32,
}
