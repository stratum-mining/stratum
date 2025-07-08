//! ## Status Reporting System for JDS
//!
//! This module defines how internal components of the Job Declarator Server (JDS) report
//! health, errors, and shutdown conditions back to the main runtime loop in `lib/mod.rs`.
//!
//! At the core, tasks send a [`Status`] (wrapping a [`State`]) through a channel,
//! which is tagged with a [`Sender`] enum to indicate the origin of the message.
//!
//! This allows for centralized, consistent error handling across the application.

use stratum_common::roles_logic_sv2::parsers_sv2::Mining;

use super::error::JdsError;

/// Identifies the component that originated a [`Status`] update.
///
/// Each sender is associated with a dedicated side of the status channel.
/// This lets the central loop distinguish between errors from different parts of the system.
#[derive(Debug)]
pub enum Sender {
    /// Downstream task (e.g. per-client connection handler)
    Downstream(async_channel::Sender<Status>),
    /// Listener for incoming downstream connections
    DownstreamListener(async_channel::Sender<Status>),
    /// Template Provider (Bitcoin Core RPC)
    Upstream(async_channel::Sender<Status>),
}

impl Clone for Sender {
    fn clone(&self) -> Self {
        match self {
            Self::Downstream(inner) => Self::Downstream(inner.clone()),
            Self::DownstreamListener(inner) => Self::DownstreamListener(inner.clone()),
            Self::Upstream(inner) => Self::Upstream(inner.clone()),
        }
    }
}

/// The kind of event or status being reported by a task.
#[derive(Debug)]
pub enum State {
    /// A downstream component (e.g. client) failed and should be shut down.
    DownstreamShutdown(JdsError),
    /// The Template Provider (upstream Bitcoin Core) failed.
    TemplateProviderShutdown(JdsError),
    /// A specific downstream instance was dropped (e.g., due to protocol error).
    DownstreamInstanceDropped(u32),
    /// A generic message to indicate health or non-critical errors.
    Healthy(String),
}

/// Wraps a status update, to be passed through a status channel.
#[derive(Debug)]
pub struct Status {
    pub state: State,
}

/// Sends a [`Status`] message tagged with its [`Sender`] to the central loop.
///
/// This is the core logic used to determine which status variant should be sent
/// based on the error type and sender context.
async fn send_status(
    sender: &Sender,
    e: JdsError,
    outcome: error_handling::ErrorBranch,
) -> error_handling::ErrorBranch {
    match sender {
        Sender::Downstream(tx) => match e {
            JdsError::Sv2ProtocolError((id, Mining::OpenMiningChannelError(_))) => {
                tx.send(Status {
                    state: State::DownstreamInstanceDropped(id),
                })
                .await
                .unwrap_or(());
            }
            JdsError::ChannelRecv(_) => {
                tx.send(Status {
                    state: State::DownstreamShutdown(e),
                })
                .await
                .unwrap_or(());
            }
            JdsError::MempoolError(_) => {
                tx.send(Status {
                    state: State::TemplateProviderShutdown(e),
                })
                .await
                .unwrap_or(());
            }
            _ => {
                let string_err = e.to_string();
                tx.send(Status {
                    state: State::Healthy(string_err),
                })
                .await
                .unwrap_or(());
            }
        },
        Sender::DownstreamListener(tx) => {
            tx.send(Status {
                state: State::DownstreamShutdown(e),
            })
            .await
            .unwrap_or(());
        }
        Sender::Upstream(tx) => {
            tx.send(Status {
                state: State::TemplateProviderShutdown(e),
            })
            .await
            .unwrap_or(());
        }
    }
    outcome
}

