//! Status reporting and error propagation Utility.
//!
//! This module provides mechanisms for communicating shutdown events and
//! component state changes across the system. Each component (downstream,
//! upstream, job declarator, template receiver, channel manager) can send
//! and receive status updates via typed channels. Errors are automatically
//! converted into shutdown signals, allowing coordinated teardown of tasks.

use tracing::{debug, error, warn};

use crate::error::JDCError;

/// Sender type for propagating status updates from different system components.
#[derive(Debug, Clone)]
pub enum StatusSender {
    /// Status updates from a specific downstream connection.
    Downstream {
        downstream_id: u32,
        tx: async_channel::Sender<Status>,
    },
    /// Status updates from the template receiver.
    TemplateReceiver(async_channel::Sender<Status>),
    /// Status updates from the channel manager.
    ChannelManager(async_channel::Sender<Status>),
    /// Status updates from the upstream.
    Upstream(async_channel::Sender<Status>),
    /// Status updates from the job declarator.
    JobDeclarator(async_channel::Sender<Status>),
}

/// High-level identifier of a component type that can send status updates.
#[derive(Debug, PartialEq, Eq)]
pub enum StatusType {
    /// A downstream connection identified by its ID.
    Downstream(u32),
    /// The template receiver component.
    TemplateReceiver,
    /// The channel manager component.
    ChannelManager,
    /// The upstream component.
    Upstream,
    /// The job declarator component.
    JobDeclarator,
}

impl From<&StatusSender> for StatusType {
    fn from(value: &StatusSender) -> Self {
        match value {
            StatusSender::ChannelManager(_) => StatusType::ChannelManager,
            StatusSender::Downstream {
                downstream_id,
                tx: _,
            } => StatusType::Downstream(*downstream_id),
            StatusSender::JobDeclarator(_) => StatusType::JobDeclarator,
            StatusSender::Upstream(_) => StatusType::Upstream,
            StatusSender::TemplateReceiver(_) => StatusType::TemplateReceiver,
        }
    }
}

impl StatusSender {
    /// Sends a status update for the associated component.
    pub async fn send(&self, status: Status) -> Result<(), async_channel::SendError<Status>> {
        match self {
            Self::Downstream { downstream_id, tx } => {
                debug!(
                    "Sending status from Downstream [{}]: {:?}",
                    downstream_id, status.state
                );
                tx.send(status).await
            }
            Self::TemplateReceiver(tx) => {
                debug!("Sending status from TemplateReceiver: {:?}", status.state);
                tx.send(status).await
            }
            Self::ChannelManager(tx) => {
                debug!("Sending status from ChannelManager: {:?}", status.state);
                tx.send(status).await
            }
            Self::Upstream(tx) => {
                debug!("Sending status from Upstream: {:?}", status.state);
                tx.send(status).await
            }
            Self::JobDeclarator(tx) => {
                debug!("Sending status from JobDeclarator: {:?}", status.state);
                tx.send(status).await
            }
        }
    }
}

/// Represents the state of a component, typically triggered by an error or shutdown event.
#[derive(Debug)]
pub enum State {
    /// A downstream connection has shut down with a reason.
    DownstreamShutdown {
        downstream_id: u32,
        reason: JDCError,
    },
    /// Template receiver has shut down with a reason.
    TemplateReceiverShutdown(JDCError),
    /// Job declarator has shut down during fallback with a reason.
    JobDeclaratorShutdownFallback(JDCError),
    /// Channel manager has shut down with a reason.
    ChannelManagerShutdown(JDCError),
    /// Upstream has shut down during fallback with a reason.
    UpstreamShutdownFallback(JDCError),
}

/// Wrapper around a component’s state, sent as status updates across the system.
#[derive(Debug)]
pub struct Status {
    /// The current state being reported.
    pub state: State,
}

/// Sends a shutdown status for the given component, logging the error cause.
async fn send_status(sender: &StatusSender, error: JDCError) {
    let state = match sender {
        StatusSender::Downstream { downstream_id, .. } => {
            warn!("Downstream [{downstream_id}] shutting down due to error: {error:?}");
            State::DownstreamShutdown {
                downstream_id: *downstream_id,
                reason: error,
            }
        }
        StatusSender::TemplateReceiver(_) => {
            warn!("Template Receiver shutting down due to error: {error:?}");
            State::TemplateReceiverShutdown(error)
        }
        StatusSender::ChannelManager(_) => {
            warn!("ChannelManager shutting down due to error: {error:?}");
            State::ChannelManagerShutdown(error)
        }
        StatusSender::Upstream(_) => {
            warn!("Upstream shutting down due to error: {error:?}");
            State::UpstreamShutdownFallback(error)
        }
        StatusSender::JobDeclarator(_) => {
            warn!("Job declarator shutting down due to error: {error:?}");
            State::JobDeclaratorShutdownFallback(error)
        }
    };

    if let Err(e) = sender.send(Status { state }).await {
        tracing::error!("Failed to send status update from {sender:?}: {e:?}");
    }
}

/// Logs an error and propagates a corresponding shutdown status for the component.
pub async fn handle_error(sender: &StatusSender, e: JDCError) {
    error!("Error in {:?}: {:?}", sender, e);
    send_status(sender, e).await;
}
