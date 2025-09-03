use crate::{
    interceptor::{InterceptAction, MessageDirection},
    message_aggregator::MessagesAggregator,
    sniffer_error::SnifferError,
    types::{MessageFrame, MsgType},
};
use async_channel::{Receiver, Sender};
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use once_cell::sync::Lazy;
use std::{
    collections::HashSet,
    convert::TryInto,
    net::{SocketAddr, TcpListener},
    sync::Mutex,
};
use stratum_common::{
    network_helpers_sv2::noise_connection::Connection,
    roles_logic_sv2::{
        codec_sv2::{
            framing_sv2::framing::Frame, HandshakeRole, Initiator, Responder, StandardEitherFrame,
            Sv2Frame,
        },
        parsers_sv2::{
            message_type_to_name, AnyMessage, CommonMessages, IsSv2Message,
            JobDeclaration::{
                AllocateMiningJobToken, AllocateMiningJobTokenSuccess, DeclareMiningJob,
                DeclareMiningJobError, DeclareMiningJobSuccess, ProvideMissingTransactions,
                ProvideMissingTransactionsSuccess, PushSolution,
            },
            TemplateDistribution,
            TemplateDistribution::CoinbaseOutputConstraints,
        },
    },
};

// prevents get_available_port from ever returning the same port twice
static UNIQUE_PORTS: Lazy<Mutex<HashSet<u16>>> = Lazy::new(|| Mutex::new(HashSet::new()));

pub fn get_available_address() -> SocketAddr {
    let port = get_available_port();
    SocketAddr::from(([127, 0, 0, 1], port))
}

fn get_available_port() -> u16 {
    let mut unique_ports = UNIQUE_PORTS.lock().unwrap();

    loop {
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        if !unique_ports.contains(&port) {
            unique_ports.insert(port);
            return port;
        }
    }
}
pub async fn wait_for_client(listen_socket: SocketAddr) -> tokio::net::TcpStream {
    let listener = tokio::net::TcpListener::bind(listen_socket)
        .await
        .expect("Impossible to listen on given address");
    if let Ok((stream, _)) = listener.accept().await {
        stream
    } else {
        panic!("Impossible to accept dowsntream connection")
    }
}

pub async fn create_downstream(
    stream: tokio::net::TcpStream,
) -> Option<(Receiver<MessageFrame>, Sender<MessageFrame>)> {
    let pub_key = "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72"
        .to_string()
        .parse::<Secp256k1PublicKey>()
        .unwrap()
        .into_bytes();
    let prv_key = "mkDLTBBRxdBv998612qipDYoTK3YUrqLe8uWw7gu3iXbSrn2n"
        .to_string()
        .parse::<Secp256k1SecretKey>()
        .unwrap()
        .into_bytes();
    let responder =
        Responder::from_authority_kp(&pub_key, &prv_key, std::time::Duration::from_secs(10000))
            .unwrap();
    if let Ok((receiver_from_client, sender_to_client)) =
        Connection::new::<AnyMessage<'static>>(stream, HandshakeRole::Responder(responder)).await
    {
        Some((receiver_from_client, sender_to_client))
    } else {
        None
    }
}

pub async fn create_upstream(
    stream: tokio::net::TcpStream,
) -> Option<(Receiver<MessageFrame>, Sender<MessageFrame>)> {
    let initiator = Initiator::without_pk().expect("This fn call can not fail");
    if let Ok((receiver_from_server, sender_to_server)) =
        Connection::new::<AnyMessage<'static>>(stream, HandshakeRole::Initiator(initiator)).await
    {
        Some((receiver_from_server, sender_to_server))
    } else {
        None
    }
}

pub async fn recv_from_down_send_to_up(
    recv: Receiver<MessageFrame>,
    send: Sender<MessageFrame>,
    downstream_messages: MessagesAggregator,
    action: Vec<InterceptAction>,
    identifier: &str,
) -> Result<(), SnifferError> {
    while let Ok(mut frame) = recv.recv().await {
        let (msg_type, msg) = message_from_frame(&mut frame);
        let action = action.iter().find(|action| {
            action
                .find_matching_action(msg_type, MessageDirection::ToUpstream)
                .is_some()
        });
        if let Some(action) = action {
            match action {
                InterceptAction::IgnoreMessage(_) => {
                    tracing::info!(
                        "🔍 Sv2 Sniffer {} | Ignored: {} | Direction: ⬆",
                        identifier,
                        message_type_to_name(msg_type)
                    );
                    continue;
                }
                InterceptAction::ReplaceMessage(intercept_message) => {
                    let intercept_frame = StandardEitherFrame::<AnyMessage<'_>>::Sv2(
                        Sv2Frame::from_message(
                            intercept_message.replacement_message.clone(),
                            intercept_message.replacement_message.message_type(),
                            0,
                            false,
                        )
                        .expect("Failed to create the frame"),
                    );
                    downstream_messages.add_message(
                        intercept_message.replacement_message.message_type(),
                        intercept_message.replacement_message.clone(),
                    );
                    send.send(intercept_frame)
                        .await
                        .map_err(|_| SnifferError::UpstreamClosed)?;
                    tracing::info!(
                        "🔍 Sv2 Sniffer {} | Replaced: {} with {} | Direction: ⬆",
                        identifier,
                        message_type_to_name(msg_type),
                        message_type_to_name(intercept_message.replacement_message.message_type())
                    );
                }
            }
        } else {
            downstream_messages.add_message(msg_type, msg.clone());
            send.send(frame)
                .await
                .map_err(|_| SnifferError::UpstreamClosed)?;
            tracing::info!(
                "🔍 Sv2 Sniffer {} | Forwarded: {} | Direction: ⬆ | Data: {}",
                identifier,
                message_type_to_name(msg_type),
                msg
            );
        }
    }
    Err(SnifferError::DownstreamClosed)
}

