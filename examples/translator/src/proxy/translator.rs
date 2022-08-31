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
    error::ProxyResult,
    proxy::{DownstreamTranslator, NextMiningNotify, UpstreamTranslator},
    upstream_sv2::{MiningMessage, Upstream},
};
use async_channel::{bounded, Receiver, Sender};
use async_std::task;
// use codec_sv2::Frame;
// use core::convert::TryInto;
// use roles_logic_sv2::{
//     parsers::{JobNegotiation, Mining},
// };
use roles_logic_sv2::utils::Mutex;
use std::sync::Arc;
use v1::json_rpc;

#[derive(Clone)]
pub(crate) struct Translator {
    pub(crate) downstream_translator: DownstreamTranslator,
    pub(crate) upstream_translator: UpstreamTranslator,
    pub(crate) next_mining_notify: NextMiningNotify,
}

impl Translator {
    // pub async fn new() -> Self {
    pub async fn initiate() {
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
            Sender<MiningMessage>,
            Receiver<MiningMessage>,
        ) = bounded(10);
        // A channel for the `Translator` to send to the `Upstream` and for the `Upstream` to
        // receive from the `Translator`
        let (sender_upstream_for_proxy, receiver_for_upstream): (
            Sender<MiningMessage>,
            Receiver<MiningMessage>,
        ) = bounded(10);

        let downstream_translator =
            DownstreamTranslator::new(sender_downstream_for_proxy, receiver_downstream_for_proxy);
        let upstream_translator =
            UpstreamTranslator::new(sender_upstream_for_proxy, receiver_upstream_for_proxy);
        let translator = Translator {
            downstream_translator,
            upstream_translator,
            next_mining_notify: NextMiningNotify::new(),
        };

        // Accept connection from one SV2 Upstream role (SV2 Pool)
        Upstream::accept_connection_upstream(sender_for_upstream, receiver_for_upstream).await;
        let translator_mutex = Arc::new(Mutex::new(translator));
        Translator::listen_upstream(translator_mutex.clone());

