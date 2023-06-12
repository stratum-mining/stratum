use async_channel::{bounded, Receiver, Sender};
use async_std::net::TcpStream;
use async_std::{
    io::BufReader,
    prelude::*,
    sync::{Arc, Mutex},
    task,
};
use std::convert::TryInto;
use std::time::Duration;
use v1::{
    client_to_server,
    error::Error,
    json_rpc, server_to_client,
    utils::{Extranonce, HexU32Be, MerkleNode, PrevHash},
    IsServer,
};

pub struct Server<'a> {
    authorized_names: Vec<String>,
    extranonce1: Extranonce<'a>,
    extranonce2_size: usize,
    version_rolling_mask: Option<HexU32Be>,
    version_rolling_min_bit: Option<HexU32Be>,
    receiver_incoming: Receiver<String>,
    sender_outgoing: Sender<String>,
}

impl<'a> Server<'a> {
    pub async fn new(stream: TcpStream) -> Arc<Mutex<Server<'static>>> {
        let stream = Arc::new(stream);

        let (reader, writer) = (stream.clone(), stream);

        let (sender_incoming, receiver_incoming) = bounded(10);
        let (sender_outgoing, receiver_outgoing) = bounded(10);

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

        let server = Server {
            authorized_names: vec![],
            extranonce1: extranonce_from_hex("00000000"),
            extranonce2_size: 2,
            version_rolling_mask: None,
            version_rolling_min_bit: None,
            receiver_incoming,
            sender_outgoing,
        };

        let server = Arc::new(Mutex::new(server));

        let cloned = server.clone();
        task::spawn(async move {
            loop {
                if let Some(mut self_) = cloned.try_lock() {
                    let incoming = self_.receiver_incoming.try_recv();
                    self_.parse_message(incoming).await;
                    drop(self_);
                    //It's healthy to sleep after giving up the lock so the other thread has a shot
                    //at acquiring it.
                    task::sleep(Duration::from_millis(100)).await;
                };
            }
        });

        let cloned = server.clone();
        task::spawn(async move {
            // let mut run_time = Self::get_runtime();

            for i in 0..5 {
                // let notify_time = 5;
                if let Some(mut self_) = cloned.try_lock() {
                    let sender = &self_.sender_outgoing.clone();
                    let notify = self_.notify().unwrap();
                    Server::send_message(sender, notify).await;
                    drop(self_);
                    // task::sleep(Duration::from_millis(100)).await;
                };
                if i == 4 {
                    break;
                }
            }
        });

        server
    }

    #[allow(clippy::single_match)]
    async fn parse_message(
        &mut self,
        incoming_message: Result<String, async_channel::TryRecvError>,
    ) {
        if let Ok(line) = incoming_message {
            println!("SERVER - message: {}", line);
            let message: Result<json_rpc::Message, _> = serde_json::from_str(&line);
            match message {
                Ok(message) => {
                    match self.handle_message(message) {
                        Ok(response) => {
                            if response.is_some() {
                                Self::send_message(
                                    &self.sender_outgoing,
                                    json_rpc::Message::OkResponse(response.unwrap()),
                                )
                                .await;
                            }
                        }
                        Err(_) => (),
                    };
                }
                Err(_) => (),
            }
        };
    }

    async fn send_message(sender_outgoing: &Sender<String>, msg: json_rpc::Message) {
        let msg = format!("{}\n", serde_json::to_string(&msg).unwrap());
        sender_outgoing.send(msg).await.unwrap();
    }
}

