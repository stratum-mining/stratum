use crate::JdcError;

#[derive(Debug)]
pub enum Sender {
    Downstream(async_channel::Sender<Status<'static>>),
    DownstreamListener(async_channel::Sender<Status<'static>>),
    Upstream(async_channel::Sender<Status<'static>>),
    TemplateReceiver(async_channel::Sender<Status<'static>>),
}

impl Sender {
    pub async fn send(
        &self,
        status: Status<'static>,
    ) -> Result<(), async_channel::SendError<Status<'_>>> {
        match self {
            Self::Downstream(inner) => inner.send(status).await,
            Self::DownstreamListener(inner) => inner.send(status).await,
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
            Self::Upstream(inner) => Self::Upstream(inner.clone()),
            Self::TemplateReceiver(inner) => Self::TemplateReceiver(inner.clone()),
        }
    }
}

#[derive(Debug)]
pub enum State<'a> {
    DownstreamShutdown(JdcError<'a>),
    UpstreamShutdown(JdcError<'a>),
    UpstreamRogue,
    Healthy(String),
}

#[derive(Debug)]
pub struct Status<'a> {
    pub state: State<'a>,
}

async fn send_status(
    sender: &Sender,
    e: JdcError<'static>,
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
        Sender::Upstream(tx) => {
            tx.send(Status {
                state: State::UpstreamShutdown(e),
            })
            .await
            .unwrap_or(());
        }
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

// this is called by `error_handling::handle_result!`
pub async fn handle_error(sender: &Sender, e: JdcError<'static>) -> error_handling::ErrorBranch {
    tracing::error!("Error: {:?}", &e);
    match e {
        JdcError::VecToSlice32(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        JdcError::ConfigError(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        // Errors from `binary_sv2` crate.
        JdcError::BinarySv2(_) => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        // Errors on bad noise handshake.
        JdcError::CodecNoise(_) => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        // Errors from `framing_sv2` crate.
        JdcError::FramingSv2(_) => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        // Errors on bad `TcpStream` connection.
        JdcError::Io(_) => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        // Errors on bad `String` to `int` conversion.
        JdcError::ParseInt(_) => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        // Errors from `roles_logic_sv2` crate.
        JdcError::RolesLogicSv2(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        JdcError::UpstreamIncoming(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        JdcError::SubprotocolMining(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        // Locking Errors
        JdcError::PoisonLock => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        // Channel Receiver Error
        JdcError::ChannelErrorReceiver(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        JdcError::TokioChannelErrorRecv(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        // Channel Sender Errors
        JdcError::ChannelErrorSender(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        JdcError::Uint256Conversion(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        JdcError::Infallible(_) => send_status(sender, e, error_handling::ErrorBranch::Break).await,
    }
}
