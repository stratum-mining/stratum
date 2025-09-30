use std::{net::SocketAddr, sync::Arc};

use async_channel::{Receiver, Sender};
use stratum_common::{
    network_helpers_sv2::noise_stream::{NoiseTcpReadHalf, NoiseTcpWriteHalf},
    roles_logic_sv2::{
        bitcoin::{self, TxOut},
        codec_sv2::{binary_sv2::Str0255, Frame, StandardEitherFrame, StandardSv2Frame, Sv2Frame},
        common_messages_sv2::{
            Protocol, SetupConnection, MESSAGE_TYPE_CHANNEL_ENDPOINT_CHANGED,
            MESSAGE_TYPE_RECONNECT, MESSAGE_TYPE_SETUP_CONNECTION,
            MESSAGE_TYPE_SETUP_CONNECTION_ERROR, MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
        },
        job_declaration_sv2::{
            MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN, MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN_SUCCESS,
            MESSAGE_TYPE_DECLARE_MINING_JOB, MESSAGE_TYPE_DECLARE_MINING_JOB_ERROR,
            MESSAGE_TYPE_DECLARE_MINING_JOB_SUCCESS, MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS,
            MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS_SUCCESS, MESSAGE_TYPE_PUSH_SOLUTION,
        },
        mining_sv2::{
            CloseChannel, MESSAGE_TYPE_CLOSE_CHANNEL, MESSAGE_TYPE_MINING_SET_NEW_PREV_HASH,
            MESSAGE_TYPE_NEW_EXTENDED_MINING_JOB, MESSAGE_TYPE_NEW_MINING_JOB,
            MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL,
            MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCESS,
            MESSAGE_TYPE_OPEN_MINING_CHANNEL_ERROR, MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL,
            MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS, MESSAGE_TYPE_SET_CUSTOM_MINING_JOB,
            MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_ERROR, MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_SUCCESS,
            MESSAGE_TYPE_SET_EXTRANONCE_PREFIX, MESSAGE_TYPE_SET_GROUP_CHANNEL,
            MESSAGE_TYPE_SET_TARGET, MESSAGE_TYPE_SUBMIT_SHARES_ERROR,
            MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED, MESSAGE_TYPE_SUBMIT_SHARES_STANDARD,
            MESSAGE_TYPE_SUBMIT_SHARES_SUCCESS, MESSAGE_TYPE_UPDATE_CHANNEL,
            MESSAGE_TYPE_UPDATE_CHANNEL_ERROR,
        },
        parsers_sv2::AnyMessage,
        template_distribution_sv2::{
            MESSAGE_TYPE_COINBASE_OUTPUT_CONSTRAINTS, MESSAGE_TYPE_NEW_TEMPLATE,
            MESSAGE_TYPE_REQUEST_TRANSACTION_DATA, MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_ERROR,
            MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_SUCCESS, MESSAGE_TYPE_SET_NEW_PREV_HASH,
            MESSAGE_TYPE_SUBMIT_SOLUTION,
        },
    },
};
use tokio::sync::broadcast;
use tracing::{error, trace, warn, Instrument};

use crate::{
    error::PoolResult,
    status::{StatusSender, StatusType},
    task_manager::TaskManager,
};

pub type Message = AnyMessage<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;
pub type SV2Frame = Sv2Frame<Message, buffer_sv2::Slice>;

/// Represents a message that can trigger shutdown of various system components.
#[derive(Debug, Clone)]
pub enum ShutdownMessage {
    /// Shutdown all components immediately
    ShutdownAll,
    /// Shutdown all downstream connections
    DownstreamShutdownAll,
    /// Shutdown a specific downstream connection by ID
    DownstreamShutdown(u32),
}

/// Constructs a `SetupConnection` message for the mining protocol.
pub fn get_setup_connection_message(
    min_version: u16,
    max_version: u16,
) -> PoolResult<SetupConnection<'static>> {
    let endpoint_host = "0.0.0.0".to_string().into_bytes().try_into()?;
    let vendor = String::new().try_into()?;
    let hardware_version = String::new().try_into()?;
    let firmware = String::new().try_into()?;
    let device_id = String::new().try_into()?;
    let flags = 0b0000_0000_0000_0000_0000_0000_0000_0110;
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

