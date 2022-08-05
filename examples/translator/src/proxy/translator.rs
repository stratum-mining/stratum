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
///    b.  SV2 Upstream role immediately sends a SV2 `SetNewPrevHash` + `NewExtendedMiningJob`
///        message.
///    c. If connection was successful, sends a SV2 `OpenExtendedMiningChannel` message to the SV2
///       Upstream role + receives a SV2 `OpenExtendedMiningChannelSuccess` or
///       `OpenMiningChannelError` message in response.
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
use async_channel::{bounded, Receiver, Sender};
use async_std::{net::TcpListener, prelude::*, task};
use roles_logic_sv2::utils::Mutex;
use std::{
    net::{IpAddr, SocketAddr},
    str::FromStr,
    sync::Arc,
};
use v1::json_rpc;

#[derive(Clone)]
/// The `Translator` is responsible for sending and receiving messages from the `Downstream` and
/// `Upstream`. It translates the messages into the appropriate protocol (SV1 or SV2) and routes
/// them to them to the appropriate role (`Downstream` or `Upstream`). The SV1 and SV2 protocols
/// are NOT 1-to-1, the `Translator` handles this.
pub(crate) struct Translator {
    /// Sends SV1 messages to the `Downstream::receiver_upstream`. These messages are either
    /// translated from SV2 messages received from the `Upstream`, or generated specifically for
    /// the SV1 protocol.
    pub(crate) sender_to_downstream: Sender<json_rpc::Message>,
    /// Receives SV1 messages to the `Downstream::receiver_upstream`. These messages are then
    /// handles by either translating them from SV1 to SV2 or dropped if not applicable to the SV2
    /// protocol.
    pub(crate) receiver_from_downstream: Receiver<json_rpc::Message>,
    /// Sends SV2 messages to the `Upstream::receiver_downstream`.
    pub(crate) sender_to_upstream: Sender<EitherFrame>,
    /// Receives SV2 messages from the `Upstream::sender_downstream`. These messages are then
    /// handled by either translating them from SV2 to SV1 or dropped if not applicable to the SV1
    /// protocol.
    pub(crate) receiver_from_upstream: Receiver<EitherFrame>,
}

impl Translator {
    /// Initializes a new `Translator` that handles all message translation and routing. There are
    /// four communication channels required:
    /// 1. A channel for the `Downstream` to send to the `Translator` and for the `Translator` to
    ///    receive from the `Downstream`:
    ///    `(sender_for_downstream, receiver_downstream_for_proxy)`
    /// 2. A channel for the `Translator` to send to the `Downstream` and for the `Downstream` to
    ///    receive from the `Translator`:
    ///    `(sender_downstream_for_proxy, receiver_for_downstream)`
    /// 3. A channel for the `Upstream` to send to the `Translator` and for the `Translator` to
    ///    receive from the `Upstream`:
    ///    `(sender_for_upstream, receiver_upstream_for_proxy)`
    /// 4. A channel for the `Translator` to send to the `Upstream` and for the `Upstream` to
    ///    receive from the `Translator`:
    ///    `(sender_upstream_for_proxy, receiver_for_upstream)`
    pub(crate) async fn new() -> Self {
        // A channel for the `Downstream` to send to the `Translator` and for the `Translator` to
        // receive from the `Downstream`
        let (sender_for_downstream, receiver_downstream_for_proxy): (
            Sender<json_rpc::Message>,
            Receiver<json_rpc::Message>,
        ) = bounded(10);
        // A channel for the `Translator` to send to the `Downstream` and for the `Downstream` to
        // receive from the `Translator`:
        let (sender_downstream_for_proxy, receiver_for_downstream): (
            Sender<json_rpc::Message>,
            Receiver<json_rpc::Message>,
        ) = bounded(10);
        // A channel for the `Upstream` to send to the `Translator` and for the `Translator` to
        // receive from the `Upstream`
        let (sender_for_upstream, receiver_upstream_for_proxy): (
            Sender<EitherFrame>,
            Receiver<EitherFrame>,
        ) = bounded(10);
        // A channel for the `Translator` to send to the `Upstream` and for the `Upstream` to
        // receive from the `Translator`
        let (sender_upstream_for_proxy, receiver_for_upstream): (
            Sender<EitherFrame>,
            Receiver<EitherFrame>,
        ) = bounded(10);

        let translator = Translator {
            sender_to_upstream: sender_upstream_for_proxy, // Sender<EitherFrame>
            receiver_from_upstream: receiver_upstream_for_proxy,
            sender_to_downstream: sender_downstream_for_proxy,
            receiver_from_downstream: receiver_downstream_for_proxy,
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
            sender_for_upstream,
            receiver_for_upstream,
        )
        .await;

        // Accept Downstream connections
        // task::spawn(async move {
        let downstream_listener = TcpListener::bind(crate::LISTEN_ADDR).await.unwrap();
        let mut downstream_incoming = downstream_listener.incoming();
        while let Some(stream) = downstream_incoming.next().await {
            let sender_for_downstream_clone = sender_for_downstream.clone();
            let receiver_for_downstream_clone = receiver_for_downstream.clone();
            let stream = stream.unwrap();
            println!(
                "PROXY SERVER - Accepting from: {}",
                stream.peer_addr().unwrap()
            );
            let server = Downstream::new(
                stream,
                sender_for_downstream_clone,
                receiver_for_downstream_clone,
            )
            .await;
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
                let message_sv1: json_rpc::Message = translator_clone
                    .receiver_from_downstream
                    .recv()
                    .await
                    .unwrap();
                println!("PROXY RECV FROM DOWNSTREAM: {:?}", &message_sv1);
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
                let message_sv2: EitherFrame = translator_clone
                    .receiver_from_upstream
                    .recv()
                    .await
                    .unwrap();
                println!("PROXY RECV FROM UPSTREAM: {:?}", &message_sv2);
                let message_sv1: json_rpc::Message = translator_clone.parse_sv2_to_sv1(message_sv2);
                translator_clone.send_sv1(message_sv1).await;
            }
        });

        translator
    }

    /// Parses a SV1 message and translates to to a SV2 message
    fn parse_sv1_to_sv2(&mut self, _message_sv1: json_rpc::Message) -> EitherFrame {
        todo!()
    }

    /// Parses a SV2 message and translates to to a SV1 message
    fn parse_sv2_to_sv1(&mut self, _message_sv2: EitherFrame) -> json_rpc::Message {
        todo!()
        // println!("PROXY PARSE SV2 -> SV1: {:?}", &message_sv2);
        // let message_str =
        //     r#"{"params": ["slush.miner1", "password"], "id": 2, "method": "mining.authorize"}"#;
        // let message_json: json_rpc::Message = serde_json::from_str(message_str).unwrap();
        // message_json
    }

    /// Sends SV2 message to the `Upstream.receiver_downstream`.
    async fn send_sv2(&mut self, message_sv2: EitherFrame) {
        self.sender_to_upstream.send(message_sv2).await.unwrap();
    }

    /// Sends SV1 message (translated from SV2) to the `Downstream.receiver_upstream`.
    async fn send_sv1(&mut self, message_sv1: json_rpc::Message) {
        self.sender_to_downstream.send(message_sv1).await.unwrap();
    }
}
