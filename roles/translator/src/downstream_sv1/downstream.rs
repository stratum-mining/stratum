use crate::{downstream_sv1, status::Status, ProxyResult};
use async_channel::{bounded, Receiver, Sender};
use async_std::{
    io::BufReader,
    net::{TcpListener, TcpStream},
    prelude::*,
    task,
};
use error_handling::handle_result;
use futures::FutureExt;

use super::{kill, SUBSCRIBE_TIMOUT_SECS};

use roles_logic_sv2::{
    bitcoin::util::uint::Uint256,
    common_properties::{IsDownstream, IsMiningDownstream},
    utils::Mutex,
};

use futures::select;

use std::{net::SocketAddr, ops::Div, sync::Arc};
use tracing::{debug, info, warn};
use v1::{
    client_to_server, json_rpc, server_to_client,
    utils::{Extranonce, HexU32Be},
    IsServer,
};

/// Handles the sending and receiving of messages to and from an SV2 Upstream role (most typically
/// a SV2 Pool server).
#[derive(Debug)]
pub struct Downstream {
    /// List of authorized Downstream Mining Devices.
    authorized_names: Vec<String>,
    extranonce1: Vec<u8>,
    /// `extranonce1` to be sent to the Downstream in the SV1 `mining.subscribe` message response.
    //extranonce1: Vec<u8>,
    //extranonce2_size: usize,
    /// Version rolling mask bits
    version_rolling_mask: Option<HexU32Be>,
    /// Minimum version rolling mask bits size
    version_rolling_min_bit: Option<HexU32Be>,
    /// Sends a SV1 `mining.submit` message received from the Downstream role to the `Bridge` for
    /// translation into a SV2 `SubmitSharesExtended`.
    tx_sv1_submit: Sender<(v1::client_to_server::Submit<'static>, Vec<u8>)>,
    /// Sends message to the SV1 Downstream role.
    tx_outgoing: Sender<json_rpc::Message>,
    /// True if this is the first job received from `Upstream`.
    first_job_received: bool,
    extranonce2_len: usize,
}

impl Downstream {
    /// Instantiate a new `Downstream`.
    #[allow(clippy::while_let_loop)]
    pub async fn new_downstream(
        stream: TcpStream,
        tx_sv1_submit: Sender<(v1::client_to_server::Submit<'static>, Vec<u8>)>,
        rx_sv1_notify: Receiver<server_to_client::Notify<'static>>,
        tx_status: Sender<Status<'static>>,
        extranonce1: Vec<u8>,
        last_notify: Option<server_to_client::Notify<'static>>,
        target: Vec<u8>,
        extranonce2_len: usize,
    ) {
        let stream = std::sync::Arc::new(stream);

        // Reads and writes from Downstream SV1 Mining Device Client
        let (socket_reader, socket_writer) = (stream.clone(), stream);
        let (tx_outgoing, receiver_outgoing) = bounded(10);

        let socket_writer_clone = socket_writer.clone();
        // let _socket_writer_set_difficulty_clone = socket_writer.clone();
        // Used to send SV1 `mining.notify` messages to the Downstreams
        let _socket_writer_notify = socket_writer;

        let downstream = Arc::new(Mutex::new(Downstream {
            authorized_names: vec![],
            extranonce1,
            //extranonce1: extranonce1.to_vec(),
            version_rolling_mask: None,
            version_rolling_min_bit: None,
            tx_sv1_submit,
            tx_outgoing,
            first_job_received: false,
            extranonce2_len,
        }));
        let self_ = downstream.clone();

        let (tx_shutdown, rx_shutdown): (Sender<bool>, Receiver<bool>) = async_channel::bounded(3);

        // Task to read from SV1 Mining Device Client socket via `socket_reader`. Depending on the
        // SV1 message received, a message response is sent directly back to the SV1 Downstream
        // role, or the message is sent upwards to the Bridge for translation into a SV2 message
        // and then sent to the SV2 Upstream role.
        let rx_shutdown_clone = rx_shutdown.clone();
        let tx_shutdown_clone = tx_shutdown.clone();
        let _socket_reader_task = task::spawn(async move {
            loop {
                task::sleep(std::time::Duration::from_millis(5)).await;
                // Read message from SV1 Mining Device Client socket
                let mut messages = BufReader::new(&*socket_reader).lines();
                // On message receive, parse to `json_rpc:Message` and send to Upstream
                // `Translator.receive_downstream` via `sender_upstream` done in
                // `send_message_upstream`.
                select! {
                    res = messages.next().fuse() => {
                        if let Some(Ok(incoming)) = res{
                            info!("Receiving from Mining Device: {:?}", &incoming);
                            let incoming: json_rpc::Message = match serde_json::from_str(&incoming) {
                                Ok(msg) => msg,
                                Err(_e) => {
                                    tracing::error!("\nBAD MESSAGE\n");
                                    break;
                                }
                            };
                            // Handle what to do with message
                            Self::handle_incoming_sv1(self_.clone(), incoming).await;
                        }
                    },
                    _ = rx_shutdown_clone.recv().fuse() => {
                            break;
                        }
                };
            }
            kill(&tx_shutdown_clone).await;
            warn!("SHUTTING DOWN READER");
        });

        let rx_shutdown_clone = rx_shutdown.clone();
        let tx_shutdown_clone = tx_shutdown.clone();
        // Task to receive SV1 message responses to SV1 messages that do NOT need translation.
        // These response messages are sent directly to the SV1 Downstream role.
        let _socket_writer_task = task::spawn(async move {
            loop {
                select! {
                    res = receiver_outgoing.recv().fuse() => {
                        match res {
                            Ok(to_send) => {
                                let to_send = match serde_json::to_string(&to_send) {
                                    Ok(string) => format!("{}\n", string),
                                    Err(_e) => {
                                        break;
                                    }
                                };
                                info!("Sending to Mining Device: {:?}", &to_send);
                                let res = (&*socket_writer_clone)
                                    .write_all(to_send.as_bytes())
                                    .await;
                                if let Err(_e) = res {
                                    break
                                }
                            }
                            Err(_e) => {
                                // should this kill the downstream or just send a status update to the main thread?
                                break;
                            }
                        }
                    },
                    _ = rx_shutdown_clone.recv().fuse() => {
                            break;
                        }
                };
            }
            kill(&tx_shutdown_clone).await;
            warn!("SHUTTING DOWN WRITER");
        });

        let _notify_task = task::spawn(async move {
            let timeout_timer = std::time::Instant::now();
            let mut first_sent = false;
            loop {
                let is_a = match downstream.safe_lock(|d| !d.authorized_names.is_empty()) {
                    Ok(is_a) => is_a,
                    Err(_e) => break,
                };
                if is_a && !first_sent && last_notify.is_some() {
                    let message =
                        handle_result!(tx_status_notify, Self::get_set_difficulty(target.clone()));
                    Downstream::send_message_downstream(downstream_clone.clone(), message).await;

                    match last_notify.clone() {
                        Some(sv1_mining_notify_msg) => {
                            let message: json_rpc::Message = match sv1_mining_notify_msg.try_into()
                            {
                                Ok(msg) => msg,
                                Err(_e) => {
                                    break;
                                }
                            };

                            Downstream::send_message_downstream(downstream.clone(), message).await;
                            if let Err(_e) = downstream.clone().safe_lock(|s| {
                                s.first_job_received = true;
                            }) {
                                break;
                            }
                            first_sent = true;
                        }
                        None => break,
                    }
                } else if is_a {
                    select! {
                        res = rx_sv1_notify.recv().fuse() => {
                            match res {
                                Ok(sv1_mining_notify_msg) => {
                                    let message: json_rpc::Message = match sv1_mining_notify_msg.try_into()
                                    {
                                        Ok(msg) => msg,
                                        Err(_e) => {
                                            // should this kill the downstream connection or log a status to the main thread and continue
                                            break;
                                        }
                                    };

                                    Downstream::send_message_downstream(downstream.clone(), message)
                                        .await;
                                }
                                Err(_e) => {
                                    break;
                                }
                            }
                        },
                        _ = rx_shutdown.recv().fuse() => {
                                break;
                            }
                    };
                } else {
                    // timeout connection if miner does not send the authorize message after sending a subscribe
                    if timeout_timer.elapsed().as_secs() > SUBSCRIBE_TIMOUT_SECS {
                        warn!("miner.subscribe/miner.authorize TIMOUT");
                        break;
                    }
                    task::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
            kill(&tx_shutdown).await;
            warn!("SHUTTING DOWN NOTIFIER");
        });
    }

    /// Helper function to check if target is set to zero for some reason (typically happens when
    /// Downstream role first connects).
    /// https://stackoverflow.com/questions/65367552/checking-a-vecu8-to-see-if-its-all-zero
    fn is_zero(buf: &[u8]) -> bool {
        let (prefix, aligned, suffix) = unsafe { buf.align_to::<u128>() };

        prefix.iter().all(|&x| x == 0)
            && suffix.iter().all(|&x| x == 0)
            && aligned.iter().all(|&x| x == 0)
    }

    /// Converts target received by the `SetTarget` SV2 message from the Upstream role into the
    /// difficulty for the Downstream role sent via the SV1 `mining.set_difficulty` message.
    fn difficulty_from_target(target: Vec<u8>) -> ProxyResult<'static, f64> {
        let target = target.as_slice();

        // If received target is 0, return 0
        if Downstream::is_zero(target) {
            return Ok(0.0);
        }
        let target = Uint256::from_be_slice(target)?;
        let pdiff: [u8; 32] = [
            0, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
        ];
        let pdiff = Uint256::from_be_bytes(pdiff);

        if pdiff > target {
            let diff = pdiff.div(target);
            Ok(diff.low_u64() as f64)
        } else {
            let diff = target.div(pdiff);
            let diff = diff.low_u64() as f64;
            // TODO still bring to too low difficulty shares
            Ok(1.0 / diff)
        }
    }

    /// Converts target received by the `SetTarget` SV2 message from the Upstream role into the
    /// difficulty for the Downstream role and creates the SV1 `mining.set_difficulty` message to
    /// be sent to the Downstream role.
    fn get_set_difficulty(target: Vec<u8>) -> ProxyResult<'static, json_rpc::Message> {
        let value = Downstream::difficulty_from_target(target)?;
        let set_target = v1::methods::server_to_client::SetDifficulty { value };
        let message: json_rpc::Message = set_target.into();
        Ok(message)
    }

    /// Accept connections from one or more SV1 Downstream roles (SV1 Mining Devices) and create a
    /// new `Downstream` for each connection.
    pub fn accept_connections(
        downstream_addr: SocketAddr,
        tx_sv1_submit: Sender<(v1::client_to_server::Submit<'static>, Vec<u8>)>,
        receiver_mining_notify: Receiver<server_to_client::Notify<'static>>,
        tx_status: Sender<Status<'static>>,
        bridge: Arc<Mutex<crate::proxy::Bridge>>,
    ) {
        task::spawn(async move {
            let downstream_listener = TcpListener::bind(downstream_addr).await.unwrap();
            let mut downstream_incoming = downstream_listener.incoming();

            while let Some(stream) = downstream_incoming.next().await {
                let stream = stream.expect("Err on SV1 Downstream connection stream");
                // TODO where should I pick the below value??
                let expected_hash_rate = 5_000_000.0;
                let open_sv1_downstream = bridge
                    .safe_lock(|s| s.on_new_sv1_connection(expected_hash_rate))
                    .unwrap();
                match open_sv1_downstream {
                    Some(opened) => {
                        info!(
                            "PROXY SERVER - ACCEPTING FROM DOWNSTREAM: {}",
                            stream.peer_addr().unwrap()
                        );
                        Downstream::new(
                            stream,
                            tx_sv1_submit.clone(),
                            receiver_mining_notify.clone(),
                            tx_status.clone(),
                            opened.extranonce,
                            opened.last_notify,
                            opened.target,
                            opened.extranonce2_len as usize,
                        )
                        .await
                        .unwrap();
                    }
                    None => todo!(),
                }
            }
        });
    }

    /// As SV1 messages come in, determines if the message response needs to be translated to SV2
    /// and sent to the `Upstream`, or if a direct response can be sent back by the `Translator`
    /// (SV1 and SV2 protocol messages are NOT 1-to-1).
    async fn handle_incoming_sv1(self_: Arc<Mutex<Self>>, message_sv1: json_rpc::Message) {
        // `handle_message` in `IsServer` trait + calls `handle_request`
        // TODO: Map err from V1Error to Error::V1Error
        let response = self_.safe_lock(|s| s.handle_message(message_sv1)).unwrap();
        match response {
            Ok(res) => {
                if let Some(r) = res {
                    // If some response is received, indicates no messages translation is needed
                    // and response should be sent directly to the SV1 Downstream. Otherwise,
                    // message will be sent to the upstream Translator to be translated to SV2 and
                    // forwarded to the `Upstream`
                    // let sender = self_.safe_lock(|s| s.connection.sender_upstream)
                    Self::send_message_downstream(self_, r.into()).await;
                } else {
                    // If None response is received, indicates this SV1 message received from the
                    // Downstream MD is passed to the `Translator` for translation into SV2
                }
            }
            Err(e) => {
                panic!("`{:?}`", e);
            }
        }
    }

    /// Send SV1 response message that is generated by `Downstream` (as opposed to being received
    /// by `Bridge`) to be written to the SV1 Downstream role.
    async fn send_message_downstream(self_: Arc<Mutex<Self>>, response: json_rpc::Message) {
        let sender = self_.safe_lock(|s| s.tx_outgoing.clone()).unwrap();
        debug!("To DOWN: {:?}", response);
        sender.send(response).await.unwrap();
    }
}

/// Implements `IsServer` for `Downstream` to handle the SV1 messages.
impl IsServer<'static> for Downstream {
    /// Handle the incoming `mining.configure` message which is received after a Downstream role is
    /// subscribed and authorized. Contains the version rolling mask parameters.
    fn handle_configure(
        &mut self,
        request: &client_to_server::Configure,
    ) -> (Option<server_to_client::VersionRollingParams>, Option<bool>) {
        info!("Down: Configuring");
        debug!("Down: Handling mining.configure: {:?}", &request);
        self.version_rolling_mask = Some(downstream_sv1::new_version_rolling_mask());
        self.version_rolling_min_bit = Some(downstream_sv1::new_version_rolling_min());
        (
            // unwraps safe since values are set above
            Some(server_to_client::VersionRollingParams::new(
                self.version_rolling_mask.clone().unwrap(),
                self.version_rolling_min_bit.clone().unwrap(),
            )),
            Some(false),
        )
    }

    /// Handle the response to a `mining.subscribe` message received from the client.
    /// The subscription messages are erroneous and just used to conform the SV1 protocol spec.
    /// Because no one unsubscribed in practice, they just unplug their machine.
    fn handle_subscribe(&self, request: &client_to_server::Subscribe) -> Vec<(String, String)> {
        info!("Down: Subscribing");
        debug!("Down: Handling mining.subscribe: {:?}", &request);

        let set_difficulty_sub = (
            "mining.set_difficulty".to_string(),
            downstream_sv1::new_subscription_id(),
        );
        let notify_sub = (
            "mining.notify".to_string(),
            "ae6812eb4cd7735a302a8a9dd95cf71f".to_string(),
        );

        vec![set_difficulty_sub, notify_sub]
    }

    /// Any numbers of workers may be authorized at any time during the session. In this way, a
    /// large number of independent Mining Devices can be handled with a single SV1 connection.
    /// https://bitcoin.stackexchange.com/questions/29416/how-do-pool-servers-handle-multiple-workers-sharing-one-connection-with-stratum
    fn handle_authorize(&self, request: &client_to_server::Authorize) -> bool {
        info!("Down: Authorizing");
        debug!("Down: Handling mining.authorize: {:?}", &request);
        true
    }

    /// When miner find the job which meets requested difficulty, it can submit share to the server.
    /// Only [Submit](client_to_server::Submit) requests for authorized user names can be submitted.
    fn handle_submit(&self, request: &client_to_server::Submit<'static>) -> bool {
        //info!("Down: Submitting Share");
        //debug!("Down: Handling mining.submit: {:?}", &request);

        // TODO: Check if receiving valid shares by adding diff field to Downstream

        if self.first_job_received {
            let mut tproxy_part =
                self.extranonce1[self.extranonce1.len() - crate::SELF_EXTRNONCE_LEN..].to_vec();
            let mut downstream_part: Vec<u8> = request.extra_nonce2.clone().into();
            tproxy_part.append(&mut downstream_part);

            let to_send = (request.clone(), tproxy_part);
            self.tx_sv1_submit.try_send(to_send).unwrap();
        };
        true
    }

    /// Indicates to the server that the client supports the mining.set_extranonce method.
    fn handle_extranonce_subscribe(&self) {}

    /// Checks if a Downstream role is authorized.
    fn is_authorized(&self, name: &str) -> bool {
        self.authorized_names.contains(&name.to_string())
    }

    /// Authorizes a Downstream role.
    fn authorize(&mut self, name: &str) {
        self.authorized_names.push(name.to_string());
    }

    /// Sets the `extranonce1` field sent in the SV1 `mining.notify` message to the value specified
    /// by the SV2 `OpenExtendedMiningChannelSuccess` message sent from the Upstream role.
    fn set_extranonce1(
        &mut self,
        _extranonce1: Option<Extranonce<'static>>,
    ) -> Extranonce<'static> {
        self.extranonce1.clone().try_into().unwrap()
    }

