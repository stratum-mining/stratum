///
/// Translator is a Proxy server sits between a Downstream role (most typically a SV1 Mining
/// Device, but could also be a SV1 Proxy server) and an Upstream role (most typically a SV2 Pool
/// server, but could also be a SV2 Proxy server). It accepts and sends messages between the SV1
/// Downstream role and the SV2 Upstream role, translating the messages into the appropriate
/// protocol.
///
/// **Translator starts**
///
/// 1. Connects to SV2 Upstream role.
///    a. Sends a SV2 `SetupConnection` message to the SV2 Upstream role + receives a SV2
///       `SetupConnectionSuccess` or `SetupConnectionError` message in response.
///    b. If connection was successful, sends a SV2 `OpenExtendedMiningChannel` message to the SV2
///       Upstream role + receives a SV2 `OpenExtendedMiningChannelSuccess` or
///       `OpenMiningChannelError` message in response.
///    c. On successful open of channel, SV2 Upstream role sends a SV2 `SetNewPrevHash` +
///       `NewExtendedMiningJob` message.
///
/// 2. Meanwhile, Translator is listening for a SV1 Downstream role to connect. On connection:
///    a. Receives a SV1 `mining.subscribe` message from the SV1 Downstream role + sends a response
///       with a SV1 `mining.set_difficulty` + `mining.notify` which the Translator builds using
///       the SV2 `SetNewPrevHash` + `NewExtendedMiningJob` messages received from the SV2 Upstream
///       role.
///
/// 3. Translator waits for the SV1 Downstream role to find a valid share submission.
///    a. It receives this share submission via a SV1 `mining.submit` message + translates it into a
///       SV2 `SubmitSharesExtended` message which is then sent to the SV2 Upstream role + receives
///       a SV2 `SubmitSharesSuccess` or `SubmitSharesError` message in response.
///    b. This keeps happening until a new Bitcoin block is confirmed on the network, making this
///       current job's PrevHash stale.
///
/// 4. When a new block is confirmed on the Bitcoin network, the Translator sends a fresh job to
///    the SV1 Downstream role.
///    a. The SV2 Upstream role immediately sends the Translator a fresh SV2 `SetNewPrevHash`
///       followed by a `NewExtendedMiningJob` message.
///    b. Once the Translator receives BOTH messages, it translates them into a SV1 `mining.notify`
///       message + sends to the SV1 Downstream role.
///    c. The SV1 Downstream role begins finding a new valid share submission + Step 3 commences
///       again.
///
use crate::{
    downstream_sv1::Downstream,
    upstream_sv2::{EitherFrame, Upstream},
};
use async_std::net::TcpListener;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;

use async_channel::{bounded, Receiver, Sender};
use async_std::{prelude::*, task};
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
        println!("PROXY: Translator Proxy Starting");
        let (sender_upstream, receiver_upstream) = bounded(10);
        let (sender_downstream, receiver_downstream) = bounded(10);

        let sender_upstream_clone = sender_upstream.clone();
        let receiver_upstream_clone = receiver_upstream.clone();
        let sender_downstream_clone = sender_downstream.clone();
        let receiver_downstream_clone = receiver_downstream.clone();

        let translator = Translator {
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
        // task::spawn(async move {
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
        // });

        // Spawn task to listen for incoming messages from SV1 Downstream.
        // Spawned task waits to receive a message from `Downstream.connection.sender_upstream`,
        // then parses the message + translates to SV2. Then the `Translator.sender_upstream` sends
        // the SV2 message to the `Upstream.receiver_downstream`.
        let mut translator_clone = translator.clone();
        task::spawn(async move {
            loop {
                let message_sv1: json_rpc::Message =
                    translator_clone.receiver_downstream.recv().await.unwrap();
                println!("PROXY TRANSLATOR RECV: {:?}", &message_sv1);
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
                println!("PROXY UPSTREAM RECV: {:?}", &message_sv2);
                // let message_sv1: json_rpc::Message = translator_clone.parse_sv2_to_sv1(message_sv2);
                // translator_clone.send_sv1(message_sv1).await;
            }
        });

        translator
    }

    /// Parses a SV1 message and translates to to a SV2 message
    fn parse_sv1_to_sv2(&mut self, _message_sv1: json_rpc::Message) -> EitherFrame {
        todo!()
    }

    /// Sends SV2 message (translated from SV1) to the `Upstream.receiver_downstream`.
    async fn send_sv2(&mut self, message_sv2: EitherFrame) {
        self.sender_upstream.send(message_sv2).await.unwrap();
    }

    /// Parses a SV2 message and translates to to a SV1 message
    fn parse_sv2_to_sv1(&mut self, message_sv2: EitherFrame) -> json_rpc::Message {
        todo!()
        // println!("PROXY PARSE SV2 -> SV1: {:?}", &message_sv2);
        // let message_str =
        //     r#"{"params": ["slush.miner1", "password"], "id": 2, "method": "mining.authorize"}"#;
        // let message_json: json_rpc::Message = serde_json::from_str(message_str).unwrap();
        // message_json
    }

    /// Sends SV1 message (translated from SV2) to the `Downstream.receiver_upstream`.
    async fn send_sv1(&mut self, message_sv1: json_rpc::Message) {
        self.sender_downstream.send(message_sv1).await.unwrap();
    }
}
