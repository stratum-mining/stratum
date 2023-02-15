use crate::{common_properties::RequestIdMapper, errors::Error, parsers::Mining};
use core::convert::TryInto;
use mining_sv2::{
    CloseChannel, NewExtendedMiningJob, NewMiningJob, OpenExtendedMiningChannel,
    OpenExtendedMiningChannelSuccess, OpenMiningChannelError, OpenStandardMiningChannel,
    OpenStandardMiningChannelSuccess, Reconnect, SetCustomMiningJob, SetCustomMiningJobError,
    SetCustomMiningJobSuccess, SetExtranoncePrefix, SetGroupChannel, SetNewPrevHash, SetTarget,
    SubmitSharesError, SubmitSharesExtended, SubmitSharesStandard, SubmitSharesSuccess,
    UpdateChannel, UpdateChannelError,
};

use crate::{
    common_properties::{IsMiningDownstream, IsMiningUpstream},
    routing_logic::{MiningRouter, MiningRoutingLogic},
    selectors::DownstreamMiningSelector,
};

use super::SendTo_;

use crate::utils::Mutex;
use std::{fmt::Debug as D, sync::Arc};
use tracing::debug;

pub type SendTo<Remote> = SendTo_<Mining<'static>, Remote>;

pub enum SupportedChannelTypes {
    Standard,
    Extended,
    Group,
    // Non header only connection can support both group and extended channels.
    GroupAndExtended,
}

/// Connection-wide downtream's messages parser implemented by an upstream.
pub trait ParseDownstreamMiningMessages<
    Up: IsMiningUpstream<Self, Selector> + D,
    Selector: DownstreamMiningSelector<Self> + D,
    Router: MiningRouter<Self, Up, Selector>,