    /// Returns the `Downstream`'s `extranonce1` value.
    fn extranonce1(&self) -> Extranonce<'static> {
        self.extranonce1.clone().try_into().unwrap()
    }

    /// Sets the `extranonce2_size` field sent in the SV1 `mining.notify` message to the value
    /// specified by the SV2 `OpenExtendedMiningChannelSuccess` message sent from the Upstream role.
    fn set_extranonce2_size(&mut self, _extra_nonce2_size: Option<usize>) -> usize {
        self.extranonce2_len
    }

    /// Returns the `Downstream`'s `extranonce2_size` value.
    fn extranonce2_size(&self) -> usize {
        self.extranonce2_len
    }

    /// Returns the version rolling mask.
    fn version_rolling_mask(&self) -> Option<HexU32Be> {
        self.version_rolling_mask.clone()
    }

    /// Sets the version rolling mask.
    fn set_version_rolling_mask(&mut self, mask: Option<HexU32Be>) {
        self.version_rolling_mask = mask;
    }

    /// Sets the minimum version rolling bit.
    fn set_version_rolling_min_bit(&mut self, mask: Option<HexU32Be>) {
        self.version_rolling_min_bit = mask
    }

    fn notify(&mut self) -> Result<json_rpc::Message, v1::error::Error> {
        unreachable!()
    }
}

impl IsMiningDownstream for Downstream {}

impl IsDownstream for Downstream {
    fn get_downstream_mining_data(
        &self,
    ) -> roles_logic_sv2::common_properties::CommonDownstreamData {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gets_difficulty_from_target() {
        let mut target = vec![
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 128, 255, 127,
            0, 0, 0, 0, 0,
        ];
        target.reverse();
        let actual = Downstream::difficulty_from_target(target).unwrap();
        let expect = 512.0;
        assert_eq!(actual, expect);
    }
}
