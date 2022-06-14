use async_channel::{bounded, Receiver, Sender};
use async_std::net::{TcpListener, TcpStream};
use async_std::{
    io::BufReader,
    prelude::*,
    sync::{Arc, Mutex},
    task,
};

use std::time;
use v1::{
    client_to_server, json_rpc, server_to_client,
    utils::{HexBytes, HexU32Be},
    IsServer,
};

struct Server {
    authorized_names: Vec<String>,
    extranonce1: HexBytes,
    extranonce2_size: usize,
    version_rolling_mask: Option<HexU32Be>,
    version_rolling_min_bit: Option<HexU32Be>,
    receiver_incoming: Receiver<String>,
    sender_outgoing: Sender<String>,
}

const ADDR: &str = "127.0.0.1:34254";

fn new_extranonce() -> HexBytes {
    "08000002".try_into().unwrap()
}

fn new_extranonce2_size() -> usize {
    4
}

fn new_version_rolling_mask() -> HexU32Be {
    HexU32Be(0xffffffff)
}
fn new_version_rolling_min() -> HexU32Be {
    HexU32Be(0x00000000)
}

pub(crate) async fn server_pool() {
    let listner = TcpListener::bind(ADDR).await.unwrap();
    let mut incoming = listner.incoming();
    while let Some(stream) = incoming.next().await {
        let stream = stream.unwrap();
        println!("SERVER - Accepting from: {}", stream.peer_addr().unwrap());
        let server = Server::new(stream).await;
        Arc::new(Mutex::new(server));
    }
}

impl Server {
    pub async fn new(stream: TcpStream) -> Arc<Mutex<Self>> {
        let stream = std::sync::Arc::new(stream);

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
            extranonce1: "00000000".try_into().unwrap(),
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
                };
            }
        });

        let cloned = server.clone();
        task::spawn(async move {
            loop {
                if let Some(mut self_) = cloned.try_lock() {
                    self_.send_notify().await;
                    drop(self_);
                    task::sleep(time::Duration::from_secs(5)).await;
                };
            }
        });

        server
    }

    async fn parse_message(
        &mut self,
        incoming_message: Result<String, async_channel::TryRecvError>,
    ) {
        if let Ok(line) = incoming_message {
            println!("SERVER - message: {}", line);
            let message: json_rpc::Message = serde_json::from_str(&line).unwrap();
            let response = self.handle_message(message).unwrap();
            if response.is_some() {
                self.send_message(json_rpc::Message::Response(response.unwrap()))
                    .await;
            }
        };
    }

    async fn send_message(&mut self, msg: json_rpc::Message) {
        let msg = format!("{}\n", serde_json::to_string(&msg).unwrap());
        self.sender_outgoing.send(msg).await.unwrap();
    }

    async fn send_notify(&mut self) {
        let notify = self.notify().unwrap();
        self.send_message(notify).await;
    }
}

impl IsServer for Server {
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
        self.extranonce1 = extranonce1.unwrap_or_else(new_extranonce);
        self.extranonce1.clone()
    }

    fn extranonce1(&self) -> HexBytes {
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
}
