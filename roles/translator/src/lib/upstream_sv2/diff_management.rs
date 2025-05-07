//! ## Upstream SV2 Difficulty Management
//!
//! This module contains logic for managing difficulty and hashrate updates
//! specifically for the upstream SV2 connection.
//!
//! It defines method for the [`Upstream`] struct
//! related to checking configuration intervals and sending
//! `UpdateChannel` messages to the upstream server
//! based on configured nominal hashrate changes.

use super::Upstream;

use super::super::{
    error::{Error::PoisonLock, ProxyResult},
    upstream_sv2::{EitherFrame, Message, StdFrame},
};
use binary_sv2::u256_from_int;
use roles_logic_sv2::{
    mining_sv2::UpdateChannel, parsers::Mining, utils::Mutex, Error as RolesLogicError,
};
use std::{sync::Arc, time::Duration};

impl Upstream {
    /// Attempts to update the upstream channel's nominal hashrate if the configured
    /// update interval has elapsed or if the nominal hashrate has changed
    pub(super) async fn try_update_hashrate(self_: Arc<Mutex<Self>>) -> ProxyResult<'static, ()> {
        let (channel_id_option, diff_mgmt, tx_frame, last_sent_hashrate) = self_
            .safe_lock(|u| {
                (
                    u.channel_id,
                    u.difficulty_config.clone(),
                    u.connection.sender.clone(),
                    u.last_sent_hashrate,
                )
            })
            .map_err(|_e| PoisonLock)?;

        let channel_id = channel_id_option.ok_or(super::super::error::Error::RolesSv2Logic(
            RolesLogicError::NotFoundChannelId,
        ))?;

        let (timeout, new_hashrate) = diff_mgmt
            .safe_lock(|d| (d.channel_diff_update_interval, d.channel_nominal_hashrate))
            .map_err(|_e| PoisonLock)?;

        let has_changed = Some(new_hashrate) != last_sent_hashrate;

        if has_changed {
            // Send UpdateChannel only if hashrate actually changed
            let update_channel = UpdateChannel {
                channel_id,
                nominal_hash_rate: new_hashrate,
                maximum_target: u256_from_int(u64::MAX),
            };
            let message = Message::Mining(Mining::UpdateChannel(update_channel));
            let either_frame: StdFrame = message.try_into()?;
            let frame: EitherFrame = either_frame.into();

            tx_frame.send(frame).await.map_err(|e| {
                super::super::error::Error::ChannelErrorSender(
                    super::super::error::ChannelSendError::General(e.to_string()),
                )
            })?;

            self_
                .safe_lock(|u| u.last_sent_hashrate = Some(new_hashrate))
                .map_err(|_e| PoisonLock)?;
        }

        // Always sleep, regardless of update
        tokio::time::sleep(Duration::from_secs(timeout as u64)).await;
        Ok(())
    }
}
