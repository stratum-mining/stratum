use crate::{downstream::Downstream, error::JDCError, utils::StdFrame};
use std::convert::TryInto;
use stratum_common::roles_logic_sv2::{
    common_messages_sv2::{
        has_requires_std_job, has_work_selection, Protocol, SetupConnection, SetupConnectionError,
        SetupConnectionSuccess,
    },
    handlers_sv2::HandleCommonMessagesFromClientAsync,
    parsers_sv2::AnyMessage,
};
use tracing::info;

impl HandleCommonMessagesFromClientAsync for Downstream {
    type Error = JDCError;
    // Handles the initial [`SetupConnection`] message from a downstream client.
    //
    // This method validates that the connection request is compatible with the
    // supported mining protocol and feature set. The flow is:
    //
    // 1. Protocol validation
    //    - Only the `MiningProtocol` is supported.
    //    - If the client requests another protocol, the connection is rejected with a
    //      [`SetupConnectionError`] (`unsupported-protocol`).
    //
    // 2. Feature flag validation
    //    - Work selection (`work_selection`) is not allowed.
    //    - If requested, the connection is rejected with a [`SetupConnectionError`]
    //      (`unsupported-feature-flags`).
    //
    // 3. Standard job requirement
    //    - If the downstream sets the `requires_standard_job` flag, it is recorded in
    //      [`DownstreamData::require_std_job`].
    //
    // 4. Successful setup
    //    - If all validations pass, a [`SetupConnectionSuccess`] message is
    async fn handle_setup_connection(
        &mut self,
        msg: SetupConnection<'_>,
    ) -> Result<(), Self::Error> {
        info!("Received: {}", msg);

        if msg.protocol != Protocol::MiningProtocol {
            info!("Rejecting connection: SetupConnection asking for other protocols than mining protocol.");
            let response = SetupConnectionError {
                flags: 0,
                error_code: "unsupported-protocol"
                    .to_string()
                    .try_into()
                    .expect("error code must be valid string"),
            };
            let frame: StdFrame = AnyMessage::Common(response.into_static().into()).try_into()?;
            _ = self.downstream_channel.downstream_sender.send(frame).await;

            return Err(JDCError::Shutdown);
        }

        if has_work_selection(msg.flags) {
            info!("Rejecting: work selection not allowed.");
            let response = SetupConnectionError {
                flags: 0b0000_0000_0000_0010,
                error_code: "unsupported-feature-flags"
                    .to_string()
                    .try_into()
                    .expect("error code must be valid string"),
            };
            let frame: StdFrame = AnyMessage::Common(response.into_static().into())
                .try_into()
                .unwrap();
            _ = self.downstream_channel.downstream_sender.send(frame).await;

            return Err(JDCError::Shutdown);
        }

        if has_requires_std_job(msg.flags) {
            self.downstream_data
                .super_safe_lock(|data| data.require_std_job = true);
        }
        let response = SetupConnectionSuccess {
            used_version: 2,
            flags: msg.flags,
        };
        let frame: StdFrame = AnyMessage::Common(response.into_static().into()).try_into()?;

        _ = self.downstream_channel.downstream_sender.send(frame).await;

        Ok(())
    }
}
