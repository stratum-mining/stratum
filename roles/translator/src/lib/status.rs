use crate::TProxyError;

#[derive(Debug)]
pub enum Sender {
    Downstream(async_channel::Sender<Status<'static>>),
    DownstreamListener(async_channel::Sender<Status<'static>>),
    Bridge(async_channel::Sender<Status<'static>>),
    Upstream(async_channel::Sender<Status<'static>>),
    TemplateReceiver(async_channel::Sender<Status<'static>>),
}

impl Sender {
    pub fn listener_to_connection(&self) -> Self {
        match self {
            Self::DownstreamListener(inner) => Self::Downstream(inner.clone()),
            _ => unreachable!(),
        }
    }

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

#[derive(Debug)]
pub enum State<'a> {
    DownstreamShutdown(TProxyError<'a>),
    BridgeShutdown(TProxyError<'a>),
    UpstreamShutdown(TProxyError<'a>),
    Healthy(String),
}

#[derive(Debug)]
pub struct Status<'a> {
    pub state: State<'a>,
}

async fn send_status(
    sender: &Sender,
    e: TProxyError<'static>,
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
pub async fn handle_error(sender: &Sender, e: TProxyError<'static>) -> error_handling::ErrorBranch {
    tracing::error!("Error: {:?}", &e);
    match e {
        TProxyError::ConfigError(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        TProxyError::VecToSlice32(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        // Errors on bad CLI argument input.
        TProxyError::BadCliArgs => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        // Errors on bad `serde_json` serialize/deserialize.
        TProxyError::BadSerdeJson(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        // Errors from `binary_sv2` crate.
        TProxyError::BinarySv2(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        // Errors on bad noise handshake.
        TProxyError::CodecNoise(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        // Errors from `framing_sv2` crate.
        TProxyError::FramingSv2(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        //If the pool sends the tproxy an invalid extranonce
        TProxyError::InvalidExtranonce(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        // Errors on bad `TcpStream` connection.
        TProxyError::Io(_) => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        // Errors on bad `String` to `int` conversion.
        TProxyError::ParseInt(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        // Errors from `roles_logic_sv2` crate.
        TProxyError::RolesLogicSv2(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        TProxyError::UpstreamIncoming(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        // SV1 protocol library error
        TProxyError::V1Protocol(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        TProxyError::SubprotocolMining(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        // Locking Errors
        TProxyError::PoisonLock => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        // Channel Receiver Error
        TProxyError::ChannelErrorReceiver(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        TProxyError::TokioChannelErrorRecv(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        // Channel Sender Errors
        TProxyError::ChannelErrorSender(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        TProxyError::Uint256Conversion(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        TProxyError::SetDifficultyToMessage(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        TProxyError::Infallible(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        TProxyError::Sv2ProtocolError(ref inner) => {
            match inner {
                // dont notify main thread just continue
                roles_logic_sv2::parsers::Mining::SubmitSharesError(_) => {
                    error_handling::ErrorBranch::Continue
                }
                _ => send_status(sender, e, error_handling::ErrorBranch::Break).await,
            }
        }
        TProxyError::TargetError(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Continue).await
        }
        TProxyError::Sv1MessageTooLong => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
    }
}
