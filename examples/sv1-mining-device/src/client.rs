use async_std::net::TcpStream;
use std::convert::TryInto;

use bitcoin::{
    blockdata::block::BlockHeader,
    hash_types::{BlockHash, TxMerkleNode},
    hashes::{sha256d::Hash as DHash, Hash},
    util::uint::Uint256,
};

use async_channel::{bounded, Receiver, Sender};

use async_std::{io::BufReader, prelude::*, task};
use roles_logic_sv2::utils::Mutex;
use std::sync::Arc;
use std::time;

const ADDR: &str = "127.0.0.1:34254";

use v1::{
    error::Error,
    json_rpc, server_to_client,
    utils::{HexBytes, HexU32Be},
    ClientStatus, IsClient,
};

pub struct Client {
    client_id: u32,
    extranonce1: HexBytes,
    extranonce2_size: usize,
    version_rolling_mask: Option<HexU32Be>,
    version_rolling_min_bit: Option<HexU32Be>,
    pub status: ClientStatus,
    sented_authorize_request: Vec<(String, String)>, // (id, user_name)
    authorized: Vec<String>,
    receiver_incoming: Receiver<String>,
    sender_outgoing: Sender<String>,
    miner: Arc<Mutex<Miner>>,
}

impl Client {
    pub async fn new(client_id: u32) {
        let stream = std::sync::Arc::new(TcpStream::connect(ADDR).await.unwrap());
        let (reader, writer) = (stream.clone(), stream);

        let (sender_incoming, receiver_incoming) = bounded(10);
        let (sender_outgoing, receiver_outgoing) = bounded(10);
        let (share_send, share_recv) = bounded(10);

        let miner = Arc::new(Mutex::new(Miner::new(0)));
        miner.safe_lock(|m| m.new_target(todo!())).unwrap();
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
            loop {
                let (nonce, job_id, version, ntime) = recv.recv().await.unwrap();
                // TODO create the message to send the share
                // TODO use sender_outgoing send message and send the sahre
                todo!()
            }
        });

        loop {
            let incoming = client.receiver_incoming.try_recv();
            client.parse_message(incoming).await;
        }
    }

    async fn parse_message(
        &mut self,
        incoming_message: Result<String, async_channel::TryRecvError>,
    ) {
        if let Ok(line) = incoming_message {
            println!("CIENT {} - message: {}", self.client_id, line);
            let message: json_rpc::Message = serde_json::from_str(&line).unwrap();
            match self.handle_message(message).unwrap() {
                Some(m) => self.send_message(m).await,
                None => (),
            }
        };
    }

    async fn send_message(&mut self, msg: json_rpc::Message) {
        let msg = format!("{}\n", serde_json::to_string(&msg).unwrap());
        self.sender_outgoing.send(msg).await.unwrap();
    }

    pub async fn send_configure(&mut self) {
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

struct Job {
    job_id: u32,
    prev_hash: [u8; 32],
    nbits: u32,
    version: u32,
    merkle_root: [u8; 32],
}

impl From<v1::methods::server_to_client::Notify> for Job {
    fn from(_: v1::methods::server_to_client::Notify) -> Self {
        todo!()
    }
}

#[derive(Debug)]
struct Miner {
    header: Option<BlockHeader>,
    target: Option<Uint256>,
    job_id: Option<u32>,
    version: Option<u32>,
    handicap: u32,
}

impl Miner {
    fn new(handicap: u32) -> Self {
        Self {
            target: None,
            header: None,
            job_id: None,
            version: None,
            handicap,
        }
    }

    fn new_target(&mut self, target: Vec<u8>) {
        self.target = Some(Uint256::from_be_bytes(target.try_into().unwrap()));
    }

    fn new_header(&mut self, new_job: Job) {
        self.job_id = Some(new_job.job_id);
        self.version = Some(new_job.version);
        let prev_hash: [u8; 32] = new_job.prev_hash;
        let prev_hash = DHash::from_inner(prev_hash);
        let merkle_root: [u8; 32] = new_job.merkle_root.to_vec().try_into().unwrap();
        let merkle_root = DHash::from_inner(merkle_root);
        let header = BlockHeader {
            version: new_job.version as i32,
            prev_blockhash: BlockHash::from_hash(prev_hash),
            merkle_root: TxMerkleNode::from_hash(merkle_root),
            time: std::time::SystemTime::now()
                .duration_since(
                    std::time::SystemTime::UNIX_EPOCH - std::time::Duration::from_secs(60),
                )
                .unwrap()
                .as_secs() as u32,
            bits: new_job.nbits,
            nonce: 0,
        };
        self.header = Some(header);
    }
    pub fn next_share(&mut self) -> Result<(), ()> {
        let header = self.header.as_ref().ok_or(())?;
        let mut hash = header.block_hash().as_hash().into_inner();
        hash.reverse();
        let hash = Uint256::from_be_bytes(hash);
        if hash < *self.target.as_ref().ok_or(())? {
            println!(
                "Found share with nonce: {}, for target: {:?}",
                header.nonce, self.target
            );
            Ok(())
        } else {
            Err(())
        }
    }
}
