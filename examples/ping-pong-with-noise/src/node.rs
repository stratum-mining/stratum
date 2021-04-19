use crate::messages::{Message, Ping, Pong};
use rand::Rng;
use serde_sv2::{from_bytes, to_bytes, GetLen, U256};

use async_channel::{bounded, Receiver, Sender};
use async_std::net::TcpStream;
use async_std::prelude::*;
use async_std::sync::{Arc, Mutex};
use async_std::task;
use core::convert::TryInto;

use codec_sv2::{Frame, HandShakeFrame, NoiseFrame, Sv2Frame};
use codec_sv2::{
    HandshakeRole, Initiator, Responder, StandardEitherFrame, StandardNoiseDecoder,
    StandardSv2Frame, Step,
};

use std::time;

use network_helpers::Connection;

#[derive(Debug)]
enum Expected {
    Ping,
    Pong,
}

#[derive(Debug)]
pub struct Node {
    name: String,
    last_id: u32,
    expected: Expected,
    receiver: Receiver<StandardEitherFrame<Message<'static>>>,
    sender: Sender<StandardEitherFrame<Message<'static>>>,
}

impl Node {
    pub async fn new(name: String, socket: TcpStream, role: HandshakeRole) -> Arc<Mutex<Self>> {
        let (receiver, sender) = Connection::new(socket, role).await;

        let node = Arc::new(Mutex::new(Node {
            last_id: 0,
            name,
            expected: Expected::Ping,
            receiver,
            sender,
        }));
        let cloned = node.clone();

        task::spawn(async move {
            loop {
                task::sleep(time::Duration::from_millis(500)).await;
                match cloned.try_lock() {
                    Some(mut node) => {
                        let incoming: StandardSv2Frame<Message<'static>> =
                            node.receiver.recv().await.unwrap().try_into().unwrap();
                        node.respond(incoming).await;
                    }
                    _ => (),
                }
            }
        });

        node
    }

    pub async fn send_ping(&mut self) {
        self.expected = Expected::Pong;
        let message = Message::Ping(Ping::new(self.last_id.clone()));
        let frame = StandardSv2Frame::<Message<'static>>::from_message(message).unwrap();
        self.sender.send(frame.into()).await.unwrap();
        self.last_id += 1;
    }
    pub async fn send_pong(&mut self) {
        self.expected = Expected::Ping;
        let message = Message::Ping(Ping::new(self.last_id.clone()));
        let mut seq: Vec<U256> = vec![];
        for _ in 0..10 {
            let random_bytes = rand::thread_rng().gen::<[u8; 32]>();
            let u256: U256 = random_bytes.into();
            seq.push(u256);
        }
        let message = Message::Pong(Pong::new(self.last_id.clone(), seq));
        let frame = StandardSv2Frame::<Message<'static>>::from_message(message).unwrap();
        self.sender.send(frame.into()).await.unwrap();
        self.last_id += 1;
    }

    async fn respond(&mut self, frame: StandardSv2Frame<Message<'static>>) {
        let response = self.handle_message(frame);
        let frame = StandardSv2Frame::<Message<'static>>::from_message(response).unwrap();
        self.sender.send(frame.into()).await.unwrap();
        self.last_id += 1;
    }

    fn handle_message(
        &mut self,
        mut frame: StandardSv2Frame<Message<'static>>,
    ) -> Message<'static> {
        match self.expected {
            Expected::Ping => {
                let ping: Result<Ping, _> = from_bytes(frame.payload());
                match ping {
                    Ok(ping) => {
                        println!("Node {} recived:", self.name);
                        println!("{:#?}\n", ping);
                        let mut seq: Vec<U256> = vec![];
                        for _ in 0..3000 {
                            let random_bytes = rand::thread_rng().gen::<[u8; 32]>();
                            let u256: U256 = random_bytes.into();
                            seq.push(u256);
                        }
                        Message::Pong(Pong::new(self.last_id.clone(), seq))
                    }
                    Err(_) => {
                        panic!();
                    }
                }
            }
            Expected::Pong => {
                let pong: Result<Pong, _> = from_bytes(frame.payload());
                match pong {
                    Ok(pong) => {
                        println!("Node {} recived:", self.name);
                        println!(
                            "Pong, id: {:#?}, message len: {} \n",
                            pong.get_id(),
                            pong.get_len()
                        );
                        Message::Ping(Ping::new(self.last_id.clone()))
                    }
                    Err(_) => panic!(),
                }
            }
        }
    }
}
