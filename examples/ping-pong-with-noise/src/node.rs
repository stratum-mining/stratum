use crate::messages::{Message, Ping, Pong};
use binary_sv2::{from_bytes, GetSize, U256};
use rand::Rng;

use async_channel::{Receiver, Sender};
use async_std::net::TcpStream;

use async_std::{
    sync::{Arc, Mutex},
    task,
};
use core::convert::TryInto;

use codec_sv2::{HandshakeRole, StandardEitherFrame, StandardSv2Frame};

use std::time;

use network_helpers_sv2::Connection;

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
    pub async fn new(
        name: String,
        socket: TcpStream,
        role: HandshakeRole,
        test_count: u32,
    ) -> Arc<Mutex<Self>> {
        let (receiver, sender) = Connection::new(socket, role, 10).await.unwrap();

        let node = Arc::new(Mutex::new(Node {
            last_id: 0,
            name,
            expected: Expected::Pong,
            receiver,
            sender,
        }));
        let cloned = node.clone();

        task::spawn(async move {
            loop {
                task::sleep(time::Duration::from_millis(500)).await;
                if let Some(mut node) = cloned.try_lock() {
                    if node.last_id > test_count {
                        node.sender.close();
                        node.receiver.close();
                        println!("Test Successful");
                        std::process::exit(0);
                    } else {
                        let incoming = node.receiver.recv().await.unwrap().try_into().unwrap();
                        node.respond(incoming).await;
                    }
                }
            }
        });

        node
    }

    pub async fn send_pong(&mut self) {
        self.expected = Expected::Ping;
        let mut seq: Vec<U256> = vec![];
        for _ in 0..10 {
            let random_bytes = rand::thread_rng().gen::<[u8; 32]>();
            let u256: U256 = random_bytes.into();
            seq.push(u256);
        }
        let message = Message::Pong(Pong::new(self.last_id, seq));
        let frame =
            StandardSv2Frame::<Message<'static>>::from_message(message, 0, 0, false).unwrap();
        self.sender.send(frame.into()).await.unwrap();
        self.last_id += 1;
    }

    async fn respond(&mut self, frame: StandardSv2Frame<Message<'static>>) {
        let response = self.handle_message(frame);
        let frame =
            StandardSv2Frame::<Message<'static>>::from_message(response, 0, 0, false).unwrap();
        self.sender.send(frame.into()).await.unwrap();
        self.last_id += 1;
    }

    fn handle_message(
        &mut self,
        mut frame: StandardSv2Frame<Message<'static>>,
    ) -> Message<'static> {
        match self.expected {
            Expected::Ping => {
                let ping: Result<Ping, _> = from_bytes(frame.payload().unwrap());
                match ping {
                    Ok(ping) => {
                        println!("Node {} received:", self.name);
                        println!("{:#?}\n", ping);
                        let mut seq: Vec<U256> = vec![];
                        for _ in 0..3000 {
                            let random_bytes = rand::thread_rng().gen::<[u8; 32]>();
                            let u256: U256 = random_bytes.into();
                            seq.push(u256);
                        }
                        Message::Pong(Pong::new(self.last_id, seq))
                    }
                    Err(_) => {
                        panic!();
                    }
                }
            }
            Expected::Pong => {
                let pong: Result<Pong, _> = from_bytes(frame.payload().unwrap());
                match pong {
                    Ok(pong) => {
                        println!("Node {} received:", self.name);
                        println!(
                            "Pong, id: {:#?}, message len: {} \n",
                            pong.get_id(),
                            pong.get_size()
                        );
                        Message::Ping(Ping::new(self.last_id))
                    }
                    Err(_) => panic!(),
                }
            }
        }
    }
}