        // Accept connections from one or more SV1 Downstream roles (SV1 Mining Devices)
        Downstream::accept_connections(sender_for_downstream, receiver_for_downstream);
        Translator::listen_downstream(translator_mutex.clone()).await;
    }

    /// Spawn task to listen for incoming messages from SV2 Upstream.
    /// Spawned task waits to receive a message from `Upstream.connection.sender_downstream`,
    /// then parses the message + translates to SV1. Then the
    /// `Translator.downstream_translator.sender` sends the SV1 message to the
    /// `Downstream.receiver_upstream`.
    fn listen_upstream(self_: Arc<Mutex<Self>>) {
        task::spawn(async move {
            println!("TP LISTENING FOR INCOMING SV2 MSG FROM TU\n");
            loop {
                let receiver = self_
                    .safe_lock(|r| r.upstream_translator.receiver.clone())
                    .unwrap();
                let message_sv2: MiningMessage = receiver.recv().await.unwrap();
                println!("TP RECV SV2 FROM TU: {:?}", &message_sv2);
                // Works because parse_sv2_to_sv1 is NOT async
                let message_sv1 = self_
                    .safe_lock(|s| s.parse_sv2_to_sv1(message_sv2).unwrap())
                    .unwrap();

                if let Some(m) = message_sv1 {
                    let sender = self_
                        .safe_lock(|s| s.downstream_translator.sender.clone())
                        .unwrap();
                    sender.send(m).await.unwrap();
                };
            }
        });
    }

    /// Spawn task to listen for incoming messages from SV1 Downstream.
    /// Spawned task waits to receive a message from `Downstream.connection.sender_upstream`,
    /// then parses the message + translates to SV2. Then the `Translator.sender_upstream` sends
    /// the SV2 message to the `Upstream.receiver_downstream`.
    async fn listen_downstream(self_: Arc<Mutex<Self>>) {
        task::spawn(async move {
            println!("TP LISTENING FOR INCOMING SV1 MSG FROM TD\n");
            loop {
                let receiver = self_
                    .clone()
                    .safe_lock(|r| r.downstream_translator.receiver.clone())
                    .unwrap();
                let message_sv1: json_rpc::Message = receiver.recv().await.unwrap();
                let _message_sv2 =
                    Translator::handle_incoming_sv1(self_.clone(), message_sv1).await;
                //
                // let _message_sv2 = self.handle_incoming_sv1(message_sv1).await;
                // let _message_sv2 = match self.parse_sv1_to_sv2(message_sv1) {
                //     Ok(msv2) => (),
                //     Err(_) => return Err(Error::bad_sv1_std_req("bad")),
                // };
                // if false {
                //     return Ok(());
                // }
                // let message_sv2: EitherFrame = self.parse_sv1_to_sv2(message_sv1)?;
                // self.upstream_translator.send_sv2(message_sv2).await;
            }
        })
        .await;
    }

    /// Parses a SV1 message and translates to to a SV2 message
    // fn parse_sv1_to_sv2(&mut self, _message_sv1: json_rpc::Message) -> Result<EitherFrame> {
    async fn handle_incoming_sv1(self_: Arc<Mutex<Self>>, message_sv1: json_rpc::Message) {
        println!("TP RECV SV1 FROM TD TO HANDLE: {:?}", &message_sv1);
        match message_sv1 {
            json_rpc::Message::StandardRequest(std_req) => {
                println!("STDREQ: {:?}", std_req);
                // let _message_sv2 = Translator::handle_sv1_std_req(self_, std_req).await;
                Translator::handle_sv1_std_req(self_, std_req).await;
                // let _message_sv2 = self.handle_sv1_std_req(std_req).await;
            }
            json_rpc::Message::Notification(not) => println!("NOTIFICATION: {:?}", not),
            json_rpc::Message::OkResponse(ok_res) => println!("OKRES: {:?}", ok_res),
            json_rpc::Message::ErrorResponse(err_res) => println!("ERRRES: {:?}", err_res),
        };
    }

    /// Parses a SV2 message and translates to to a SV1 message
    /// TODO: rename
    fn parse_sv2_to_sv1(
        &mut self,
        message_sv2: MiningMessage,
    ) -> ProxyResult<Option<json_rpc::Message>> {
        println!("TP PARSE SV2 -> SV1: {:?}", &message_sv2);
        match message_sv2 {
            MiningMessage::NewExtendedMiningJob(m) => {
                self.next_mining_notify.new_extended_mining_job = Some(m);
                Ok(None)
            }
            MiningMessage::SetNewPrevHash(m) => {
                self.next_mining_notify.set_new_prev_hash = Some(m);
                Ok(None)
            }
            _ => {
                println!("TODO!!: TP RECV OTHER MESSAGE: {:?}", &message_sv2);
                Ok(None)
            }
        }
    }

    // TODO: use SendTo here to add context, but SendTo uses Sv2
    // Create an enum SendToSv1
    // 1. send mutex to this fn, and use mutex to send here + make everything work
    // 2. think of more elegant way to handle
    async fn handle_sv1_std_req(
        self_: Arc<Mutex<Self>>,
        std_req: json_rpc::StandardRequest,
    ) -> json_rpc::Message {
        let method = std_req.method;
        match method.as_ref() {
            // Use SV2 `SetNewPrevHash` + `NewExtendedMiningJob` to create a SV1 `mining.subscribe`
            // response message to send to the Downstream MD
            "mining.subscribe" => {
                let sv1_message_to_send_downstream = self_
                    .safe_lock(|s| s.next_mining_notify.create_subscribe_response())
                    .unwrap();
                sv1_message_to_send_downstream

                // self_
                //     .safe_lock(|s| {
                //         s.downstream_translator
                //             .sender
                //             .send(sv1_message_to_send_downstream)
                //             .await
                //             .unwrap()
                //     })
                //     .unwrap();
                // self.downstream_translator
                //     .sender
                //     .send(sv1_message_to_send_downstream)
                //     .await
                //     .unwrap();
                //
                // let _ = self.next_mining_notify.create_notify();
                // let sv1_notify_message = self.next_mining_notify.create_notify();
                // self.downstream_translator
                //     .sender
                //     .send(sv1_notify_message)
                //     .await
                //     .unwrap();
                // Ok(())
            }
            // "mining.submit" => (),
            _ => panic!("RRR TODO"), // RR TODO
                                     // "mining.configure" => Err(Error::no_translation_required(
                                     //     "`mining.configure` should not be translated to SV2",
                                     // )),
                                     // _ => Err(Error::bad_sv1_std_req(format!(
                                     //     "Bad SV1 Mining Method: {}",
                                     //     method
                                     // )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    /// Mock a `Translator`.
    fn mock_translator() -> Translator {
        let (_, receiver_downstream_for_proxy): (
            Sender<json_rpc::Message>,
            Receiver<json_rpc::Message>,
        ) = bounded(10);
        let (sender_downstream_for_proxy, _): (
            Sender<json_rpc::Message>,
            Receiver<json_rpc::Message>,
        ) = bounded(10);
        let (_, receiver_upstream_for_proxy): (Sender<MiningMessage>, Receiver<MiningMessage>) =
            bounded(10);
        let (sender_upstream_for_proxy, _): (Sender<MiningMessage>, Receiver<MiningMessage>) =
            bounded(10);

        let downstream_translator =
            DownstreamTranslator::new(sender_downstream_for_proxy, receiver_downstream_for_proxy);
        let upstream_translator =
            UpstreamTranslator::new(sender_upstream_for_proxy, receiver_upstream_for_proxy);
        let next_mining_notify = NextMiningNotify::new();
        Translator {
            downstream_translator,
            upstream_translator,
            next_mining_notify,
        }
    }

    /// Mock a `json_rpc::StandardRequest`.
    fn mock_sv1_std_req(
        id: Option<&str>,
        method: &str,
        parameters: Option<serde_json::Value>,
    ) -> json_rpc::StandardRequest {
        let id = id.unwrap_or("1661448689530130279");
        let parameters = parameters.unwrap_or(serde_json::Value::Null);
        json_rpc::StandardRequest {
            id: id.into(),
            method: method.into(),
            parameters,
        }
    }

    #[test]
    fn ret_err_on_handle_bad_sv1_std_req() {
        let std_req = mock_sv1_std_req(None, "mining.bad_method", None);
        let translator = mock_translator();
        let actual = translator.handle_sv1_std_req(std_req).unwrap_err().kind();
        let expect = ErrorKind::BadSv1StdReq;
        assert_eq!(actual, expect);
    }

    #[test]
    fn ret_err_on_handle_bad_translation_attempt() {
        let std_req = mock_sv1_std_req(None, "mining.configure", None);
        let translator = mock_translator();
        let actual = translator.handle_sv1_std_req(std_req).unwrap_err().kind();
        let expect = ErrorKind::NoTranslationRequired;
        assert_eq!(actual, expect);
    }
}
