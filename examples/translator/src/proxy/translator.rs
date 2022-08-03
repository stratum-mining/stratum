use crate::{
    downstream_sv1::Downstream,
    upstream_sv2::{EitherFrame, Upstream},
};
use async_std::net::{TcpListener, TcpStream};
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;

use async_channel::{bounded, Receiver, Sender};
use async_std::{io::BufReader, prelude::*, task};
use roles_logic_sv2::utils::Mutex;
use std::sync::Arc;
use v1::json_rpc;

#[derive(Clone)]
pub(crate) struct Translator {
    /// Sends Sv2 messages  to the upstream. these sv2 messages were receieved from
    /// reciever_downstream and then translated from sv1 to sv2
    pub(crate) sender_upstream: Sender<EitherFrame>,
    /// Receives Sv2 messages from upstream to be translated into Sv1 and sent to downstream via
    /// the sender_downstream
    /// will have the other part of the channel on the upstream that wont be called sender_upstream
    /// (becuase we have sender_upstream here), the other part of the channel that lives on
    /// Upstream will be called sender_upstream
    pub(crate) receiver_upstream: Receiver<EitherFrame>,
    /// Sends Sv1 messages from initially received by reciever_upstream, then translated to Sv1 and
    /// then will be received by reciver_downstream
    pub(crate) sender_downstream: Sender<json_rpc::Message>,
    /// Receives Sv1 messages from the sender_downstream to be translated to Sv2 and sent to the
    /// sender_upstream
    pub(crate) receiver_downstream: Receiver<json_rpc::Message>,
}

impl Translator {
    pub(crate) async fn new() -> Self {
        let (sender_upstream, receiver_upstream) = bounded(10);
        let (sender_downstream, receiver_downstream) = bounded(10);

        let sender_upstream_clone = sender_upstream.clone();
        let receiver_upstream_clone = receiver_upstream.clone();
        let sender_downstream_clone = sender_downstream.clone();
        let receiver_downstream_clone = receiver_downstream.clone();

        let mut translator = Translator {
            sender_upstream,
            receiver_upstream,
            sender_downstream,
            receiver_downstream,
        };

        // Connect to Upstream
        let authority_public_key = [
            215, 11, 47, 78, 34, 232, 25, 192, 195, 168, 170, 209, 95, 181, 40, 114, 154, 226, 176,
            190, 90, 169, 238, 89, 191, 183, 97, 63, 194, 119, 11, 31,
        ];
        let upstream_addr = SocketAddr::new(IpAddr::from_str("127.0.0.1").unwrap(), 34254);
        let _upstream = Upstream::new(
            upstream_addr,
            authority_public_key,
            sender_upstream_clone,
            receiver_upstream_clone,
        )
        .await;

        // Accept Downstream connections
        let downstream_listener = TcpListener::bind(crate::LISTEN_ADDR).await.unwrap();
        let mut downstream_incoming = downstream_listener.incoming();
        while let Some(stream) = downstream_incoming.next().await {
            let sender_downstream_clone = sender_downstream_clone.clone();
            let receiver_downstream_clone = receiver_downstream_clone.clone();
            let stream = stream.unwrap();
            println!(
                "PROXY SERVER - Accepting from: {}",
                stream.peer_addr().unwrap()
            );
            let server =
                Downstream::new(stream, sender_downstream_clone, receiver_downstream_clone).await;
            Arc::new(Mutex::new(server));
        }

        // Spawn task to listen for incoming messages from SV1 Downstream.
        // Spawned task waits to receive a message from `Downstream.connection.sender_upstream`,
        // then parses the message + translates to SV2. Then the `Translator.sender_upstream` sends
        // the SV2 message to the `Upstream.receiver_downstream`.
        let mut translator_clone = translator.clone();
        task::spawn(async move {
            loop {
                let message_sv1: json_rpc::Message =
                    translator_clone.receiver_downstream.recv().await.unwrap();
                let message_sv2: EitherFrame = translator_clone.parse_sv1_to_sv2(message_sv1);
                translator_clone.send_sv2(message_sv2).await;
            }
        });

        // Spawn task to listen for incoming messages from SV2 Upstream.
        // Spawned task waits to receive a message from `Upstream.connection.sender_downstream`,
        // then parses the message + translates to SV1. Then the `Translator.sender_downstream`
        // sends the SV1 message to the `Downstream.receiver_upstream`.
        let mut translator_clone = translator.clone();
        task::spawn(async move {
            loop {
                let message_sv2: EitherFrame =
                    translator_clone.receiver_upstream.recv().await.unwrap();
                let message_sv1: json_rpc::Message = translator_clone.parse_sv2_to_sv1(message_sv2);
                translator_clone.send_sv1(message_sv1).await;
            }
        });

        translator
    }

    /// Parses a SV1 message and translates to to a SV2 message
    fn parse_sv1_to_sv2(&mut self, message_sv1: json_rpc::Message) -> EitherFrame {
        todo!()
    }

    /// Sends SV2 message (translated from SV1) to the `Upstream.receiver_downstream`.
    async fn send_sv2(&mut self, message_sv2: EitherFrame) {
        self.sender_upstream.send(message_sv2).await.unwrap();
    }

    /// Parses a SV2 message and translates to to a SV1 message
    fn parse_sv2_to_sv1(&mut self, message_sv2: EitherFrame) -> json_rpc::Message {
        todo!()
    }

    /// Sends SV1 message (translated from SV2) to the `Downstream.receiver_upstream`.
    async fn send_sv1(&mut self, message_sv1: json_rpc::Message) {
        self.sender_downstream.send(message_sv1).await.unwrap();
    }
}
