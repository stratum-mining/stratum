use crate::error::{self, Error};

#[derive(Debug)]
pub enum Sender {
    Downstream(async_channel::Sender<Status<'static>>),
    DownstreamListener(async_channel::Sender<Status<'static>>),
    Bridge(async_channel::Sender<Status<'static>>),
    Upstream(async_channel::Sender<Status<'static>>),
    TemplateReceiver(async_channel::Sender<Status<'static>>),
    DownstreamTokio(tokio::sync::mpsc::UnboundedSender<Status<'static>>),
    DownstreamListenerTokio(tokio::sync::mpsc::UnboundedSender<Status<'static>>),
    BridgeTokio(tokio::sync::mpsc::UnboundedSender<Status<'static>>),
    UpstreamTokio(tokio::sync::mpsc::UnboundedSender<Status<'static>>),
    TemplateReceiverTokio(tokio::sync::mpsc::UnboundedSender<Status<'static>>),
}

#[derive(Debug)]
pub enum ErrorS {
    AsyncError(async_channel::SendError<Status<'static>>),
    TokioError(tokio::sync::mpsc::error::SendError<Status<'static>>),
}

impl Sender {
    pub fn listener_to_connection(&self) -> Self {
        match self {
            Self::DownstreamListener(inner) => Self::Downstream(inner.clone()),
            Self::DownstreamListenerTokio(inner) => Self::DownstreamTokio(inner.clone()),
            _ => unreachable!(),
        }
    }

    pub async fn send(&self, status: Status<'static>) -> Result<(), ErrorS> {
        match self {
            Self::Downstream(inner) => inner.send(status).await.map_err(|e| ErrorS::AsyncError(e)),
            Self::DownstreamListener(inner) => {
                inner.send(status).await.map_err(|e| ErrorS::AsyncError(e))
            }
            Self::Bridge(inner) => inner.send(status).await.map_err(|e| ErrorS::AsyncError(e)),
            Self::Upstream(inner) => inner.send(status).await.map_err(|e| ErrorS::AsyncError(e)),
            Self::TemplateReceiver(inner) => {
                inner.send(status).await.map_err(|e| ErrorS::AsyncError(e))
            }
            Self::UpstreamTokio(inner) => inner.send(status).map_err(|e| ErrorS::TokioError(e)),
            Self::BridgeTokio(inner) => inner.send(status).map_err(|e| ErrorS::TokioError(e)),
            Self::DownstreamListenerTokio(inner) => {
                inner.send(status).map_err(|e| ErrorS::TokioError(e))
            }
            Self::DownstreamTokio(inner) => inner.send(status).map_err(|e| ErrorS::TokioError(e)),
            Self::TemplateReceiverTokio(inner) => {
                inner.send(status).map_err(|e| ErrorS::TokioError(e))
            }
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
            Self::UpstreamTokio(inner) => Self::UpstreamTokio(inner.clone()),
            Self::BridgeTokio(inner) => Self::BridgeTokio(inner.clone()),
            Self::DownstreamListenerTokio(inner) => Self::DownstreamListenerTokio(inner.clone()),
            Self::DownstreamTokio(inner) => Self::DownstreamTokio(inner.clone()),
            Self::TemplateReceiverTokio(inner) => Self::DownstreamTokio(inner.clone()),
        }
    }
}

#[derive(Debug)]
pub enum State<'a> {
    DownstreamShutdown(Error<'a>),
    BridgeShutdown(Error<'a>),
    UpstreamShutdown(Error<'a>),
    UpstreamTryReconnect(Error<'a>),
    Healthy(String),
}

#[derive(Debug)]
pub struct Status<'a> {
    pub state: State<'a>,
}

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
        Sender::UpstreamTokio(tx) => match e {
            Error::ChannelErrorReceiver(_) => {
                tx.send(Status {
                    state: State::UpstreamTryReconnect(e),
                })
                .unwrap_or(());
            }
            _ => {
                tx.send(Status {
                    state: State::UpstreamShutdown(e),
                })
                .unwrap_or(());
            }
        },
        Sender::BridgeTokio(tx) => {
            tx.send(Status {
                state: State::BridgeShutdown(e),
            })
            .unwrap_or(());
        }
        Sender::DownstreamTokio(tx) => {
            tx.send(Status {
                state: State::Healthy(e.to_string()),
            })
            .unwrap_or(());
        }
        Sender::DownstreamListenerTokio(tx) => {
            tx.send(Status {
                state: State::DownstreamShutdown(e),
            })
            .unwrap_or(());
        }
        Sender::TemplateReceiverTokio(tx) => {
            tx.send(Status {
                state: State::UpstreamShutdown(e),
            })
            .unwrap_or(());
        }
    }
    outcome
}

// this is called by `error_handling::handle_result!`
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
        Error::Uint256Conversion(_) => {
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
