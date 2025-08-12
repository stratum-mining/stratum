use std::{net::SocketAddr, sync::Arc};

use async_channel::{Receiver, Sender};
use buffer_sv2::Slice;
use stratum_common::{
    network_helpers_sv2::noise_stream::{NoiseTcpReadHalf, NoiseTcpWriteHalf},
    roles_logic_sv2::{
        codec_sv2::{Frame, StandardEitherFrame, StandardSv2Frame},
        common_messages_sv2::{Protocol, SetupConnection},
        parsers_sv2::{AnyMessage, CommonMessages},
    },
};
use tokio::sync::broadcast;
use tracing::{error, info, warn};

use crate::{
    error::JDCError,
    status::{handle_error, StatusSender},
    task_manager::TaskManager,
};

pub type Message = AnyMessage<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;

#[derive(Debug, Clone)]
pub enum ShutdownMessage {
    /// Shutdown all components immediately
    ShutdownAll,
    /// Shutdown all downstream connections
    DownstreamShutdownAll,
    /// Shutdown a specific downstream connection by ID
    DownstreamShutdown(u32),
}

pub fn get_setup_connection_message(
    min_version: u16,
    max_version: u16,
    is_work_selection_enabled: bool,
) -> Result<SetupConnection<'static>, JDCError> {
    let endpoint_host = "0.0.0.0".to_string().into_bytes().try_into()?;
    let vendor = String::new().try_into()?;
    let hardware_version = String::new().try_into()?;
    let firmware = String::new().try_into()?;
    let device_id = String::new().try_into()?;
    let flags = match is_work_selection_enabled {
        false => 0b0000_0000_0000_0000_0000_0000_0000_0100,
        true => 0b0000_0000_0000_0000_0000_0000_0000_0110,
    };
    Ok(SetupConnection {
        protocol: Protocol::MiningProtocol,
        min_version,
        max_version,
        flags,
        endpoint_host,
        endpoint_port: 50,
        vendor,
        hardware_version,
        firmware,
        device_id,
    })
}

pub fn get_setup_connection_message_jds(proxy_address: &SocketAddr) -> SetupConnection<'static> {
    let endpoint_host = proxy_address
        .ip()
        .to_string()
        .into_bytes()
        .try_into()
        .unwrap();
    let vendor = String::new().try_into().unwrap();
    let hardware_version = String::new().try_into().unwrap();
    let firmware = String::new().try_into().unwrap();
    let device_id = String::new().try_into().unwrap();
    let mut setup_connection = SetupConnection {
        protocol: Protocol::JobDeclarationProtocol,
        min_version: 2,
        max_version: 2,
        flags: 0b0000_0000_0000_0000_0000_0000_0000_0000,
        endpoint_host,
        endpoint_port: proxy_address.port(),
        vendor,
        hardware_version,
        firmware,
        device_id,
    };
    setup_connection.allow_full_template_mode();
    setup_connection
}

pub fn get_setup_connection_message_tp(address: SocketAddr) -> SetupConnection<'static> {
    let endpoint_host = address.ip().to_string().into_bytes().try_into().unwrap();
    let vendor = String::new().try_into().unwrap();
    let hardware_version = String::new().try_into().unwrap();
    let firmware = String::new().try_into().unwrap();
    let device_id = String::new().try_into().unwrap();
    SetupConnection {
        protocol: Protocol::TemplateDistributionProtocol,
        min_version: 2,
        max_version: 2,
        flags: 0b0000_0000_0000_0000_0000_0000_0000_0000,
        endpoint_host,
        endpoint_port: address.port(),
        vendor,
        hardware_version,
        firmware,
        device_id,
    }
}

pub fn message_from_frame(
    frame: &mut Frame<AnyMessage<'static>, Slice>,
) -> Result<(u8, Vec<u8>, AnyMessage<'static>), JDCError> {
    match frame {
        Frame::Sv2(frame) => {
            let header = frame.get_header().ok_or(JDCError::UnexpectedMessage)?;
            let message_type = header.msg_type();
            let mut payload = frame.payload().to_vec();
            let message: Result<AnyMessage<'_>, _> =
                (message_type, payload.as_mut_slice()).try_into();
            match message {
                Ok(message) => {
                    let message = into_static(message)?;
                    Ok((message_type, payload.to_vec(), message))
                }
                Err(_) => {
                    error!("Received frame with invalid payload or message type: {frame:?}");
                    Err(JDCError::UnexpectedMessage)
                }
            }
        }
        Frame::HandShake(f) => {
            error!("Received unexpected handshake frame: {f:?}");
            Err(JDCError::UnexpectedMessage)
        }
    }
}

pub fn into_static(m: AnyMessage<'_>) -> Result<AnyMessage<'static>, JDCError> {
    match m {
        AnyMessage::Mining(m) => Ok(AnyMessage::Mining(m.into_static())),
        AnyMessage::Common(m) => match m {
            CommonMessages::ChannelEndpointChanged(m) => Ok(AnyMessage::Common(
                CommonMessages::ChannelEndpointChanged(m.into_static()),
            )),
            CommonMessages::SetupConnection(m) => Ok(AnyMessage::Common(
                CommonMessages::SetupConnection(m.into_static()),
            )),
            CommonMessages::SetupConnectionError(m) => Ok(AnyMessage::Common(
                CommonMessages::SetupConnectionError(m.into_static()),
            )),
            CommonMessages::SetupConnectionSuccess(m) => Ok(AnyMessage::Common(
                CommonMessages::SetupConnectionSuccess(m.into_static()),
            )),
            CommonMessages::Reconnect(m) => Ok(AnyMessage::Common(CommonMessages::Reconnect(
                m.into_static(),
            ))),
        },
        _ => Err(JDCError::UnexpectedMessage),
    }
}

pub fn spawn_io_tasks(
    task_manager: Arc<TaskManager>,
    mut reader: NoiseTcpReadHalf<Message>,
    mut writer: NoiseTcpWriteHalf<Message>,
    outbound_rx: Receiver<EitherFrame>,
    inbound_tx: Sender<EitherFrame>,
    notify_shutdown: broadcast::Sender<ShutdownMessage>,
    status_sender: StatusSender,
) {
    {
        let mut shutdown_rx = notify_shutdown.subscribe();
        let inbound_tx = inbound_tx.clone();
        let status_sender = status_sender.clone();

        task_manager.spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        info!("Reader: shutdown signal received");
                        break;
                    }
                    res = reader.read_frame() => {
                        match res {
                            Ok(frame) => {
                                if let Err(e) = inbound_tx.send(frame).await {
                                    error!("Failed to send inbound: {:?}", e);
                                    handle_error(&status_sender, JDCError::ChannelErrorSender).await;
                                    break;
                                }
                            }
                            Err(e) => {
                                error!("Reader error: {:?}", e);
                                handle_error(&status_sender, e.into()).await;
                                break;
                            }
                        }
                    }
                }
            }
            warn!("Reader task exited.");
        });
    }

    {
        let mut shutdown_rx = notify_shutdown.subscribe();

        task_manager.spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        info!("Writer: shutdown signal received");
                        break;
                    }
                    res = outbound_rx.recv() => {
                        match res {
                            Ok(frame) => {
                                if let Err(e) = writer.write_frame(frame).await {
                                    error!("Writer error: {:?}", e);
                                    handle_error(&status_sender, e.into()).await;
                                    break;
                                }
                            }
                            Err(_) => {
                                warn!("Writer channel closed.");
                                break;
                            }
                        }
                    }
                }
            }
            warn!("Writer task exited.");
        });
    }
}
