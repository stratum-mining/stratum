use crate::{downstream::Downstream, error::PoolError, utils::StdFrame};
use std::{convert::TryInto, sync::atomic::Ordering};
use stratum_apps::stratum_core::{
    common_messages_sv2::{
        has_requires_std_job, has_work_selection, SetupConnection, SetupConnectionSuccess,
    },
    handlers_sv2::HandleCommonMessagesFromClientAsync,
    parsers_sv2::AnyMessage,
};
use tracing::info;

impl HandleCommonMessagesFromClientAsync for Downstream {
    type Error = PoolError;

    type Output<'a> = ();

    async fn handle_setup_connection(
        &mut self,
        _client_id: Option<usize>,
        msg: SetupConnection<'_>,
    ) -> Result<Self::Output<'_>, Self::Error> {
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
        self.downstream_channel
            .downstream_sender
            .send(frame)
            .await?;

        Ok(())
    }
}
