use super::Upstream;

use crate::{
    error::Error::PoisonLock,
    upstream_sv2::{EitherFrame, Message, StdFrame},
    utils::is_outdated,
    ProxyResult,
};
use binary_sv2::u256_from_int;
use roles_logic_sv2::{
    mining_sv2::UpdateChannel, parsers::Mining, utils::Mutex, Error as RolesLogicError,
};
use std::sync::Arc;
use tracing::debug;

impl Upstream {
    /// this function checks if the elapsed time since the last update has surpassed the config
    pub(super) async fn try_update_hashrate(self_: Arc<Mutex<Self>>) -> ProxyResult<'static, ()> {
        let (channel_id_option, diff_mgmt, tx_frame) = self_
            .safe_lock(|u| {
                (
                    u.channel_id,
                    u.difficulty_config.clone(),
                    u.connection.sender.clone(),
                )
            })
            .map_err(|_e| PoisonLock)?;
        // dont run this if we shouldnt be aggregating hashrate
        let should_aggregate = diff_mgmt
            .safe_lock(|d| d.should_aggregate)
            .map_err(|_e| PoisonLock)?;
        if !should_aggregate {
            return Ok(());
        }

        let channel_id = channel_id_option.ok_or(crate::Error::RolesSv2Logic(
            RolesLogicError::NotFoundChannelId,
        ))?;

        let should_update_channel_option = diff_mgmt
            .safe_lock(|c| {
                if c.actual_nominal_hashrate == 0.0 {
                    c.actual_nominal_hashrate = c.channel_nominal_hashrate;
                }

                if is_outdated(c.timestamp_of_last_update, c.channel_diff_update_interval) {
                    c.channel_nominal_hashrate = c.actual_nominal_hashrate;
                    return Some(c.channel_nominal_hashrate);
                }
                None
            })
            .map_err(|_e| PoisonLock)?;

        if let Some(new_hashrate) = should_update_channel_option {
            // UPDATE CHANNEL
            let update_channel = UpdateChannel {
                channel_id,
                nominal_hash_rate: new_hashrate,
                maximum_target: u256_from_int(u64::MAX),
            };
            let message = Message::Mining(Mining::UpdateChannel(update_channel));
            debug!("{:?}", &message);
            let either_frame: StdFrame = message.try_into()?;
            let frame: EitherFrame = either_frame.try_into()?;

            tx_frame.send(frame).await.map_err(|e| {
                crate::Error::ChannelErrorSender(crate::error::ChannelSendError::General(
                    e.to_string(),
                ))
            })?;
        }
        Ok(())
    }
}
