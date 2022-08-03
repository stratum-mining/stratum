use async_std::net::TcpStream;
use std::convert::TryInto;

use bitcoin::util::uint::Uint256;

use async_channel::{bounded, Receiver, Sender};

use async_std::{io::BufReader, prelude::*, task};
use roles_logic_sv2::utils::Mutex;
use std::sync::Arc;
use std::time;

use v1::{
    client_to_server,
    error::Error,
    json_rpc, server_to_client,
    utils::{HexBytes, HexU32Be},
    ClientStatus, IsClient,
};

use crate::{job::Job, miner::Miner};
const ADDR: &str = "127.0.0.1:34254";

/// Represents the Mining Device client which is connected to a Upstream node (either a SV1 Pool
/// server or a SV1<->SV2 Translator Proxy server).
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
    pub(crate) async fn new(client_id: u32) {
        let stream = std::sync::Arc::new(TcpStream::connect(ADDR).await.unwrap());
        let (reader, writer) = (stream.clone(), stream);

        let (sender_incoming, receiver_incoming) = bounded(10);
        let (sender_outgoing, receiver_outgoing) = bounded(10);
        let (share_send, share_recv) = bounded(10);

        let miner = Arc::new(Mutex::new(Miner::new(0)));
        // TODO: This is hard coded for the purposes of a demo
        let default_target: Uint256 = Uint256::from_u64(45_u64).unwrap();
        miner.safe_lock(|m| m.new_target(default_target)).unwrap();
        let miner_cloned = miner.clone();

        task::spawn(async move {
            let mut messages = BufReader::new(&*reader).lines();
            while let Some(message) = messages.next().await {
                let message = message.unwrap();
                // println!("{}", message);
                sender_incoming.send(message).await.unwrap();
            }
        });

        task::spawn(async move {
            loop {
                let message: String = receiver_outgoing.recv().await.unwrap();
                (&*writer).write_all(message.as_bytes()).await.unwrap();
            }
        });

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

        client.send_configure().await;

        std::thread::spawn(move || loop {
            if miner_cloned.safe_lock(|m| m.next_share()).unwrap().is_ok() {
                let nonce = miner_cloned.safe_lock(|m| m.header.unwrap().nonce).unwrap();
                let time = miner_cloned.safe_lock(|m| m.header.unwrap().time).unwrap();
                let job_id = miner_cloned.safe_lock(|m| m.job_id).unwrap();
                let version = miner_cloned.safe_lock(|m| m.version).unwrap();
                share_send
                    .try_send((nonce, job_id.unwrap(), version.unwrap(), time))
                    .unwrap();
            }
            miner_cloned
                .safe_lock(|m| m.header.as_mut().map(|h| h.nonce += 1))
                .unwrap();
        });

        task::spawn(async move {
            let recv = share_recv.clone();
            // let sender = share_send.clone();
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
                // sender.send(message).await.unwrap();
                sender_outgoing.send(message).await.unwrap();
            }
        });

        loop {
            let incoming = client.receiver_incoming.try_recv();
            client.parse_message(incoming).await;
        }
    }

    /// Parse SV1 messages received from the Upstream node.
    async fn parse_message(
        &mut self,
        incoming_message: Result<String, async_channel::TryRecvError>,
    ) {
        if let Ok(line) = incoming_message {
            println!("CLIENT {} - message: {}", self.client_id, line);
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

    /// Send SV1 messages to the Upstream node
    async fn send_message(&mut self, msg: json_rpc::Message) {
        let msg = format!("{}\n", serde_json::to_string(&msg).unwrap());
        self.sender_outgoing.send(msg).await.unwrap();
    }

    pub(crate) async fn send_configure(&mut self) {
        let id = time::SystemTime::now()
            .duration_since(time::SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            .to_string();
        let configure = self.configure(id);
        self.send_message(configure).await;
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
