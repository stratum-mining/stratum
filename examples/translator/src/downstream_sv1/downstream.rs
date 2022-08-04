use crate::downstream_sv1::DownstreamConnection;
use async_std::net::TcpStream;

use async_channel::{bounded, Receiver, Sender};
use async_std::{io::BufReader, prelude::*, task};
use roles_logic_sv2::common_properties::{IsDownstream, IsMiningDownstream};
use roles_logic_sv2::utils::Mutex;
use std::sync::Arc;
use v1::json_rpc;

/// Handles the sending and receiving of messages to and from an SV2 Upstream role (most typically
/// a SV2 Pool server).
#[derive(Debug)]
pub(crate) struct Downstream {
    connection: DownstreamConnection,
}
// new task loops through receiver upstream is sending something, if so use sender outgoing and
// transform to sv1 messages then use sender outgoing to send to the socket
impl IsMiningDownstream for Downstream {}
impl IsDownstream for Downstream {
    fn get_downstream_mining_data(
        &self,
    ) -> roles_logic_sv2::common_properties::CommonDownstreamData {
        todo!()
    }
}

impl Downstream {
    pub async fn new(
        stream: TcpStream,
        sender_upstream: Sender<json_rpc::Message>,
        receiver_upstream: Receiver<json_rpc::Message>,
    ) -> Arc<Mutex<Self>> {
        let stream = std::sync::Arc::new(stream);

        // Reads and writes from Downstream SV1 Mining Device Client
        let (socket_reader, socket_writer) = (stream.clone(), stream);
        let (sender_incoming, receiver_incoming) = bounded(10);
        let (sender_outgoing, receiver_outgoing) = bounded(10);

        let connection = DownstreamConnection {
            sender_outgoing,
            receiver_incoming,
            sender_upstream,
            receiver_upstream,
        };
        let sender_upstream_clone = connection.sender_upstream.clone();
        let receiver_upstream_clone = connection.receiver_upstream.clone();
        let receiver_incoming_clone = connection.receiver_incoming.clone();

        let downstream = Arc::new(Mutex::new(Downstream { connection }));
        let self_ = downstream.clone();

        // 1. Task to read from SV1 Mining Device Client socket via `socket_reader` +
        //    `sender_incoming` sends to `receiver_incoming`.
        // 2. Loop with `receiver_incoming` awaiting a message from Client socket via
        //    `sender_incoming`. Serializes received message as `json_rpc::Message` and sends via
        //    `upstream_sender` to the Upstream `Translator.downstream_receiver`.
        // 3. Task with `receiver_upstream` awaiting a message from Upstream
        //    `Translator.sender_downstream`. Formats `json_rpc:Message` as string and sends via
        //    `sender_outgoing` to `receiver_outgoing` to be written to the SV1 Mining Device
        //    Client socket.
        // 4. Task with `receiver_outgoing` waiting to receive message from `sender_outgoing` to be
        //    written to the SV1 Mining Device Client socket.

        // 1. Task to read from SV1 Mining Device Client socket via `socket_reader` +
        //    `sender_incoming` sends to `receiver_incoming`.
        // task::spawn(async move {
        //     loop {
        //         // Read message from SV1 Mining Device Client socket
        //         let mut messages = BufReader::new(&*socket_reader).lines();
        //         let message = message.unwrap();
        //         sender_incoming.send(message).await.unwrap();
        //         }
        //     }
        // });
        //
        // 1. Task to read from SV1 Mining Device Client socket via `socket_reader`. Parses
        //    received message as `json_rpc::Message` + sends to upstream
        //    `Translator.receive_downstream` via `sender_upstream` (done in
        //    `send_message_upstream`.
        task::spawn(async move {
            loop {
                // Read message from SV1 Mining Device Client socket
                let mut messages = BufReader::new(&*socket_reader).lines();
                // On message receive, parse to `json_rpc:Message` and send to Upstream
                // `Translator.receive_downstream` via `sender_upstream` done in
                // `send_message_upstream`.
                while let Some(incoming) = messages.next().await {
                    let incoming = incoming.unwrap();
                    let incoming: Result<json_rpc::Message, _> = serde_json::from_str(&incoming);
                    Self::send_message_upstream(self_.clone(), incoming.unwrap()).await;
                }
            }
        });

        // 4. Task to loop over the `receiver_outgoing` waiting to receive `json_rpc::Message`
        //    messages from `Translator.sender_downstream` to be written to the SV1 Mining Device
        //    Client socket.
        //    RR NOTE: may need to have another task to receive from Translator.sender_downstream
        //    into receiver_upstream, then pass that to sender_outgoing which then sends to
        //    receiver_outgoing which should replace the receiver_upstream here.
        task::spawn(async move {
            loop {
                let to_send = receiver_upstream_clone.recv().await.unwrap();
                let to_send = format!("{}\n", serde_json::to_string(&to_send).unwrap());
                (&*socket_writer)
                    .write_all(to_send.as_bytes())
                    .await
                    .unwrap();
            }
        });

        // // 2. Loop with `receiver_incoming` awaiting a message from Client socket via
        // //    `sender_incoming`. Serializes received message as `json_rpc::Message` and sends via
        // //    `upstream_sender` to the Upstream `Translator.downstream_receiver`.
        // loop {
        //     let incoming_downstream = dreceiver_incoming_clone.recv().await.unwrap();
        //     let message_sv1 = downstream
        //         .parse_message(Ok(incoming_downstream))
        //         .await
        //         .unwrap();
        //     downstream.upstream_sender.send(message_sv1).await.unwrap();
        // }
        //
        // // 3. Task with `receiver_upstream` awaiting a message from Upstream
        // //    `Translator.sender_downstream`. Formats `json_rpc:Message` as string and sends via
        // //    `sender_outgoing` to `receiver_outgoing` to be written to the SV1 Mining Device
        // //    Client socket.
        // // RR TODO: Have as loop, maybe should be task?
        // loop {
        //     let incoming_upstream = downstream.receiver_upstream.recv().await.unwrap();
        //     let message = format!("{}\n", serde_json::to_string(&incoming_upstream).unwrap());
        //     sender_outgoing.send(message).await.unwrap();
        // }
        //
        // // 4. Task with loop with `receiver_outgoing` waiting to receive message from
        // //    `sender_outgoing` to be written to the SV1 Mining Device Client socket.
        // task::spawn(async move {
        //     loop {
        //         let message: String = receiver_outgoing.recv().await.unwrap();
        //         (&*socket_writer)
        //             .write_all(message.as_bytes())
        //             .await
        //             .unwrap();
        //     }
        // });

        downstream
        //
        // // Spawn task to send SV1 message with `Downstream.sender_upstream` to
        // // `Translator.receiver_downstream` to to be translated to a SV2 message and forwarded to
        // // the Upstream
        // task::spawn(async move {
        //     loop {
        //         // Receives an incoming message from the Mining Device via `sender_outgoing`
        //         let to_send = receiver_outgoing.recv().await.unwrap();
        //         let sv1_message_to_send_upstream = to_send.clone();
        //         sender_upstream_clone
        //             .send(sv1_message_to_send_upstream)
        //             .await
        //             .unwrap();
        //         // let to_send = format!("{}\n", serde_json::to_string(&to_send).unwrap());
        //
        //         // (&*socket_writer)
        //         //     .write_all(to_send.as_bytes())
        //         //     .await
        //         //     .unwrap();
        //     }
        // });
        //
        // // Task to listen on Downstream socket for incoming messages. Receives messages and sends
        // // via `sender_incoming` to `receiver_incoming` waiting in another task.
        // // `receiver_incoming` and sends `sender_incoming` incoming in other task.
        // task::spawn(async move {
        //     let mut messages = BufReader::new(&*socket_reader).lines();
        //     while let Some(incoming) = messages.next().await {
        //         let incoming = incoming.unwrap();
        //         let incoming: Result<json_rpc::Message, _> = serde_json::from_str(&incoming);
        //         match incoming {
        //             Ok(message) => {
        //                 let to_send = Self::parse_message(self_.clone(), message).await;
        //                 sender_incoming.send(to_send).await.unwrap();
        //                 // match to_send {
        //                 //     Some(message) => {
        //                 //         // TODO: add relay_message fn
        //                 //         // self.relay_message(m).await;
        //                 //         // Sends Downstream messages received from socket to downstream
        //                 //         Self::send_message(self_.clone(), message).await;
        //                 //     }
        //                 //     None => (),
        //                 // }
        //             }
        //             Err(_) => (),
        //         }
        //     }
        // });
        //
        // // Spawn task to accept messages from SV1 Downstream socket from `sender_incoming` to
        // // `receiver_incoming`. Then send the received message via `sender_upstream` to be received
        // // by `Translator.receiver_downstream`.
        // task::spawn(async move {
        //     let message = receiver_incoming.recv().await;
        //     sender_upstream.send(message).await.unwrap();
        // });
        //
        // // Loop to receive SV1 message via `receiver_upstream` from the Upstream
        // // `Translator.sender_downstream`.
        // loop {
        //     let message_incoming = receiver_upstream.recv().await;
        //     translator.parse_message(Ok(message_incoming)).await;
        //
        // });
        //
        // // Spawn task to receive SV1 message from the SV1 Mining Device socket via
        // // `Downstream.sender_incoming`, which is sent to `Downstream.receiver_incoming` in the
        // // other task to be sent to the Upstream proxy `Translator.receiver_downstream`.
        //
        //
        // dowstream
    }

    // #[allow(clippy::single_match)]
    // async fn parse_message(
    //     self_: Arc<Mutex<Self>>,
    //     incoming_message: json_rpc::Message,
    // ) -> Option<json_rpc::Message> {
    //     if let Ok(line) = incoming_message {
    //         println!("CLIENT - message: {}", line);
    //         serde_json::from_str(&line)
    //     }
    // }

    /// Sends SV1 message to the Upstream Translator to be translated to SV2 and sent to the
    /// Upstream role (most typically a SV2 Pool).
    async fn send_message_upstream(self_: Arc<Mutex<Self>>, msg: json_rpc::Message) {
        let sender = self_
            .safe_lock(|s| s.connection.sender_upstream.clone())
            .unwrap();
        sender.send(msg).await.unwrap()
    }

    /// Sends SV1 message to the Downstream client (most typically a SV1 Mining Device).
    async fn send_message(self_: Arc<Mutex<Self>>, msg: json_rpc::Message) {
        let sender = self_
            .safe_lock(|s| s.connection.sender_outgoing.clone())
            .unwrap();
        sender.send(msg).await.unwrap()
    }
}