pub async fn recv_from_up_send_to_down(
    recv: Receiver<MessageFrame>,
    send: Sender<MessageFrame>,
    upstream_messages: MessagesAggregator,
    action: Vec<InterceptAction>,
    identifier: &str,
) -> Result<(), SnifferError> {
    while let Ok(mut frame) = recv.recv().await {
        let (msg_type, msg) = message_from_frame(&mut frame);
        let action = action.iter().find(|action| {
            action
                .find_matching_action(msg_type, MessageDirection::ToDownstream)
                .is_some()
        });

        if let Some(action) = action {
            match action {
                InterceptAction::IgnoreMessage(_) => {
                    tracing::info!(
                        "🔍 Sv2 Sniffer {} | Ignored: {} | Direction: ⬇",
                        identifier,
                        message_type_to_name(msg_type)
                    );
                    continue;
                }
                InterceptAction::ReplaceMessage(intercept_message) => {
                    let intercept_frame = StandardEitherFrame::<AnyMessage<'_>>::Sv2(
                        Sv2Frame::from_message(
                            intercept_message.replacement_message.clone(),
                            intercept_message.replacement_message.message_type(),
                            0,
                            false,
                        )
                        .expect("Failed to create the frame"),
                    );
                    upstream_messages.add_message(
                        intercept_message.replacement_message.message_type(),
                        intercept_message.replacement_message.clone(),
                    );
                    send.send(intercept_frame)
                        .await
                        .map_err(|_| SnifferError::DownstreamClosed)?;
                    tracing::info!(
                        "🔍 Sv2 Sniffer {} | Replaced: {} with {} | Direction: ⬇",
                        identifier,
                        message_type_to_name(msg_type),
                        message_type_to_name(intercept_message.replacement_message.message_type())
                    );
                }
            }
        } else {
            upstream_messages.add_message(msg_type, msg.clone());
            send.send(frame)
                .await
                .map_err(|_| SnifferError::DownstreamClosed)?;
            tracing::info!(
                "🔍 Sv2 Sniffer {} | Forwarded: {} | Direction: ⬇ | Data: {}",
                identifier,
                message_type_to_name(msg_type),
                msg
            );
        }
    }
    Err(SnifferError::UpstreamClosed)
}

pub fn message_from_frame(frame: &mut MessageFrame) -> (MsgType, AnyMessage<'static>) {
    match frame {
        Frame::Sv2(frame) => {
            if let Some(header) = frame.get_header() {
                let message_type = header.msg_type();
                let mut payload = frame.payload().to_vec();
                let message: Result<AnyMessage<'_>, _> =
                    (message_type, payload.as_mut_slice()).try_into();
                match message {
                    Ok(message) => {
                        let message = into_static(message);
                        (message_type, message)
                    }
                    _ => {
                        println!("Received frame with invalid payload or message type: {frame:?}");
                        panic!();
                    }
                }
            } else {
                println!("Received frame with invalid header: {frame:?}");
                panic!();
            }
        }
        Frame::HandShake(f) => {
            println!("Received unexpected handshake frame: {f:?}");
            panic!();
        }
    }
}

