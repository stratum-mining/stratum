use crate::error::{self, Error};

#[derive(Debug)]
pub enum State<'a> {
    Shutdown(Error<'a>),
    Healthy(String),
}

#[derive(Debug)]
pub struct Status<'a> {
    pub state: State<'a>,
}

async fn send_status(
    sender: &async_channel::Sender<Status<'static>>,
    e: error::Error<'static>,
    outcome: error_handling::ErrorBranch,
) -> error_handling::ErrorBranch {
    match sender
        .send(Status {
            state: State::Shutdown(e),
        })
        .await
    {
        Ok(_) => {
            // implement error specific handling here instead of panicking
            outcome
        }
        Err(_e) => outcome,
    }
}

// this is called by `error_handling::handle_result!`
pub async fn handle_error(
    sender: &async_channel::Sender<Status<'static>>,
    e: error::Error<'static>,
) -> error_handling::ErrorBranch {
    tracing::error!("Error: {:?}", &e);
    match e {
        Error::VecToSlice32(_) => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        // Errors on bad CLI argument input.
        Error::BadCliArgs => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        // Errors on bad `serde_json` serialize/deserialize.
        Error::BadSerdeJson(_) => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        // Errors on bad `toml` deserialize.
        Error::BadTomlDeserialize(_) => {
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
        // SV1 protocol library error
        Error::V1Protocol(_) => send_status(sender, e, error_handling::ErrorBranch::Break).await,
        Error::SubprotocolMining(_) => {
            send_status(sender, e, error_handling::ErrorBranch::Break).await
        }
        // Locking Errors
        // PoisonLock(LockError<'a>),
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
    }
}
