use super::error::{self, Error};

#[derive(Debug)]
pub enum Sender {
    DownstreamTokio(tokio::sync::mpsc::UnboundedSender<Status>),
    TemplateReceiverTokio(tokio::sync::mpsc::UnboundedSender<Status>),
    UpstreamTokio(tokio::sync::mpsc::UnboundedSender<Status>),
}

#[allow(dead_code)]
#[derive(Debug)]
pub enum ErrorS {
    TokioError(tokio::sync::mpsc::error::SendError<Status>),
}

impl Sender {
    pub async fn send(&self, status: Status) -> Result<(), ErrorS> {
        match self {
            Self::DownstreamTokio(inner) => inner.send(status).map_err(|e| ErrorS::TokioError(e)),
            Self::TemplateReceiverTokio(inner) => {
                inner.send(status).map_err(|e| ErrorS::TokioError(e))
            }
            Self::UpstreamTokio(inner) => inner.send(status).map_err(|e| ErrorS::TokioError(e)),
        }
    }
}

impl Clone for Sender {
    fn clone(&self) -> Self {
        match self {
            Self::DownstreamTokio(inner) => Self::DownstreamTokio(inner.clone()),
            Self::TemplateReceiverTokio(inner) => Self::TemplateReceiverTokio(inner.clone()),
            Self::UpstreamTokio(inner) => Self::UpstreamTokio(inner.clone()),
        }
    }
}

#[derive(Debug)]
pub enum State {
    DownstreamShutdown(Error),
    UpstreamShutdown(Error),
    UpstreamRogue,
    Healthy(String),
}

#[derive(Debug)]
pub struct Status {
    pub state: State,
}

async fn send_status(
    sender: &Sender,
    e: error::Error,
    outcome: error_handling::ErrorBranch,
) -> error_handling::ErrorBranch {
    match sender {
        Sender::DownstreamTokio(tx) => {
            tx.send(Status {
                state: State::Healthy(e.to_string()),
            })
            .unwrap_or(());
        }
        Sender::TemplateReceiverTokio(tx) => {
            tx.send(Status {
                state: State::UpstreamShutdown(e),
            })
            .unwrap_or(());
        }
        Sender::UpstreamTokio(tx) => {
            tx.send(Status {
                state: State::UpstreamShutdown(e),
            })
            .unwrap_or(());
        }
    }
    outcome
}

// This is called by `error_handling::handle_result!`
pub async fn handle_error(
    sender: &Sender,
    e: error::Error,
) -> error_handling::ErrorBranch {
    tracing::error!("Error: {:?}", &e);
    match e {
        Error::VecToSlice32(_) => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        // Errors on bad CLI argument input.
        Error::BadCliArgs => send_status(sender, e, error_handling::ErrorBranch::Break).await,
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
        // Errors on bad `TcpStream` connection.
        Error::Io(_) => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        // Errors on bad `String` to `int` conversion.
        Error::ParseInt(_) => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        // Errors from `roles_logic_sv2` crate.
        Error::RolesSv2Logic(_) => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        Error::UpstreamIncoming(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        Error::SubprotocolMining(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        // Locking Errors
        Error::PoisonLock => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        Error::TokioChannelErrorRecv(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        // Channel Sender Errors
        Error::ChannelErrorSender(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        Error::Uint256Conversion(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        Error::Infallible(_) => send_status(sender, e, error_handling::ErrorBranch::Break).await,
    }
}
