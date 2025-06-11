//! ## Status Reporting System for Translator
//!
//! This module defines how internal components of the Translator report
//! health, errors, and shutdown conditions back to the main runtime loop in `lib/mod.rs`.
//!
//! At the core, tasks send a [`Status`] (wrapping a [`State`]) through a channel,
//! which is tagged with a [`Sender`] enum to indicate the origin of the message.
//!
//! This allows for centralized, consistent error handling across the application.

use network_helpers_sv2::roles_logic_sv2;

use crate::error::{self, Error};

/// Identifies the component that originated a [`Status`] update.
///
/// Each sender is associated with a dedicated side of the status channel.
/// This lets the central loop distinguish between errors from different parts of the system.
#[derive(Debug)]
pub enum Sender {
    /// Sender for downstream connections.
    Downstream(async_channel::Sender<Status<'static>>),
    /// Sender for downstream listener.
    DownstreamListener(async_channel::Sender<Status<'static>>),
    /// Sender for bridge connections.
    Bridge(async_channel::Sender<Status<'static>>),
    /// Sender for upstream connections.
    Upstream(async_channel::Sender<Status<'static>>),
    /// Sender for template receiver.
    TemplateReceiver(async_channel::Sender<Status<'static>>),
}

impl Sender {
    /// Converts a `DownstreamListener` sender to a `Downstream` sender.
    /// FIXME: Use `From` trait and remove this
    pub fn listener_to_connection(&self) -> Self {
        match self {
            Self::DownstreamListener(inner) => Self::Downstream(inner.clone()),
            _ => unreachable!(),
        }
    }

    /// Sends a status update.
    pub async fn send(
        &self,
        status: Status<'static>,
    ) -> Result<(), async_channel::SendError<Status<'_>>> {
        match self {
            Self::Downstream(inner) => inner.send(status).await,
            Self::DownstreamListener(inner) => inner.send(status).await,
            Self::Bridge(inner) => inner.send(status).await,
            Self::Upstream(inner) => inner.send(status).await,
            Self::TemplateReceiver(inner) => inner.send(status).await,
        }
    }
}

impl Clone for Sender {
    fn clone(&self) -> Self {
        match self {
            Self::Downstream(inner) => Self::Downstream(inner.clone()),
            Self::DownstreamListener(inner) => Self::DownstreamListener(inner.clone()),
            Self::Bridge(inner) => Self::Bridge(inner.clone()),
            Self::Upstream(inner) => Self::Upstream(inner.clone()),
            Self::TemplateReceiver(inner) => Self::TemplateReceiver(inner.clone()),
        }
    }
}

/// The kind of event or status being reported by a task.
#[derive(Debug)]
pub enum State<'a> {
    /// Downstream connection shutdown.
    DownstreamShutdown(Error<'a>),
    /// Bridge connection shutdown.
    BridgeShutdown(Error<'a>),
    /// Upstream connection shutdown.
    UpstreamShutdown(Error<'a>),
    /// Upstream connection trying to reconnect.
    UpstreamTryReconnect(Error<'a>),
    /// Component is healthy.
    Healthy(String),
}

/// Wraps a status update, to be passed through a status channel.
#[derive(Debug)]
pub struct Status<'a> {
    pub state: State<'a>,
}

/// Sends a [`Status`] message tagged with its [`Sender`] to the central loop.
///
/// This is the core logic used to determine which status variant should be sent
/// based on the error type and sender context.
async fn send_status(
    sender: &Sender,
    e: error::Error<'static>,
    outcome: error_handling::ErrorBranch,
) -> error_handling::ErrorBranch {
    match sender {
        Sender::Downstream(tx) => {
            tx.send(Status {
                state: State::Healthy(e.to_string()),
            })
            .await
            .unwrap_or(());
        }
        Sender::DownstreamListener(tx) => {
            tx.send(Status {
                state: State::DownstreamShutdown(e),
            })
            .await
            .unwrap_or(());
        }
        Sender::Bridge(tx) => {
            tx.send(Status {
                state: State::BridgeShutdown(e),
            })
            .await
            .unwrap_or(());
        }
        Sender::Upstream(tx) => match e {
            Error::ChannelErrorReceiver(_) => {
                tx.send(Status {
                    state: State::UpstreamTryReconnect(e),
                })
                .await
                .unwrap_or(());
            }
            _ => {
                tx.send(Status {
                    state: State::UpstreamShutdown(e),
                })
                .await
                .unwrap_or(());
            }
        },
        Sender::TemplateReceiver(tx) => {
            tx.send(Status {
                state: State::UpstreamShutdown(e),
            })
            .await
            .unwrap_or(());
        }
    }
    outcome
}

/// Centralized error dispatcher for the Translator.
///
/// Used by the `handle_result!` macro across the codebase.
/// Decides whether the task should `Continue` or `Break` based on the error type and source.
pub async fn handle_error(
    sender: &Sender,
    e: error::Error<'static>,
) -> error_handling::ErrorBranch {
    tracing::error!("Error: {:?}", &e);
    match e {
        Error::VecToSlice32(_) => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        // Errors on bad CLI argument input.
        Error::BadCliArgs => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        // Errors on bad `serde_json` serialize/deserialize.
        Error::BadSerdeJson(_) => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        // Errors on bad `config` TOML deserialize.
        Error::BadConfigDeserialize(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        // Errors from `binary_sv2` crate.
        Error::BinarySv2(_) => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        // Errors on bad noise handshake.
        Error::CodecNoise(_) => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        // Errors from `framing_sv2` crate.
        Error::FramingSv2(_) => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        //If the pool sends the tproxy an invalid extranonce
        Error::InvalidExtranonce(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        // Errors on bad `TcpStream` connection.
        Error::Io(_) => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        // Errors on bad `String` to `int` conversion.
        Error::ParseInt(_) => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        // Errors from `roles_logic_sv2` crate.
        Error::RolesSv2Logic(_) => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        Error::UpstreamIncoming(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        // SV1 protocol library error
        Error::V1Protocol(_) => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        Error::SubprotocolMining(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        // Locking Errors
        Error::PoisonLock => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        // Channel Receiver Error
        Error::ChannelErrorReceiver(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        Error::TokioChannelErrorRecv(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        // Channel Sender Errors
        Error::ChannelErrorSender(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        Error::SetDifficultyToMessage(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        Error::Infallible(_) => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        Error::Sv2ProtocolError(ref inner) => {
            match inner {
                // dont notify main thread just continue
                roles_logic_sv2::parsers::Mining::SubmitSharesError(_) => {
                    error_handling::ErrorBranch::Continue
                }
                _ => send_status(sender, e, error_handling::ErrorBranch::Break).await,
            }
        }
        Error::TargetError(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Continue).await
        }
        Error::Sv1MessageTooLong => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
    }
}
