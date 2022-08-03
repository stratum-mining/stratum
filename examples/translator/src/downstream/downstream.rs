use async_std::net::TcpStream;

use async_channel::{bounded, Receiver, Sender};
use async_std::{io::BufReader, prelude::*, task};
use roles_logic_sv2::common_properties::{IsDownstream, IsMiningDownstream};
use roles_logic_sv2::utils::Mutex;
use std::sync::Arc;
use v1::json_rpc;

#[derive(Debug)]
pub(crate) struct Downstream {
    receiver_incoming: Receiver<json_rpc::Message>,
    sender_outgoing: Sender<json_rpc::Message>,
}
impl IsMiningDownstream for Downstream {}
impl IsDownstream for Downstream {
    fn get_downstream_mining_data(
        &self,
    ) -> roles_logic_sv2::common_properties::CommonDownstreamData {
        todo!()
    }
}

impl Downstream {
    pub async fn new(stream: TcpStream) -> Arc<Mutex<Self>> {
        let stream = std::sync::Arc::new(stream);

        let (socket_reader, socket_writer) = (stream.clone(), stream);

        let (sender_incoming, receiver_incoming) = bounded(10);
        let (sender_outgoing, receiver_outgoing) = bounded(10);

        let dowstream = Arc::new(Mutex::new(Downstream {
            receiver_incoming,
            sender_outgoing,
        }));

        let self_ = dowstream.clone();
        task::spawn(async move {
            loop {
                let to_send = receiver_outgoing.recv().await.unwrap();
                let to_send = format!("{}\n", serde_json::to_string(&to_send).unwrap());
                (&*socket_writer)
                    .write_all(to_send.as_bytes())
                    .await
                    .unwrap();
            }
        });
        task::spawn(async move {
            let mut messages = BufReader::new(&*socket_reader).lines();
            while let Some(incoming) = messages.next().await {
                let incoming = incoming.unwrap();
                let incoming: Result<json_rpc::Message, _> = serde_json::from_str(&incoming);
                match incoming {
                    Ok(message) => {
                        let to_send = Self::parse_message(self_.clone(), message).await;
                        match to_send {
                            Some(message) => {
                                Self::send_message(self_.clone(), message).await;
                            }
                            None => (),
                        }
                    }
                    Err(_) => (),
                }
            }
        });

        dowstream
    }

    #[allow(clippy::single_match)]
    async fn parse_message(
        self_: Arc<Mutex<Self>>,
        incoming_message: json_rpc::Message,
    ) -> Option<json_rpc::Message> {
        todo!()
    }

    async fn send_message(self_: Arc<Mutex<Self>>, msg: json_rpc::Message) {
        let sender = self_.safe_lock(|s| s.sender_outgoing.clone()).unwrap();
        sender.send(msg).await.unwrap()
    }
}
