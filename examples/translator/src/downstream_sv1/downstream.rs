use crate::downstream_sv1::DownstreamConnection;
use async_std::net::TcpStream;

use async_channel::{Receiver, Sender};
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

        let connection = DownstreamConnection {
            sender_upstream,
            receiver_upstream,
        };
        let receiver_upstream_clone = connection.receiver_upstream.clone();

        let downstream = Arc::new(Mutex::new(Downstream { connection }));
        let self_ = downstream.clone();

        // Task to read from SV1 Mining Device Client socket via `socket_reader`. Parses received
        // message as `json_rpc::Message` + sends to upstream `Translator.receive_downstream` via
        // `sender_upstream` (done in `send_message_upstream`.
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
                    println!("PROXY DOWNSTREAM RECV: {:?}", &incoming);
                    Self::send_message_upstream(self_.clone(), incoming.unwrap()).await;
                }
            }
        });

        // Task to loop over the `receiver_outgoing` waiting to receive `json_rpc::Message`
        // messages from `Translator.sender_downstream` to be written to the SV1 Mining Device
        // Client socket.
        // RR NOTE: may need to have another task to receive from `Translator.sender_downstream`
        // into `receiver_upstream`, then pass that to `sender_outgoing` which then sends to
        // `receiver_outgoing` which should replace the `receiver_upstream` here.
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

        downstream
    }

    /// Sends SV1 message to the Upstream Translator to be translated to SV2 and sent to the
    /// Upstream role (most typically a SV2 Pool).
    async fn send_message_upstream(self_: Arc<Mutex<Self>>, msg: json_rpc::Message) {
        println!("RRMSG: {:?}", &msg);
        let sender = self_
            .safe_lock(|s| s.connection.sender_upstream.clone())
            .unwrap();
        sender.send(msg).await.unwrap()
    }
}
