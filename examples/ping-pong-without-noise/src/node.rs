use crate::messages::{Message, Ping, Pong,BiggerPing};
use binary_sv2::{from_bytes, U256};
use rand::Rng;

use async_channel::{bounded, Receiver, Sender};
use async_std::{
    net::TcpStream,
    prelude::*,
    sync::{Arc, Mutex},
    task,
};

use codec_sv2::{StandardDecoder, StandardSv2Frame};

#[derive(Debug)]
enum Expected {
    Ping,
    Pong,
}

#[derive(Debug)]
pub struct Node {
    name: String,
    last_id: u32,
    #[allow(dead_code)]
    connection: Arc<Mutex<Connection>>,
    expected: Expected,
    receiver: Receiver<StandardSv2Frame<Message<'static>>>,
    sender: Sender<StandardSv2Frame<Message<'static>>>,
}

impl Node {
    pub fn new(name: String, socket: TcpStream, test_count: u32) -> Arc<Mutex<Self>> {
        let (connection, receiver, sender) = Connection::new(socket);

        let node = Arc::new(Mutex::new(Node {
            last_id: 0,
            name,
            connection,
            expected: Expected::Ping,
            receiver,
            sender,
        }));
        let cloned = node.clone();

        task::spawn(async move {
            loop {
                task::sleep(time::Duration::from_millis(500)).await;
                //This lock is sharing access with the client lock in main.rs::new_client
                if let Some(mut node) = cloned.try_lock() {
                    if node.last_id > test_count {
                        node.sender.close();
                        node.receiver.close();
                        println!("Test Successful");
                        std::process::exit(0);
                    } else if !node.receiver.is_empty() {
                        let incoming = node.receiver.recv().await.unwrap();
                        node.respond(incoming).await;
                    }
                }
            }
        });

        node
    }

    pub async fn send_ping(&mut self) {
        self.expected = Expected::Pong;
        let message = Message::BiggerPing(BiggerPing::new(self.last_id));
        let frame =
            StandardSv2Frame::<Message<'static>>::from_message(message, 0, 0, false).unwrap();
        self.sender.send(frame).await.unwrap();
        self.last_id += 1;
    }

    async fn respond(&mut self, frame: StandardSv2Frame<Message<'static>>) {
        let response = self.handle_message(frame);
        let frame =
            StandardSv2Frame::<Message<'static>>::from_message(response, 0, 0, false).unwrap();
        self.sender.send(frame).await.unwrap();
        self.last_id += 1;
    }

    fn handle_message(
        &mut self,
        mut frame: StandardSv2Frame<Message<'static>>,
    ) -> Message<'static> {
        match self.expected {
            Expected::Ping => {
                let ping: Result<BiggerPing, _> = from_bytes(frame.payload());
                match ping.as_ref() {
                    Ok(ping) => {
                        println!("Node {} received:", self.name);
                        println!("{:#?}\n", ping);
                        let mut seq: Vec<U256> = vec![];
                        for _ in 0..100 {
                            let random_bytes = rand::thread_rng().gen::<[u8; 32]>();
                            let u256: U256 = random_bytes.into();
                            seq.push(u256);
                        }
                        Message::Pong(Pong::new(self.last_id, seq))
                    }
                    Err(error) => {
                        let error = error.clone();
                        drop(ping);
                        let ping: Result<Ping, _> = from_bytes(frame.payload());
                        match ping {
                            Ok(ping) => {
                                println!("Node {} received:", self.name);
                                println!("{:#?}\n", ping);
                                let mut seq: Vec<U256> = vec![];
                                for _ in 0..100 {
                                    let random_bytes = rand::thread_rng().gen::<[u8; 32]>();
                                    let u256: U256 = random_bytes.into();
                                    seq.push(u256);
                                }
                                Message::Pong(Pong::new(self.last_id, seq))
                            }
                            Err(e) => {
                                println!("Error parsing BiggerPing {:#?}", error);
                                println!("Error parsing Ping {:#?}", e);
                                todo!()
                            }
                        }
                    }
                }
            }
            Expected::Pong => {
                let pong: Result<Pong, _> = from_bytes(frame.payload());
                match pong {
                    Ok(pong) => {
                        println!("Node {} received:", self.name);
                        println!("Pong, id: {:#?}\n", pong.get_id());
                        Message::Ping(Ping::new(self.last_id))
                    }
                    Err(_) => panic!(),
                }
            }
        }
    }
}

#[derive(Debug)]
struct Connection {}

use std::time;
impl Connection {
    #[allow(clippy::type_complexity)]
    fn new(
        stream: TcpStream,
    ) -> (
        Arc<Mutex<Self>>,
        Receiver<StandardSv2Frame<Message<'static>>>,
        Sender<StandardSv2Frame<Message<'static>>>,
    ) {
        let (mut reader, writer) = (stream.clone(), stream);

        let (sender_incoming, receiver_incoming): (
            Sender<StandardSv2Frame<Message<'static>>>,
            Receiver<StandardSv2Frame<Message<'static>>>,
        ) = bounded(10);
        let (sender_outgoing, receiver_outgoing): (
            Sender<StandardSv2Frame<Message<'static>>>,
            Receiver<StandardSv2Frame<Message<'static>>>,
        ) = bounded(10);

        // Receive and parse incoming messages from TCP stream
        task::spawn(async move {
            let mut decoder = StandardDecoder::new();

            loop {
                let writable = decoder.writable();
                reader.read_exact(writable).await.unwrap();
                if let Ok(x) = decoder.next_frame() {
                    sender_incoming.send(x).await.unwrap();
                }
                task::sleep(time::Duration::from_millis(500)).await;
            }
        });

        // Encode and send incoming messages to TCP stream
        task::spawn(async move {
            let mut encoder = codec_sv2::Encoder::<Message>::new();

            loop {
                if let Ok(frame) = receiver_outgoing.recv().await {
                    let b = encoder.encode(frame).unwrap();
                    (&writer).write_all(b).await.unwrap();
                }
            }
        });

        let connection = Arc::new(Mutex::new(Self {}));

        (connection, receiver_incoming, sender_outgoing)
    }
}
