//! ## Status Reporting System
//!
//! This module provides a centralized way for components of the Translator to report
//! health updates, shutdown reasons, or fatal errors to the main runtime loop.
//!
//! Each task wraps its report in a [`Status`] and sends it over an async channel,
//! tagged with a [`Sender`] variant that identifies the source subsystem.

use tracing::error;

use crate::error::TproxyError;

/// Identifies the component that originated a [`Status`] update.
///
/// Each variant contains a channel to the main coordinator, and optionally a component ID
/// (e.g. a downstream connection ID).
#[derive(Debug, Clone)]
pub enum StatusSender {
    /// A specific downstream connection.
    Downstream {
        downstream_id: u32,
        tx: async_channel::Sender<Status>,
    },
    /// The SV1 server listener.
    Sv1Server(async_channel::Sender<Status>),
    /// The SV2 <-> SV1 bridge manager.
    ChannelManager(async_channel::Sender<Status>),
    /// The upstream SV2 connection handler.
    Upstream(async_channel::Sender<Status>),
}

impl StatusSender {
    /// Sends a [`Status`] update.
    pub async fn send(&self, status: Status) -> Result<(), async_channel::SendError<Status>> {
        match self {
            Self::Downstream { tx, .. } => tx.send(status).await,
            Self::Sv1Server(tx) => tx.send(status).await,
            Self::ChannelManager(tx) => tx.send(status).await,
            Self::Upstream(tx) => tx.send(status).await,
        }
    }
}

/// The type of event or error being reported by a component.
#[derive(Debug)]
pub enum State {
    /// Downstream task exited or encountered an unrecoverable error.
    DownstreamShutdown {
        downstream_id: u32,
        reason: TproxyError,
    },
    /// SV1 server listener exited unexpectedly.
    Sv1ServerShutdown(TproxyError),
    /// Channel manager shut down (SV2 bridge manager).
    ChannelManagerShutdown(TproxyError),
    /// Upstream SV2 connection closed or failed.
    UpstreamShutdown(TproxyError),
}

/// A message reporting the current [`State`] of a component.
#[derive(Debug)]
pub struct Status {
    pub state: State,
}

/// Constructs and sends a [`Status`] update based on the [`Sender`] and error context.
async fn send_status(sender: &StatusSender, error: TproxyError) {
    let state = match sender {
        StatusSender::Downstream { downstream_id, .. } => State::DownstreamShutdown {
            downstream_id: *downstream_id,
            reason: error,
        },
        StatusSender::Sv1Server(_) => State::Sv1ServerShutdown(error),
        StatusSender::ChannelManager(_) => State::ChannelManagerShutdown(error),
        StatusSender::Upstream(_) => State::UpstreamShutdown(error),
    };

    let _ = sender.send(Status { state }).await;
}

/// Centralized error dispatcher for the Translator.
///
/// Used by the `handle_result!` macro across the codebase.
/// Decides whether the task should `Continue` or `Break` based on the error type and source.
pub async fn handle_error(sender: &StatusSender, e: TproxyError) {
    error!("Error: {:?}", &e);
    send_status(sender, e).await;
}