> where
    Self: IsMiningDownstream + Sized + D,
{
    fn get_channel_type(&self) -> SupportedChannelTypes;

    /// Used to parse and route SV2 mining messages from the downstream based on `message_type` and `payload`
    fn handle_message_mining(
        self_mutex: Arc<Mutex<Self>>,
        message_type: u8,
        payload: &mut [u8],
        routing_logic: MiningRoutingLogic<Self, Up, Selector, Router>,
    ) -> Result<SendTo<Up>, Error>
    where
        Self: IsMiningDownstream + Sized,
    {
        let (channel_type, is_work_selection_enabled, downstream_mining_data) = self_mutex
            .safe_lock(|self_| {
                (
                    self_.get_channel_type(),
                    self_.is_work_selection_enabled(),
                    self_.get_downstream_mining_data(),
                )
            })
            .map_err(|e| crate::Error::PoisonLock(e.to_string()))?;
        match (message_type, payload).try_into() {
            Ok(Mining::OpenStandardMiningChannel(mut m)) => {
                debug!("Received OpenStandardMiningChannel message");
                let upstream = match routing_logic {
                    MiningRoutingLogic::None => None,
                    MiningRoutingLogic::Proxy(r_logic) => {
                        debug!("Routing logic is Proxy");
                        let up = r_logic
                            .safe_lock(|r_logic| {
                                r_logic.on_open_standard_channel(
                                    self_mutex.clone(),
                                    &mut m,
                                    &downstream_mining_data,
                                )
                            })
                            .map_err(|e| crate::Error::PoisonLock(e.to_string()))?;
                        Some(up?)
                    }
                    // Variant just used for phantom data is ok to panic
                    MiningRoutingLogic::_P(_) => panic!(),
                };
                match channel_type {
                    SupportedChannelTypes::Standard => self_mutex
                        .safe_lock(|self_| self_.handle_open_standard_mining_channel(m, upstream))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Extended => Err(Error::UnexpectedMessage(message_type)),
                    SupportedChannelTypes::Group => self_mutex
                        .safe_lock(|self_| self_.handle_open_standard_mining_channel(m, upstream))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::GroupAndExtended => self_mutex
                        .safe_lock(|self_| self_.handle_open_standard_mining_channel(m, upstream))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                }
            }
            Ok(Mining::OpenExtendedMiningChannel(m)) => match channel_type {
                SupportedChannelTypes::Standard => Err(Error::UnexpectedMessage(message_type)),
                SupportedChannelTypes::Extended => {
                    debug!("Received OpenExtendedMiningChannel->Extended message");
                    self_mutex
                        .safe_lock(|self_| self_.handle_open_extended_mining_channel(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
                }
                SupportedChannelTypes::Group => Err(Error::UnexpectedMessage(message_type)),
                SupportedChannelTypes::GroupAndExtended => {
                    debug!("Received OpenExtendedMiningChannel->GroupAndExtended message");
                    self_mutex
                        .safe_lock(|self_| self_.handle_open_extended_mining_channel(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
                }
            },
            Ok(Mining::UpdateChannel(m)) => match channel_type {
                SupportedChannelTypes::Standard => {
                    debug!("Received UpdateChannel->Standard message");
                    self_mutex
                        .safe_lock(|self_| self_.handle_update_channel(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
                }
                SupportedChannelTypes::Extended => {
                    debug!("Received UpdateChannel->Extended message");
                    self_mutex
                        .safe_lock(|self_| self_.handle_update_channel(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
                }
                SupportedChannelTypes::Group => {
                    debug!("Received UpdateChannel->Group message");
                    self_mutex
                        .safe_lock(|self_| self_.handle_update_channel(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
                }
                SupportedChannelTypes::GroupAndExtended => {
                    debug!("Received UpdateChannel->GroupAndExtended message");
                    self_mutex
                        .safe_lock(|self_| self_.handle_update_channel(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
                }
            },
            Ok(Mining::SubmitSharesStandard(m)) => match channel_type {
                SupportedChannelTypes::Standard => {
                    debug!("Received SubmitSharesStandard->Standard message");
                    self_mutex
                        .safe_lock(|self_| self_.handle_submit_shares_standard(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
                }
                SupportedChannelTypes::Extended => Err(Error::UnexpectedMessage(message_type)),
                SupportedChannelTypes::Group => {
                    debug!("Received SubmitSharesStandard->Group message");
                    self_mutex
                        .safe_lock(|self_| self_.handle_submit_shares_standard(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
                }
                SupportedChannelTypes::GroupAndExtended => {
                    debug!("Received SubmitSharesStandard->GroupAndExtended message");
                    self_mutex
                        .safe_lock(|self_| self_.handle_submit_shares_standard(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
                }
            },
            Ok(Mining::SubmitSharesExtended(m)) => {
                debug!("Received SubmitSharesExtended message");
                match channel_type {
                    SupportedChannelTypes::Standard => Err(Error::UnexpectedMessage(message_type)),
                    SupportedChannelTypes::Extended => self_mutex
                        .safe_lock(|self_| self_.handle_submit_shares_extended(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Group => Err(Error::UnexpectedMessage(message_type)),
                    SupportedChannelTypes::GroupAndExtended => self_mutex
                        .safe_lock(|self_| self_.handle_submit_shares_extended(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                }
            }
            Ok(Mining::SetCustomMiningJob(m)) => {
                debug!("Received SetCustomMiningJob message");
                match (channel_type, is_work_selection_enabled) {
                    (SupportedChannelTypes::Extended, true) => self_mutex
                        .safe_lock(|self_| self_.handle_set_custom_mining_job(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    (SupportedChannelTypes::GroupAndExtended, true) => self_mutex
                        .safe_lock(|self_| self_.handle_set_custom_mining_job(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    _ => Err(Error::UnexpectedMessage(message_type)),
                }
            }
            Ok(_) => Err(Error::UnexpectedMessage(message_type)),
            Err(e) => Err(e),
        }
    }

    fn is_work_selection_enabled(&self) -> bool;

    fn handle_open_standard_mining_channel(
        &mut self,
        m: OpenStandardMiningChannel,
        up: Option<Arc<Mutex<Up>>>,
    ) -> Result<SendTo<Up>, Error>;

    fn handle_open_extended_mining_channel(
        &mut self,
        m: OpenExtendedMiningChannel,
    ) -> Result<SendTo<Up>, Error>;

    fn handle_update_channel(&mut self, m: UpdateChannel) -> Result<SendTo<Up>, Error>;

    fn handle_submit_shares_standard(
        &mut self,
        m: SubmitSharesStandard,
    ) -> Result<SendTo<Up>, Error>;

    fn handle_submit_shares_extended(
        &mut self,
        m: SubmitSharesExtended,
    ) -> Result<SendTo<Up>, Error>;

    fn handle_set_custom_mining_job(&mut self, m: SetCustomMiningJob) -> Result<SendTo<Up>, Error>;
}
/// Connection-wide upstream's messages parser implemented by a downstream.
pub trait ParseUpstreamMiningMessages<
    Down: IsMiningDownstream + D,
    Selector: DownstreamMiningSelector<Down> + D,
    Router: MiningRouter<Down, Self, Selector>,
> where
    Self: IsMiningUpstream<Down, Selector> + Sized + D,
{
    fn get_channel_type(&self) -> SupportedChannelTypes;

    fn get_request_id_mapper(&mut self) -> Option<Arc<Mutex<RequestIdMapper>>> {
        None
    }

    /// Used to parse and route SV2 mining messages from the upstream based on `message_type` and `payload`
    /// The implementor of DownstreamMining needs to pass a RequestIdMapper if needing to change the req id.
    /// Proxies likely would want to update a downstream req id to a new one as req id must be
    /// connection-wide unique
    fn handle_message_mining(
        self_mutex: Arc<Mutex<Self>>,
        message_type: u8,
        payload: &mut [u8],
        routing_logic: MiningRoutingLogic<Down, Self, Selector, Router>,
    ) -> Result<SendTo<Down>, Error> {
        let (channel_type, is_work_selection_enabled) = self_mutex
            .safe_lock(|s| (s.get_channel_type(), s.is_work_selection_enabled()))
            .map_err(|e| crate::Error::PoisonLock(e.to_string()))?;

        match (message_type, payload).try_into() {
            Ok(Mining::OpenStandardMiningChannelSuccess(mut m)) => {
                let remote = match routing_logic {
                    MiningRoutingLogic::None => None,
                    MiningRoutingLogic::Proxy(r_logic) => {
                        let up = r_logic
                            .safe_lock(|r_logic| {
                                r_logic.on_open_standard_channel_success(self_mutex.clone(), &mut m)
                            })
                            .map_err(|e| crate::Error::PoisonLock(e.to_string()))?;
                        Some(up?)
                    }
                    // Variant just used for phantom data is ok to panic
                    MiningRoutingLogic::_P(_) => panic!(),
                };
                match channel_type {
                    SupportedChannelTypes::Standard => self_mutex
                        .safe_lock(|s| s.handle_open_standard_mining_channel_success(m, remote))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Extended => Err(Error::UnexpectedMessage(message_type)),
                    SupportedChannelTypes::Group => self_mutex
                        .safe_lock(|s| s.handle_open_standard_mining_channel_success(m, remote))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::GroupAndExtended => self_mutex
                        .safe_lock(|s| s.handle_open_standard_mining_channel_success(m, remote))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                }
            }
            Ok(Mining::OpenExtendedMiningChannelSuccess(m)) => match channel_type {
                SupportedChannelTypes::Standard => Err(Error::UnexpectedMessage(message_type)),
                SupportedChannelTypes::Extended => self_mutex
                    .safe_lock(|s| s.handle_open_extended_mining_channel_success(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                SupportedChannelTypes::Group => Err(Error::UnexpectedMessage(message_type)),
                SupportedChannelTypes::GroupAndExtended => self_mutex
                    .safe_lock(|s| s.handle_open_extended_mining_channel_success(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
            },

            Ok(Mining::OpenMiningChannelError(m)) => match channel_type {
                SupportedChannelTypes::Standard => self_mutex
                    .safe_lock(|x| x.handle_open_mining_channel_error(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                SupportedChannelTypes::Extended => self_mutex
                    .safe_lock(|x| x.handle_open_mining_channel_error(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                SupportedChannelTypes::Group => self_mutex
                    .safe_lock(|x| x.handle_open_mining_channel_error(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                SupportedChannelTypes::GroupAndExtended => self_mutex
                    .safe_lock(|x| x.handle_open_mining_channel_error(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
            },

            Ok(Mining::UpdateChannelError(m)) => match channel_type {
                SupportedChannelTypes::Standard => self_mutex
                    .safe_lock(|x| x.handle_update_channel_error(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                SupportedChannelTypes::Extended => self_mutex
                    .safe_lock(|x| x.handle_update_channel_error(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                SupportedChannelTypes::Group => self_mutex
                    .safe_lock(|x| x.handle_update_channel_error(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                SupportedChannelTypes::GroupAndExtended => self_mutex
                    .safe_lock(|x| x.handle_update_channel_error(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
            },

            Ok(Mining::CloseChannel(m)) => match channel_type {
                SupportedChannelTypes::Standard => self_mutex
                    .safe_lock(|x| x.handle_close_channel(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                SupportedChannelTypes::Extended => self_mutex
                    .safe_lock(|x| x.handle_close_channel(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                SupportedChannelTypes::Group => self_mutex
                    .safe_lock(|x| x.handle_close_channel(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                SupportedChannelTypes::GroupAndExtended => self_mutex
                    .safe_lock(|x| x.handle_close_channel(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
            },

            Ok(Mining::SetExtranoncePrefix(m)) => match channel_type {
                SupportedChannelTypes::Standard => self_mutex
                    .safe_lock(|x| x.handle_set_extranonce_prefix(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                SupportedChannelTypes::Extended => self_mutex
                    .safe_lock(|x| x.handle_set_extranonce_prefix(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                SupportedChannelTypes::Group => self_mutex
                    .safe_lock(|x| x.handle_set_extranonce_prefix(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                SupportedChannelTypes::GroupAndExtended => self_mutex
                    .safe_lock(|x| x.handle_set_extranonce_prefix(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
            },
            Ok(Mining::SubmitSharesSuccess(m)) => match channel_type {
                SupportedChannelTypes::Standard => self_mutex
                    .safe_lock(|x| x.handle_submit_shares_success(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                SupportedChannelTypes::Extended => self_mutex
                    .safe_lock(|x| x.handle_submit_shares_success(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                SupportedChannelTypes::Group => self_mutex
                    .safe_lock(|x| x.handle_submit_shares_success(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                SupportedChannelTypes::GroupAndExtended => self_mutex
                    .safe_lock(|x| x.handle_submit_shares_success(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
            },
            Ok(Mining::SubmitSharesError(m)) => match channel_type {
                SupportedChannelTypes::Standard => self_mutex
                    .safe_lock(|x| x.handle_submit_shares_error(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                SupportedChannelTypes::Extended => self_mutex
                    .safe_lock(|x| x.handle_submit_shares_error(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                SupportedChannelTypes::Group => self_mutex
                    .safe_lock(|x| x.handle_submit_shares_error(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                SupportedChannelTypes::GroupAndExtended => self_mutex
                    .safe_lock(|x| x.handle_submit_shares_error(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
            },

            Ok(Mining::NewMiningJob(m)) => match channel_type {
                SupportedChannelTypes::Standard => self_mutex
                    .safe_lock(|x| x.handle_new_mining_job(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                SupportedChannelTypes::Extended => Err(Error::UnexpectedMessage(message_type)),
                SupportedChannelTypes::Group => Err(Error::UnexpectedMessage(message_type)),
                SupportedChannelTypes::GroupAndExtended => {
                    Err(Error::UnexpectedMessage(message_type))
                }
            },
            Ok(Mining::NewExtendedMiningJob(m)) => {
                debug!("Received new extended mining job");
                match channel_type {
                    SupportedChannelTypes::Standard => Err(Error::UnexpectedMessage(message_type)),
                    SupportedChannelTypes::Extended => self_mutex
                        .safe_lock(|x| x.handle_new_extended_mining_job(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Group => self_mutex
                        .safe_lock(|x| x.handle_new_extended_mining_job(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::GroupAndExtended => self_mutex
                        .safe_lock(|x| x.handle_new_extended_mining_job(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                }
            }
            Ok(Mining::SetNewPrevHash(m)) => {
                debug!("Received SetNewPrevHash");
                match channel_type {
                    SupportedChannelTypes::Standard => self_mutex
                        .safe_lock(|x| x.handle_set_new_prev_hash(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Extended => self_mutex
                        .safe_lock(|x| x.handle_set_new_prev_hash(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Group => self_mutex
                        .safe_lock(|x| x.handle_set_new_prev_hash(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::GroupAndExtended => self_mutex
                        .safe_lock(|x| x.handle_set_new_prev_hash(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                }
            }

            Ok(Mining::SetCustomMiningJobSuccess(m)) => {
                match (channel_type, is_work_selection_enabled) {
                    (SupportedChannelTypes::Extended, true) => self_mutex
                        .safe_lock(|x| x.handle_set_custom_mining_job_success(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    (SupportedChannelTypes::GroupAndExtended, true) => self_mutex
                        .safe_lock(|x| x.handle_set_custom_mining_job_success(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    _ => Err(Error::UnexpectedMessage(message_type)),
                }
            }

            Ok(Mining::SetCustomMiningJobError(m)) => {
                match (channel_type, is_work_selection_enabled) {
                    (SupportedChannelTypes::Extended, true) => self_mutex
                        .safe_lock(|x| x.handle_set_custom_mining_job_error(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    (SupportedChannelTypes::Group, true) => self_mutex
                        .safe_lock(|x| x.handle_set_custom_mining_job_error(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    (SupportedChannelTypes::GroupAndExtended, true) => self_mutex
                        .safe_lock(|x| x.handle_set_custom_mining_job_error(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    _ => Err(Error::UnexpectedMessage(message_type)),
                }
            }
            Ok(Mining::SetTarget(m)) => match channel_type {
                SupportedChannelTypes::Standard => self_mutex
                    .safe_lock(|x| x.handle_set_target(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                SupportedChannelTypes::Extended => self_mutex
                    .safe_lock(|x| x.handle_set_target(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                SupportedChannelTypes::Group => self_mutex
                    .safe_lock(|x| x.handle_set_target(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                SupportedChannelTypes::GroupAndExtended => self_mutex
                    .safe_lock(|x| x.handle_set_target(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
            },

            Ok(Mining::Reconnect(m)) => match channel_type {
                SupportedChannelTypes::Standard => self_mutex
                    .safe_lock(|x| x.handle_reconnect(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                SupportedChannelTypes::Extended => self_mutex
                    .safe_lock(|x| x.handle_reconnect(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                SupportedChannelTypes::Group => self_mutex
                    .safe_lock(|x| x.handle_reconnect(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                SupportedChannelTypes::GroupAndExtended => self_mutex
                    .safe_lock(|x| x.handle_reconnect(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
            },
            Ok(Mining::SetGroupChannel(m)) => match channel_type {
                SupportedChannelTypes::Standard => Err(Error::UnexpectedMessage(message_type)),
                SupportedChannelTypes::Extended => Err(Error::UnexpectedMessage(message_type)),
                SupportedChannelTypes::Group => self_mutex
                    .safe_lock(|x| x.handle_set_group_channel(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                SupportedChannelTypes::GroupAndExtended => self_mutex
                    .safe_lock(|x| x.handle_set_group_channel(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
            },
            Ok(_) => Err(Error::UnexpectedMessage(message_type)),
            Err(e) => Err(e),
        }
    }

    fn is_work_selection_enabled(&self) -> bool;

    fn handle_open_standard_mining_channel_success(
        &mut self,
        m: OpenStandardMiningChannelSuccess,
        remote: Option<Arc<Mutex<Down>>>,
    ) -> Result<SendTo<Down>, Error>;

    fn handle_open_extended_mining_channel_success(
        &mut self,
        m: OpenExtendedMiningChannelSuccess,
    ) -> Result<SendTo<Down>, Error>;

    fn handle_open_mining_channel_error(
        &mut self,
        m: OpenMiningChannelError,
    ) -> Result<SendTo<Down>, Error>;

    fn handle_update_channel_error(&mut self, m: UpdateChannelError)
        -> Result<SendTo<Down>, Error>;

    fn handle_close_channel(&mut self, m: CloseChannel) -> Result<SendTo<Down>, Error>;

    fn handle_set_extranonce_prefix(
        &mut self,
        m: SetExtranoncePrefix,
    ) -> Result<SendTo<Down>, Error>;

    fn handle_submit_shares_success(
        &mut self,
        m: SubmitSharesSuccess,
    ) -> Result<SendTo<Down>, Error>;

    fn handle_submit_shares_error(&mut self, m: SubmitSharesError) -> Result<SendTo<Down>, Error>;

    fn handle_new_mining_job(&mut self, m: NewMiningJob) -> Result<SendTo<Down>, Error>;

    fn handle_new_extended_mining_job(
        &mut self,
        m: NewExtendedMiningJob,
    ) -> Result<SendTo<Down>, Error>;

    fn handle_set_new_prev_hash(&mut self, m: SetNewPrevHash) -> Result<SendTo<Down>, Error>;

    fn handle_set_custom_mining_job_success(
        &mut self,
        m: SetCustomMiningJobSuccess,
    ) -> Result<SendTo<Down>, Error>;

    fn handle_set_custom_mining_job_error(
        &mut self,
        m: SetCustomMiningJobError,
    ) -> Result<SendTo<Down>, Error>;

    fn handle_set_target(&mut self, m: SetTarget) -> Result<SendTo<Down>, Error>;

    fn handle_reconnect(&mut self, m: Reconnect) -> Result<SendTo<Down>, Error>;

    fn handle_set_group_channel(&mut self, _m: SetGroupChannel) -> Result<SendTo<Down>, Error> {
        Ok(SendTo::None(None))
    }
}
