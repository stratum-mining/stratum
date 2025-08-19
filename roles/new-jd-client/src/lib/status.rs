use tracing::{debug, error, warn};

use crate::error::JDCError;

#[derive(Debug, Clone)]
pub enum StatusSender {
    Downstream {
        downstream_id: u32,
        tx: async_channel::Sender<Status>,
    },
    TemplateReceiver(async_channel::Sender<Status>),
    ChannelManager(async_channel::Sender<Status>),
    Upstream(async_channel::Sender<Status>),
    JobDeclarator(async_channel::Sender<Status>),
}

#[derive(Debug, PartialEq, Eq)]
pub enum StatusType {
    Downstream(u32),
    TemplateReceiver,
    ChannelManager,
    Upstream,
    JobDeclarator,
}

impl From<&StatusSender> for StatusType {
    fn from(value: &StatusSender) -> Self {
        match value {
            StatusSender::ChannelManager(_) => StatusType::ChannelManager,
            StatusSender::Downstream { downstream_id, tx } => {
                StatusType::Downstream(*downstream_id)
            }
            StatusSender::JobDeclarator(_) => StatusType::JobDeclarator,
            StatusSender::Upstream(_) => StatusType::Upstream,
            StatusSender::TemplateReceiver(_) => StatusType::TemplateReceiver,
        }
    }
}

impl StatusSender {
    /// Sends a [`Status`] update.
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

#[derive(Debug)]
pub enum State {
    DownstreamShutdown {
        downstream_id: u32,
        reason: JDCError,
    },
    TemplateReceiverShutdown(JDCError),
    JobDeclaratorShutdown(JDCError),
    ChannelManagerShutdown(JDCError),
    UpstreamShutdown(JDCError),
}

#[derive(Debug)]
pub struct Status {
    pub state: State,
}

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
            State::UpstreamShutdown(error)
        }
        StatusSender::JobDeclarator(_) => {
            warn!("Job declarator shutting down due to error: {error:?}");
            State::JobDeclaratorShutdown(error)
        }
    };

    if let Err(e) = sender.send(Status { state }).await {
        tracing::error!("Failed to send status update from {sender:?}: {e:?}");
    }
}

pub async fn handle_error(sender: &StatusSender, e: JDCError) {
    error!("Error in {:?}: {:?}", sender, e);
    send_status(sender, e).await;
}