impl<'a> IsServer<'a> for Server<'a> {
    fn handle_configure(
        &mut self,
        _request: &client_to_server::Configure,
    ) -> (Option<server_to_client::VersionRollingParams>, Option<bool>) {
        self.version_rolling_mask = self
            .version_rolling_mask
            .clone()
            .map_or(Some(new_version_rolling_mask()), Some);
        self.version_rolling_min_bit = self
            .version_rolling_mask
            .clone()
            .map_or(Some(new_version_rolling_min()), Some);
        (
            Some(
                server_to_client::VersionRollingParams::new(
                    self.version_rolling_mask.clone().unwrap(),
                    self.version_rolling_min_bit.clone().unwrap(),
                )
                .unwrap(),
            ),
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
    fn set_extranonce1(&mut self, extranonce1: Option<Extranonce<'a>>) -> Extranonce<'a> {
        self.extranonce1 = extranonce1.unwrap_or_else(new_extranonce);
        self.extranonce1.clone()
    }

    fn extranonce1(&self) -> Extranonce<'a> {
        self.extranonce1.clone()
    }

    /// Set extranonce2_size to extranonce2_size if provided. If not create a new one and set it.
    fn set_extranonce2_size(&mut self, extra_nonce2_size: Option<usize>) -> usize {
        self.extranonce2_size = extra_nonce2_size.unwrap_or_else(new_extranonce2_size);
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

    fn notify(&mut self) -> Result<json_rpc::Message, Error<'a>> {
        let hex = "ffff";
        Ok(server_to_client::Notify {
            job_id: "ciao".to_string(),
            prev_hash: prevhash_from_hex(hex),
            coin_base1: hex.try_into()?,
            coin_base2: hex.try_into()?,
            merkle_branch: vec![merklenode_from_hex(hex)],
            version: HexU32Be(5667),
            bits: HexU32Be(5678),
            time: HexU32Be(5609),
            clean_jobs: true,
        }
        .try_into()?)
    }
}

pub fn new_extranonce<'a>() -> v1::utils::Extranonce<'a> {
    extranonce_from_hex("08000005")
}

fn extranonce_from_hex<'a>(hex: &str) -> Extranonce<'a> {
    let data = utils::decode_hex(hex).unwrap();
    Extranonce::try_from(data).expect("Failed to convert hex to U256")
}

fn merklenode_from_hex<'a>(hex: &str) -> v1::utils::MerkleNode<'a> {
    let data = utils::decode_hex(hex).unwrap();
    let len = data.len();
    if hex.len() >= 64 {
        // panic if hex is larger than 32 bytes
        v1::utils::MerkleNode::try_from(hex).expect("Failed to convert hex to U256")
    } else {
        // prepend hex with zeros so that it is 32 bytes
        let mut new_vec = vec![0_u8; 32 - len];
        new_vec.extend(data.iter());
        MerkleNode::try_from(utils::encode_hex(&new_vec).as_str())
            .expect("Failed to convert hex to U256")
    }
}

pub fn prevhash_from_hex<'a>(hex: &str) -> PrevHash<'a> {
    let data = utils::decode_hex(hex).unwrap();
    let len = data.len();
    if hex.len() >= 64 {
        // panic if hex is larger than 32 bytes
        PrevHash::try_from(hex).expect("Failed to convert hex to U256")
    } else {
        // prepend hex with zeros so that it is 32 bytes
        let mut new_vec = vec![0_u8; 32 - len];
        new_vec.extend(data.iter());
        PrevHash::try_from(utils::encode_hex(&new_vec).as_str())
            .expect("Failed to convert hex to U256")
    }
}

fn new_extranonce2_size() -> usize {
    4
}

fn new_version_rolling_mask() -> HexU32Be {
    HexU32Be(0xffffffff)
}
pub fn new_version_rolling_min() -> HexU32Be {
    HexU32Be(0x00000000)
}

mod utils {
    use std::fmt::Write;

    pub fn decode_hex(s: &str) -> Result<Vec<u8>, core::num::ParseIntError> {
        let s = match s.strip_prefix("0x") {
            Some(s) => s,
            None => s,
        };
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
            .collect()
    }

    pub fn encode_hex(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(bytes.len() * 2);
        for &b in bytes {
            write!(&mut s, "{:02x}", b).unwrap();
        }
        s
    }
}
