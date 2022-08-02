use async_std::net::{TcpListener, TcpStream};
use std::convert::TryInto;

use async_channel::{bounded, Receiver, Sender};
use async_std::{
    io::BufReader,
    prelude::*,
    sync::{Arc, Mutex},
    task,
};
use std::time;
use v1::{
    client_to_server,
    error::Error,
    json_rpc, server_to_client,
    utils::{self, HexBytes, HexU32Be},
    ClientStatus, IsClient, IsServer,
};

const ADDR: &str = "127.0.0.1:34254";

#[async_std::main]
async fn main() {
    task::spawn(async {
        proxy_server().await;
    }).await;
}

async fn proxy_server() {
    let listner = TcpListener::bind(ADDR).await.unwrap();
    let mut incoming = listner.incoming();
    while let Some(stream) = incoming.next().await {
        let stream = stream.unwrap();
        println!("SERVER - Accepting from: {}", stream.peer_addr().unwrap());
        let server = Proxy::new(stream).await;
        Arc::new(Mutex::new(server));
    }
}

struct Proxy {
    receiver_incoming: Receiver<String>,
    sender_outgoing: Sender<String>,
}

impl Proxy {
    pub async fn new(stream: TcpStream) -> Arc<Mutex<Self>> {
        let stream = std::sync::Arc::new(stream);

        let (reader, writer) = (stream.clone(), stream);

        let (sender_incoming, receiver_incoming) = bounded(10);
        let (sender_outgoing, receiver_outgoing) = bounded(10);

        task::spawn(async move {
            let mut messages = BufReader::new(&*reader).lines();
            while let Some(message) = messages.next().await {
                let message = message.unwrap();
                println!("{}", message);
                sender_incoming.send(message).await.unwrap();
            }
        });

        task::spawn(async move {
            loop {
                let message: String = receiver_outgoing.recv().await.unwrap();
                (&*writer).write_all(message.as_bytes()).await.unwrap();
            }
        });

        let proxy = Proxy {
            receiver_incoming,
            sender_outgoing,
        };

        let proxy = Arc::new(Mutex::new(proxy));
        let cloned = proxy.clone();

        task::spawn(async move {
            loop {
                if let Some(mut self_) = cloned.try_lock() {
                    let incoming = self_.receiver_incoming.try_recv();
                    self_.parse_message(incoming).await;
                    drop(self_);
                };
            }
        });

        let cloned = proxy.clone();
        task::spawn(async move {
            loop {
                if let Some(mut self_) = cloned.try_lock() {
                    let outgoing = self_.sender_outgoing.send(String::from("hello")).await;
                    // self_.send_notify().await;
                    // drop(self_);
                    task::sleep(time::Duration::from_secs(5)).await;
                };
            }
        });

        proxy
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
                    self.send_message().await;
                }
                Err(_) => (),
            }
        };
    }

    async fn send_message(&mut self) {
        // let msg = format!("{}\n", serde_json::to_string(&msg).unwrap());
        let msg = String::from("HI");
        self.sender_outgoing.send(msg).await.unwrap();
    }
}