/// Constructs a `SetupConnection` message for the Template Provider (TP).
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

/// Spawns async reader and writer tasks for handling framed I/O with shutdown support.
#[track_caller]
#[allow(clippy::too_many_arguments)]
pub fn spawn_io_tasks(
    task_manager: Arc<TaskManager>,
    mut reader: NoiseTcpReadHalf<Message>,
    mut writer: NoiseTcpWriteHalf<Message>,
    outbound_rx: Receiver<SV2Frame>,
    inbound_tx: Sender<SV2Frame>,
    notify_shutdown: broadcast::Sender<ShutdownMessage>,
    status_sender: StatusSender,
) {
    let caller = std::panic::Location::caller();
    let inbound_tx_clone = inbound_tx.clone();
    let outbound_rx_clone = outbound_rx.clone();
    {
        let mut shutdown_rx = notify_shutdown.subscribe();
        let status_sender = status_sender.clone();
        let status_type: StatusType = StatusType::from(&status_sender);

        task_manager.spawn(async move {
            trace!("Reader task started");
            loop {
                tokio::select! {
                    message = shutdown_rx.recv() => {
                        match message {
                            Ok(ShutdownMessage::ShutdownAll) => {
                                trace!("Received global shutdown");
                                inbound_tx.close();
                                break;
                            }
                            Ok(ShutdownMessage::DownstreamShutdown(down_id))  if matches!(status_type, StatusType::Downstream(id) if id == down_id) => {
                                trace!(down_id, "Received downstream shutdown");
                                if status_type != StatusType::TemplateReceiver {
                                    inbound_tx.close();
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    res = reader.read_frame() => {
                        match res {
                            Ok(frame) => {
                                match frame {
                                    Frame::HandShake(frame) => {
                                        error!(?frame, "Received handshake frame");
                                        drop(frame);
                                        break;
                                    },
                                    Frame::Sv2(sv2_frame) => {
                                        trace!("Received inbound frame");
                                        if let Err(e) = inbound_tx.send(sv2_frame).await {
                                            inbound_tx.close();
                                            error!(error=?e, "Failed to forward inbound frame");
                                            break;
                                        }
                                    },
                                }
                            }
                            Err(e) => {
                                error!(error=?e, "Reader error");
                                inbound_tx.close();
                                break;
                            }
                        }
                    }
                }
            }
            inbound_tx.close();
            outbound_rx_clone.close();
            drop(inbound_tx);
            drop(outbound_rx_clone);
            warn!("Reader task exited.");
        }.instrument(tracing::info_span!(
            "reader_task",
            spawned_at = %format!("{}:{}", caller.file(), caller.line())
        )));
    }

    {
        let mut shutdown_rx = notify_shutdown.subscribe();
        let status_type: StatusType = StatusType::from(&status_sender);

        task_manager.spawn(async move {
            trace!("Writer task started");
            loop {
                tokio::select! {
                    message = shutdown_rx.recv() => {
                        match message {
                            Ok(ShutdownMessage::ShutdownAll) => {
                                trace!("Received global shutdown");
                                outbound_rx.close();
                                break;
                            }
                            Ok(ShutdownMessage::DownstreamShutdown(down_id))  if matches!(status_type, StatusType::Downstream(id) if id == down_id) => {
                                trace!(down_id, "Received downstream shutdown");
                                if status_type != StatusType::TemplateReceiver {
                                    outbound_rx.close();
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    res = outbound_rx.recv() => {
                        match res {
                            Ok(frame) => {
                                trace!("Sending outbound frame");
                                if let Err(e) = writer.write_frame(frame.into()).await {
                                    error!(error=?e, "Writer error");
                                    outbound_rx.close();
                                    break;
                                }
                            }
                            Err(_) => {
                                outbound_rx.close();
                                warn!("Outbound channel closed");
                                break;
                            }
                        }
                    }
                }
            }
            outbound_rx.close();
            inbound_tx_clone.close();
            drop(outbound_rx);
            drop(inbound_tx_clone);
            warn!("Writer task exited.");
        }.instrument(tracing::info_span!(
            "writer_task",
            spawned_at = %format!("{}:{}", caller.file(), caller.line())
        )));
    }
}

/// Deserializes a raw coinbase output into a list of transaction outputs.
pub fn deserialize_coinbase_outputs(coinbase_output: &[u8]) -> Vec<TxOut> {
    bitcoin::consensus::deserialize(coinbase_output).expect("Invalid coinbase output")
}

/// Creates a [`CloseChannel`] message for the given channel ID and reason.
///
/// The `msg` is converted into a [`Str0255`] reason code.  
/// If conversion fails, this function will panic.
pub(crate) fn create_close_channel_msg<'a>(channel_id: u32, msg: &'a str) -> CloseChannel<'a> {
    CloseChannel {
        channel_id,
        reason_code: Str0255::try_from(msg.to_string()).expect("Could not convert message."),
    }
}

pub fn is_common_message(message_type: u8) -> bool {
    matches!(
        message_type,
        MESSAGE_TYPE_SETUP_CONNECTION
            | MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS
            | MESSAGE_TYPE_SETUP_CONNECTION_ERROR
            | MESSAGE_TYPE_CHANNEL_ENDPOINT_CHANGED
            | MESSAGE_TYPE_RECONNECT
    )
}

pub fn is_mining_message(message_type: u8) -> bool {
    matches!(
        message_type,
        MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL
            | MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS
            | MESSAGE_TYPE_OPEN_MINING_CHANNEL_ERROR
            | MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL
            | MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCESS
            | MESSAGE_TYPE_NEW_MINING_JOB
            | MESSAGE_TYPE_UPDATE_CHANNEL
            | MESSAGE_TYPE_UPDATE_CHANNEL_ERROR
            | MESSAGE_TYPE_CLOSE_CHANNEL
            | MESSAGE_TYPE_SET_EXTRANONCE_PREFIX
            | MESSAGE_TYPE_SUBMIT_SHARES_STANDARD
            | MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED
            | MESSAGE_TYPE_SUBMIT_SHARES_SUCCESS
            | MESSAGE_TYPE_SUBMIT_SHARES_ERROR
            // | MESSAGE_TYPE_RESERVED
            | 0x1e
            | MESSAGE_TYPE_NEW_EXTENDED_MINING_JOB
            | MESSAGE_TYPE_MINING_SET_NEW_PREV_HASH
            | MESSAGE_TYPE_SET_TARGET
            | MESSAGE_TYPE_SET_CUSTOM_MINING_JOB
            | MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_SUCCESS
            | MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_ERROR
            | MESSAGE_TYPE_SET_GROUP_CHANNEL
    )
}

pub fn is_job_declaration_message(message_type: u8) -> bool {
    matches!(
        message_type,
        MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN
            | MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN_SUCCESS
            | MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS
            | MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS_SUCCESS
            | MESSAGE_TYPE_DECLARE_MINING_JOB
            | MESSAGE_TYPE_DECLARE_MINING_JOB_SUCCESS
            | MESSAGE_TYPE_DECLARE_MINING_JOB_ERROR
            | MESSAGE_TYPE_PUSH_SOLUTION
    )
}

pub fn is_template_distribution_message(message_type: u8) -> bool {
    matches!(
        message_type,
        MESSAGE_TYPE_COINBASE_OUTPUT_CONSTRAINTS
            | MESSAGE_TYPE_NEW_TEMPLATE
            | MESSAGE_TYPE_SET_NEW_PREV_HASH
            | MESSAGE_TYPE_REQUEST_TRANSACTION_DATA
            | MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_SUCCESS
            | MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_ERROR
            | MESSAGE_TYPE_SUBMIT_SOLUTION
    )
}

#[derive(Debug, PartialEq, Eq)]
pub enum MessageType {
    Common,
    Mining,
    JobDeclaration,
    TemplateDistribution,
    Unknown,
}

pub fn protocol_message_type(message_type: u8) -> MessageType {
    if is_common_message(message_type) {
        MessageType::Common
    } else if is_mining_message(message_type) {
        MessageType::Mining
    } else if is_job_declaration_message(message_type) {
        MessageType::JobDeclaration
    } else if is_template_distribution_message(message_type) {
        MessageType::TemplateDistribution
    } else {
        MessageType::Unknown
    }
}
