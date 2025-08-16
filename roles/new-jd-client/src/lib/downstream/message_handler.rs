use crate::{downstream::Downstream, utils::StdFrame};
use std::convert::TryInto;
use stratum_common::roles_logic_sv2::{
    codec_sv2::{self, Frame},
    common_messages_sv2::{
        has_work_selection, SetupConnection, SetupConnectionError, SetupConnectionSuccess,
    },
    handlers_sv2::{HandleCommonMessagesFromClientAsync, HandlerError as Error},
    parsers_sv2::{AnyMessage, MiningDeviceMessages},
};
use tracing::info;

impl HandleCommonMessagesFromClientAsync for Downstream {
    async fn handle_setup_connection(&mut self, msg: SetupConnection<'_>) -> Result<(), Error> {
        info!(
            "Received `SetupConnection`: version={}, flags={:b}",
            msg.min_version, msg.flags
        );
        if has_work_selection(msg.flags) {
            info!("We ourselves provide with custom job, you just go and mine");
            let response = SetupConnectionError {
                flags: msg.flags ^ 0b0000_0000_0000_0010,
                error_code: "work-selection-not-allowed"
                    .to_string()
                    .try_into()
                    .expect("error code must be valid string"),
            };
            let frame: StdFrame = AnyMessage::Common(response.into_static().into())
                .try_into()
                .unwrap();
            self.downstream_channel.outbound_tx.send(frame.into()).await;

            return Ok(());
        }
        let response = SetupConnectionSuccess {
            used_version: 2,
            flags: 0b0000_0000_0000_0010,
        };
        let frame: StdFrame = AnyMessage::Common(response.into_static().into())
            .try_into()
            .unwrap();
        self.downstream_channel.outbound_tx.send(frame.into()).await;

        Ok(())
    }
}
