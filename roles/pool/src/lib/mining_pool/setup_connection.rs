//! ## Setup Connection Handler Module
//! Handles the setup connection handshake with the Downstream.
//!
//! [`SetupConnectionHandler`] builds and receives a `SetupConnection` message,
//! processes the response, and implements `ParseCommonMessagesFromDownstream` for
//! handling common downstream messages.
use super::super::{
    error::{PoolError, PoolResult},
    mining_pool::{EitherFrame, StdFrame},
};
use async_channel::{Receiver, Sender};
use std::{convert::TryInto, net::SocketAddr, sync::Arc};
use stratum_common::roles_logic_sv2::{
    self,
    common_messages_sv2::{
        has_requires_std_job, has_work_selection, SetupConnection, SetupConnectionSuccess,
    },
    errors::Error,
    handlers::common::ParseCommonMessagesFromDownstream,
    parsers_sv2::{AnyMessage, CommonMessages},
    utils::Mutex,
};
use tracing::{debug, error, info};

/// Handles the `SetupConnection` message for downstream connections.
pub struct SetupConnectionHandler {
    // Whether only block headers are required for this connection.
    header_only: Option<bool>,
}

impl Default for SetupConnectionHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl SetupConnectionHandler {
    /// Creates a new `SetupConnectionHandler` instance.
    pub fn new() -> Self {
        Self { header_only: None }
    }

    /// Handles the `SetupConnection` message from a downstream connection.
    pub async fn setup(
        self_: Arc<Mutex<Self>>,
        receiver: &mut Receiver<EitherFrame>,
        sender: &mut Sender<EitherFrame>,
        address: SocketAddr,
    ) -> PoolResult<(bool, bool)> {
        // read stdFrame from receiver

        let mut incoming: StdFrame = match receiver.recv().await {
            Ok(EitherFrame::Sv2(s)) => {
                debug!("Got sv2 message: {:?}", s);
                s
            }
            Ok(EitherFrame::HandShake(s)) => {
                error!(
                    "Got unexpected handshake message from upstream: {:?} at {}",
                    s, address
                );
                panic!()
            }
            Err(e) => {
                error!("Error receiving message: {:?}", e);
                return Err(Error::NoDownstreamsConnected.into());
            }
        };

        let message_type = incoming
            .get_header()
            .ok_or_else(|| PoolError::Custom(String::from("No header set")))?
            .msg_type();
        let payload = incoming.payload();
        let response = ParseCommonMessagesFromDownstream::handle_message_common(
            self_.clone(),
            message_type,
            payload,
        )?;

        let message = response.into_message().ok_or(PoolError::RolesLogic(
            roles_logic_sv2::Error::NoDownstreamsConnected,
        ))?;

        let sv2_frame: StdFrame = AnyMessage::Common(message.clone()).try_into()?;
        let sv2_frame = sv2_frame.into();
        sender.send(sv2_frame).await?;
        self_.safe_lock(|s| s.header_only)?;

        match message {
            CommonMessages::SetupConnectionSuccess(m) => {
                debug!("Sent back SetupConnectionSuccess: {:?}", m);
                Ok((has_requires_std_job(m.flags), has_work_selection(m.flags)))
            }
            _ => panic!(),
        }
    }
}

impl ParseCommonMessagesFromDownstream for SetupConnectionHandler {
    // Handles the specific SetupConnection message received from the downstream.
    //
    // Returns
    // - `Ok(SendTo::RelayNewMessageToRemote)` - Containing either `SetupConnectionSuccess`.
    fn handle_setup_connection(
        &mut self,
        incoming: SetupConnection,
    ) -> Result<roles_logic_sv2::handlers::common::SendTo, Error> {
        info!(
            "Received `SetupConnection`: version={}, flags={:b}",
            incoming.min_version, incoming.flags
        );
        use roles_logic_sv2::handlers::common::SendTo;
        let header_only = incoming.requires_standard_job();
        debug!("Handling setup connection: header_only: {}", header_only);
        self.header_only = Some(header_only);
        Ok(SendTo::RelayNewMessageToRemote(
            Arc::new(Mutex::new(())),
            CommonMessages::SetupConnectionSuccess(SetupConnectionSuccess {
                flags: incoming.flags,
                used_version: 2,
            }),
        ))
    }
}
