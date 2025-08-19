use std::convert::TryFrom;
use stratum_common::roles_logic_sv2::{
    codec_sv2::binary_sv2::{u256_from_int, Str0255, U256},
    common_messages_sv2::{
        ChannelEndpointChanged, Protocol, Reconnect, SetupConnection, SetupConnectionError,
        SetupConnectionSuccess,
    },
    handlers_sv2::{HandleCommonMessagesFromServerAsync, HandlerError as Error},
    mining_sv2::OpenExtendedMiningChannel,
    parsers_sv2::{AnyMessage, Mining},
};
use tracing::info;

use crate::{error::JDCError, jd_mode::set_jd_mode, upstream::Upstream, utils::StdFrame};

impl HandleCommonMessagesFromServerAsync for Upstream {
    async fn handle_setup_connection_success(
        &mut self,
        msg: SetupConnectionSuccess,
    ) -> Result<(), Error> {
        info!(
            "Received `SetupConnectionSuccess` from Pool: version={}, flags={:b}",
            msg.used_version, msg.flags
        );

        Ok(())
    }

    async fn handle_channel_endpoint_changed(
        &mut self,
        msg: ChannelEndpointChanged,
    ) -> Result<(), Error> {
        info!("Received {msg:#?}");
        Ok(())
    }

    async fn handle_reconnect(&mut self, msg: Reconnect<'_>) -> Result<(), Error> {
        info!("Received {msg:#?}");
        Ok(())
    }

    async fn handle_setup_connection_error(
        &mut self,
        msg: SetupConnectionError<'_>,
    ) -> Result<(), Error> {
        info!("Received {msg:#?}");
        Ok(())
    }
}
