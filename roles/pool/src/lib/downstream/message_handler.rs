use crate::{downstream::Downstream, error::PoolError, utils::StdFrame};
use std::{convert::TryInto, sync::atomic::Ordering};
use stratum_common::roles_logic_sv2::{
    common_messages_sv2::{
        has_requires_std_job, has_work_selection, Protocol, SetupConnection, SetupConnectionError,
        SetupConnectionSuccess,
    },
    handlers_sv2::HandleCommonMessagesFromClientAsync,
    parsers_sv2::AnyMessage,
};
use tracing::{debug, info};

impl HandleCommonMessagesFromClientAsync for Downstream {
    type Error = PoolError;

    async fn handle_setup_connection(
        &mut self,
        msg: SetupConnection<'_>,
    ) -> Result<(), Self::Error> {
        info!(
            "Received `SetupConnection`: version={}, flags={:b}",
            msg.min_version, msg.flags
        );

        self.requires_custom_work
            .store(has_work_selection(msg.flags), Ordering::SeqCst);
        self.requires_standard_jobs
            .store(has_requires_std_job(msg.flags), Ordering::SeqCst);

        let response = SetupConnectionSuccess {
            used_version: 2,
            flags: msg.flags,
        };
        let frame: StdFrame = AnyMessage::Common(response.into_static().into()).try_into()?;
        _ = self
            .downstream_channel
            .downstream_sender
            .send(frame)
            .await?;

        Ok(())
    }
}