/// Centralized error dispatcher for the JDS.
///
/// Used by the `handle_result!` macro across the codebase.
/// Decides whether the task should `Continue` or `Break` based on the error type and source.
pub async fn handle_error(sender: &Sender, e: JdsError) -> error_handling::ErrorBranch {
    tracing::debug!("Error: {:?}", &e);
    match e {
        JdsError::Io(_) => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        JdsError::ChannelSend(_) => {
            //This should be a continue because if we fail to send to 1 downstream we should
            // continue processing the other downstreams in the loop we are in.
            // Otherwise if a downstream fails to send to then subsequent downstreams in
            // the map won't get send called on them
            send_status(sender, e, error_handling::ErrorBranch::Continue).await
        }
        JdsError::ChannelRecv(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        JdsError::BinarySv2(_) => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        JdsError::Codec(_) => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        JdsError::Noise(_) => send_status(sender, e, error_handling::ErrorBranch::Continue).await,
        JdsError::RolesLogic(_) => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        JdsError::Custom(_) => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        JdsError::Framing(_) => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        JdsError::PoisonLock(_) => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        JdsError::Sv2ProtocolError(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        JdsError::MempoolError(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        JdsError::ImpossibleToReconstructBlock(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Continue).await
        }
        JdsError::NoLastDeclaredJob => {
            send_status(sender, e, error_handling::ErrorBranch::Continue).await
        }
        JdsError::InvalidRPCUrl => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        JdsError::BadCliArgs => send_status(sender, e, error_handling::ErrorBranch::Break).await,
    }
}

#[cfg(test)]
mod tests {
    use std::{convert::TryInto, io::Error};

    use super::*;
    use async_channel::{bounded, RecvError};
    use stratum_common::roles_logic_sv2::{
        self,
        codec_sv2::{self, binary_sv2, noise_sv2},
        mining_sv2::OpenMiningChannelError,
    };

    #[tokio::test]
    async fn test_send_status_downstream_listener_shutdown() {
        let (tx, rx) = bounded(1);
        let sender = Sender::DownstreamListener(tx);
        let error = JdsError::ChannelRecv(async_channel::RecvError);

        send_status(&sender, error, error_handling::ErrorBranch::Continue).await;
        match rx.recv().await {
            Ok(status) => match status.state {
                State::DownstreamShutdown(e) => {
                    assert_eq!(e.to_string(), "Channel recv failed: `RecvError`")
                }
                _ => panic!("Unexpected state received"),
            },
            Err(_) => panic!("Failed to receive status"),
        }
    }

    #[tokio::test]
    async fn test_send_status_upstream_shutdown() {
        let (tx, rx) = bounded(1);
        let sender = Sender::Upstream(tx);
        let error = JdsError::MempoolError(crate::mempool::error::JdsMempoolError::EmptyMempool);
        let error_string = error.to_string();
        send_status(&sender, error, error_handling::ErrorBranch::Continue).await;

        match rx.recv().await {
            Ok(status) => match status.state {
                State::TemplateProviderShutdown(e) => assert_eq!(e.to_string(), error_string),
                _ => panic!("Unexpected state received"),
            },
            Err(_) => panic!("Failed to receive status"),
        }
    }

    #[tokio::test]
    async fn test_handle_error_io_error() {
        let (tx, rx) = bounded(1);
        let sender = Sender::Downstream(tx);
        let error = JdsError::Io(Error::new(std::io::ErrorKind::Interrupted, "IO error"));
        let error_string = error.to_string();

        handle_error(&sender, error).await;
        match rx.recv().await {
            Ok(status) => match status.state {
                State::Healthy(e) => assert_eq!(e, error_string),
                _ => panic!("Unexpected state received"),
            },
            Err(_) => panic!("Failed to receive status"),
        }
    }

    #[tokio::test]
    async fn test_handle_error_channel_send_error() {
        let (tx, rx) = bounded(1);
        let sender = Sender::Downstream(tx);
        let error = JdsError::ChannelSend(Box::new("error"));
        let error_string = error.to_string();

        handle_error(&sender, error).await;
        match rx.recv().await {
            Ok(status) => match status.state {
                State::Healthy(e) => assert_eq!(e, error_string),
                _ => panic!("Unexpected state received"),
            },
            Err(_) => panic!("Failed to receive status"),
        }
    }

    #[tokio::test]
    async fn test_handle_error_channel_receive_error() {
        let (tx, rx) = bounded(1);
        let sender = Sender::Downstream(tx);
        let error = JdsError::ChannelRecv(RecvError);
        let error_string = error.to_string();

        handle_error(&sender, error).await;
        match rx.recv().await {
            Ok(status) => match status.state {
                State::DownstreamShutdown(e) => assert_eq!(e.to_string(), error_string),
                _ => panic!("Unexpected state received"),
            },
            Err(_) => panic!("Failed to receive status"),
        }
    }

    #[tokio::test]
    async fn test_handle_error_binary_sv2_error() {
        let (tx, rx) = bounded(1);
        let sender = Sender::Downstream(tx);
        let error = JdsError::BinarySv2(binary_sv2::Error::IoError);
        let error_string = error.to_string();
        handle_error(&sender, error).await;
        match rx.recv().await {
            Ok(status) => match status.state {
                State::Healthy(e) => assert_eq!(e, error_string),
                _ => panic!("Unexpected state received"),
            },
            Err(_) => panic!("Failed to receive status"),
        }
    }

    #[tokio::test]
    async fn test_handle_error_codec_error() {
        let (tx, rx) = bounded(1);
        let sender = Sender::Downstream(tx);
        let error = JdsError::Codec(codec_sv2::Error::InvalidStepForInitiator);
        let error_string = error.to_string();
        handle_error(&sender, error).await;
        match rx.recv().await {
            Ok(status) => match status.state {
                State::Healthy(e) => assert_eq!(e, error_string),
                _ => panic!("Unexpected state received"),
            },
            Err(_) => panic!("Failed to receive status"),
        }
    }

    #[tokio::test]
    async fn test_handle_error_noise_error() {
        let (tx, rx) = bounded(1);
        let sender = Sender::Downstream(tx);
        let error = JdsError::Noise(noise_sv2::Error::HandshakeNotFinalized);
        let error_string = error.to_string();
        handle_error(&sender, error).await;
        match rx.recv().await {
            Ok(status) => match status.state {
                State::Healthy(e) => assert_eq!(e, error_string),
                _ => panic!("Unexpected state received"),
            },
            Err(_) => panic!("Failed to receive status"),
        }
    }

    #[tokio::test]
    async fn test_handle_error_roles_logic_error() {
        let (tx, rx) = bounded(1);
        let sender = Sender::Downstream(tx);
        let error = JdsError::RolesLogic(roles_logic_sv2::Error::BadPayloadSize);
        let error_string = error.to_string();
        handle_error(&sender, error).await;
        match rx.recv().await {
            Ok(status) => match status.state {
                State::Healthy(e) => assert_eq!(e, error_string),
                _ => panic!("Unexpected state received"),
            },
            Err(_) => panic!("Failed to receive status"),
        }
    }

    #[tokio::test]
    async fn test_handle_error_custom_error() {
        let (tx, rx) = bounded(1);
        let sender = Sender::Downstream(tx);
        let error = JdsError::Custom("error".to_string());
        let error_string = error.to_string();
        handle_error(&sender, error).await;
        match rx.recv().await {
            Ok(status) => match status.state {
                State::Healthy(e) => assert_eq!(e, error_string),
                _ => panic!("Unexpected state received"),
            },
            Err(_) => panic!("Failed to receive status"),
        }
    }

    #[tokio::test]
    async fn test_handle_error_framing_error() {
        let (tx, rx) = bounded(1);
        let sender = Sender::Downstream(tx);
        let error = JdsError::Framing(codec_sv2::framing_sv2::Error::ExpectedHandshakeFrame);
        let error_string = error.to_string();
        handle_error(&sender, error).await;
        match rx.recv().await {
            Ok(status) => match status.state {
                State::Healthy(e) => assert_eq!(e, error_string),
                _ => panic!("Unexpected state received"),
            },
            Err(_) => panic!("Failed to receive status"),
        }
    }

    #[tokio::test]
    async fn test_handle_error_poison_lock_error() {
        let (tx, rx) = bounded(1);
        let sender = Sender::Downstream(tx);
        let error = JdsError::PoisonLock("error".to_string());
        let error_string = error.to_string();
        handle_error(&sender, error).await;
        match rx.recv().await {
            Ok(status) => match status.state {
                State::Healthy(e) => assert_eq!(e, error_string),
                _ => panic!("Unexpected state received"),
            },
            Err(_) => panic!("Failed to receive status"),
        }
    }

    #[tokio::test]
    async fn test_handle_error_impossible_to_reconstruct_block_error() {
        let (tx, rx) = bounded(1);
        let sender = Sender::Downstream(tx);
        let error = JdsError::ImpossibleToReconstructBlock("Impossible".to_string());
        let error_string = error.to_string();
        handle_error(&sender, error).await;
        match rx.recv().await {
            Ok(status) => match status.state {
                State::Healthy(e) => assert_eq!(e, error_string),
                _ => panic!("Unexpected state received"),
            },
            Err(_) => panic!("Failed to receive status"),
        }
    }

    #[tokio::test]
    async fn test_handle_error_no_last_declared_job_error() {
        let (tx, rx) = bounded(1);
        let sender = Sender::Downstream(tx);
        let error = JdsError::NoLastDeclaredJob;
        let error_string = error.to_string();
        handle_error(&sender, error).await;
        match rx.recv().await {
            Ok(status) => match status.state {
                State::Healthy(e) => assert_eq!(e, error_string),
                _ => panic!("Unexpected state received"),
            },
            Err(_) => panic!("Failed to receive status"),
        }
    }

    #[tokio::test]
    async fn test_handle_error_last_mempool_error() {
        let (tx, rx) = bounded(1);
        let sender = Sender::Downstream(tx);
        let error = JdsError::MempoolError(crate::mempool::error::JdsMempoolError::EmptyMempool);
        let error_string = error.to_string();
        handle_error(&sender, error).await;
        match rx.recv().await {
            Ok(status) => match status.state {
                State::TemplateProviderShutdown(e) => assert_eq!(e.to_string(), error_string),
                _ => panic!("Unexpected state received"),
            },
            Err(_) => panic!("Failed to receive status"),
        }
    }

    #[tokio::test]
    async fn test_handle_error_sv2_protocol_error() {
        let (tx, rx) = bounded(1);
        let sender = Sender::Downstream(tx);
        let inner: [u8; 32] = rand::random();
        let value = inner.to_vec().try_into().unwrap();
        let error = JdsError::Sv2ProtocolError((
            12,
            Mining::OpenMiningChannelError(OpenMiningChannelError {
                request_id: 1,
                error_code: value,
            }),
        ));
        let error_string = "12";
        handle_error(&sender, error).await;
        match rx.recv().await {
            Ok(status) => match status.state {
                State::DownstreamInstanceDropped(e) => assert_eq!(e.to_string(), error_string),
                _ => panic!("Unexpected state received"),
            },
            Err(_) => panic!("Failed to receive status"),
        }
    }
}