pub fn into_static(m: AnyMessage<'_>) -> AnyMessage<'static> {
    match m {
        AnyMessage::Mining(m) => AnyMessage::Mining(m.into_static()),
        AnyMessage::Common(m) => match m {
            CommonMessages::ChannelEndpointChanged(m) => {
                AnyMessage::Common(CommonMessages::ChannelEndpointChanged(m.into_static()))
            }
            CommonMessages::SetupConnection(m) => {
                AnyMessage::Common(CommonMessages::SetupConnection(m.into_static()))
            }
            CommonMessages::SetupConnectionError(m) => {
                AnyMessage::Common(CommonMessages::SetupConnectionError(m.into_static()))
            }
            CommonMessages::SetupConnectionSuccess(m) => {
                AnyMessage::Common(CommonMessages::SetupConnectionSuccess(m.into_static()))
            }
            CommonMessages::Reconnect(m) => {
                AnyMessage::Common(CommonMessages::Reconnect(m.into_static()))
            }
        },
        AnyMessage::JobDeclaration(m) => match m {
            AllocateMiningJobToken(m) => {
                AnyMessage::JobDeclaration(AllocateMiningJobToken(m.into_static()))
            }
            AllocateMiningJobTokenSuccess(m) => {
                AnyMessage::JobDeclaration(AllocateMiningJobTokenSuccess(m.into_static()))
            }
            DeclareMiningJob(m) => AnyMessage::JobDeclaration(DeclareMiningJob(m.into_static())),
            DeclareMiningJobError(m) => {
                AnyMessage::JobDeclaration(DeclareMiningJobError(m.into_static()))
            }
            DeclareMiningJobSuccess(m) => {
                AnyMessage::JobDeclaration(DeclareMiningJobSuccess(m.into_static()))
            }
            ProvideMissingTransactions(m) => {
                AnyMessage::JobDeclaration(ProvideMissingTransactions(m.into_static()))
            }
            ProvideMissingTransactionsSuccess(m) => {
                AnyMessage::JobDeclaration(ProvideMissingTransactionsSuccess(m.into_static()))
            }
            PushSolution(m) => AnyMessage::JobDeclaration(PushSolution(m.into_static())),
        },
        AnyMessage::TemplateDistribution(m) => match m {
            CoinbaseOutputConstraints(m) => {
                AnyMessage::TemplateDistribution(CoinbaseOutputConstraints(m.into_static()))
            }
            TemplateDistribution::NewTemplate(m) => {
                AnyMessage::TemplateDistribution(TemplateDistribution::NewTemplate(m.into_static()))
            }
            TemplateDistribution::RequestTransactionData(m) => AnyMessage::TemplateDistribution(
                TemplateDistribution::RequestTransactionData(m.into_static()),
            ),
            TemplateDistribution::RequestTransactionDataError(m) => {
                AnyMessage::TemplateDistribution(TemplateDistribution::RequestTransactionDataError(
                    m.into_static(),
                ))
            }
            TemplateDistribution::RequestTransactionDataSuccess(m) => {
                AnyMessage::TemplateDistribution(
                    TemplateDistribution::RequestTransactionDataSuccess(m.into_static()),
                )
            }
            TemplateDistribution::SetNewPrevHash(m) => AnyMessage::TemplateDistribution(
                TemplateDistribution::SetNewPrevHash(m.into_static()),
            ),
            TemplateDistribution::SubmitSolution(m) => AnyMessage::TemplateDistribution(
                TemplateDistribution::SubmitSolution(m.into_static()),
            ),
        },
    }
}

pub mod http {
    pub fn make_get_request(download_url: &str, retries: usize) -> Vec<u8> {
        for attempt in 1..=retries {
            let response = minreq::get(download_url).send();
            match response {
                Ok(res) => {
                    let status_code = res.status_code;
                    if (200..300).contains(&status_code) {
                        return res.as_bytes().to_vec();
                    } else if (500..600).contains(&status_code) {
                        eprintln!(
                            "Attempt {attempt}: URL {download_url} returned a server error code {status_code}"
                        );
                    } else {
                        panic!(
                            "URL {download_url} returned unexpected status code {status_code}. Aborting."
                        );
                    }
                }
                Err(err) => {
                    eprintln!(
                        "Attempt {}: Failed to fetch URL {}: {:?}",
                        attempt + 1,
                        download_url,
                        err
                    );
                }
            }

            if attempt < retries {
                let delay = 1u64 << (attempt - 1);
                eprintln!("Retrying in {delay} seconds (exponential backoff)...");
                std::thread::sleep(std::time::Duration::from_secs(delay));
            }
        }
        // If all retries fail, panic with an error message
        panic!("Cannot reach URL {download_url} after {retries} attempts");
    }
}

pub mod tarball {
    use flate2::read::GzDecoder;
    use std::{
        fs::File,
        io::{BufReader, Read},
        path::Path,
    };
    use tar::Archive;

    pub fn read_from_file(path: &str) -> Vec<u8> {
        let file = File::open(path).unwrap_or_else(|_| {
            panic!("Cannot find {path:?} specified with env var BITCOIND_TARBALL_FILE")
        });
        let mut reader = BufReader::new(file);
        let mut buffer = Vec::new();
        reader.read_to_end(&mut buffer).unwrap();
        buffer
    }

    pub fn unpack(tarball_bytes: &[u8], destination: &Path) {
        let decoder = GzDecoder::new(tarball_bytes);
        let mut archive = Archive::new(decoder);
        for mut entry in archive.entries().unwrap().flatten() {
            if let Ok(file) = entry.path() {
                if file.ends_with("bitcoind") {
                    entry.unpack_in(destination).unwrap();
                }
            }
        }
    }
}

pub mod fs_utils {
    use std::{fs, path::Path};

    /// Recursively copy all contents from source directory to destination directory
    pub fn copy_dir_contents(src: &Path, dst: &Path) -> std::io::Result<()> {
        if !dst.exists() {
            fs::create_dir_all(dst)?;
        }

        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());

            if src_path.is_dir() {
                copy_dir_contents(&src_path, &dst_path)?;
            } else {
                fs::copy(&src_path, &dst_path)?;
            }
        }
        Ok(())
    }
}
