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
use const_sv2::*;
use std::{fmt::Debug as D, sync::Arc};
use tracing::{debug, error, info, trace};

pub type SendTo<Remote> = SendTo_<Mining<'static>, Remote>;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
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
        match Self::handle_message_mining_deserialized(
            self_mutex,
            (message_type, payload).try_into(),
            routing_logic,
        ) {
            Err(Error::UnexpectedMessage(0)) => Err(Error::UnexpectedMessage(message_type)),
            result => result,
        }
    }

    /// Used to route SV2 mining messages from the downstream
    fn handle_message_mining_deserialized(
        self_mutex: Arc<Mutex<Self>>,
        message: Result<Mining<'_>, Error>,
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
        match message {
            Ok(Mining::OpenStandardMiningChannel(mut m)) => {
                info!(
                    "Received OpenStandardMiningChannel from: {} with id: {}",
                    std::str::from_utf8(m.user_identity.as_ref()).unwrap_or("Unknown identity"),
                    m.get_request_id_as_u32()
                );
                debug!("OpenStandardMiningChannel: {:?}", m);
                // check user auth
                if !Self::is_downstream_authorized(self_mutex.clone(), &m.user_identity)? {
                    info!(
                        "On OpenStandardMiningChannel client not authorized: {:?}",
                        &m.user_identity
                    );
                    return Ok(SendTo::Respond(Mining::OpenMiningChannelError(
                        OpenMiningChannelError::new_unknown_user(m.get_request_id_as_u32()),
                    )));
                }
                let upstream = match routing_logic {
                    MiningRoutingLogic::None => None,
                    MiningRoutingLogic::Proxy(r_logic) => {
                        trace!("On OpenStandardMiningChannel r_logic is: {:?}", r_logic);
                        let up = r_logic
                            .safe_lock(|r_logic| {
                                r_logic.on_open_standard_channel(
                                    self_mutex.clone(),
                                    &mut m,
                                    &downstream_mining_data,
                                )
                            })
                            .map_err(|e| crate::Error::PoisonLock(e.to_string()))?;
                        trace!("On OpenStandardMiningChannel best candidate is: {:?}", up);
                        Some(up?)
                    }
                    // Variant just used for phantom data is ok to panic
                    MiningRoutingLogic::_P(_) => panic!("Must use either MiningRoutingLogic::None or MiningRoutingLogic::Proxy for `routing_logic` param"),
                };
                trace!(
                    "On OpenStandardMiningChannel channel type is: {:?}",
                    channel_type
                );
                match channel_type {
                    SupportedChannelTypes::Standard => self_mutex
                        .safe_lock(|self_| self_.handle_open_standard_mining_channel(m, upstream))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Extended => Err(Error::UnexpectedMessage(
                        MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL,
                    )),
                    SupportedChannelTypes::Group => self_mutex
                        .safe_lock(|self_| self_.handle_open_standard_mining_channel(m, upstream))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::GroupAndExtended => self_mutex
                        .safe_lock(|self_| self_.handle_open_standard_mining_channel(m, upstream))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                }
            }
            Ok(Mining::OpenExtendedMiningChannel(m)) => {
                info!(
                    "Received OpenExtendedMiningChannel from: {} with id: {}",
                    std::str::from_utf8(m.user_identity.as_ref()).unwrap_or("Unknown identity"),
                    m.get_request_id_as_u32()
                );
                debug!("OpenExtendedMiningChannel: {:?}", m);
                // check user auth
                if !Self::is_downstream_authorized(self_mutex.clone(), &m.user_identity)? {
                    info!(
                        "On OpenStandardMiningChannel client not authorized: {:?}",
                        &m.user_identity
                    );
                    return Ok(SendTo::Respond(Mining::OpenMiningChannelError(
                        OpenMiningChannelError::new_unknown_user(m.get_request_id_as_u32()),
                    )));
                };
                trace!(
                    "On OpenExtendedMiningChannel channel type is: {:?}",
                    channel_type
                );
                match channel_type {
                    SupportedChannelTypes::Standard => Err(Error::UnexpectedMessage(
                        MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL,
                    )),
                    SupportedChannelTypes::Extended => self_mutex
                        .safe_lock(|self_| self_.handle_open_extended_mining_channel(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Group => Err(Error::UnexpectedMessage(
                        MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL,
                    )),
                    SupportedChannelTypes::GroupAndExtended => self_mutex
                        .safe_lock(|self_| self_.handle_open_extended_mining_channel(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                }
            }
            Ok(Mining::UpdateChannel(m)) => match channel_type {
                SupportedChannelTypes::Standard => {
                    info!("Received UpdateChannel->Standard message");
                    self_mutex
                        .safe_lock(|self_| self_.handle_update_channel(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
                }
                SupportedChannelTypes::Extended => {
                    info!("Received UpdateChannel->Extended message");
                    self_mutex
                        .safe_lock(|self_| self_.handle_update_channel(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
                }
                SupportedChannelTypes::Group => {
                    info!("Received UpdateChannel->Group message");
                    self_mutex
                        .safe_lock(|self_| self_.handle_update_channel(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
                }
                SupportedChannelTypes::GroupAndExtended => {
                    info!("Received UpdateChannel->GroupAndExtended message");
                    self_mutex
                        .safe_lock(|self_| self_.handle_update_channel(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
                }
            },
            Ok(Mining::SubmitSharesStandard(m)) => match channel_type {
                SupportedChannelTypes::Standard => {
                    debug!("Received SubmitSharesStandard->Standard message");
                    trace!("SubmitSharesStandard {:?}", m);
                    self_mutex
                        .safe_lock(|self_| self_.handle_submit_shares_standard(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
                }
                SupportedChannelTypes::Extended => Err(Error::UnexpectedMessage(
                    MESSAGE_TYPE_SUBMIT_SHARES_STANDARD,
                )),
                SupportedChannelTypes::Group => {
                    debug!("Received SubmitSharesStandard->Group message");
                    trace!("SubmitSharesStandard {:?}", m);
                    self_mutex
                        .safe_lock(|self_| self_.handle_submit_shares_standard(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
                }
                SupportedChannelTypes::GroupAndExtended => {
                    debug!("Received SubmitSharesStandard->GroupAndExtended message");
                    trace!("SubmitSharesStandard {:?}", m);
                    self_mutex
                        .safe_lock(|self_| self_.handle_submit_shares_standard(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
                }
            },
            Ok(Mining::SubmitSharesExtended(m)) => {
                debug!("Received SubmitSharesExtended message");
                trace!("SubmitSharesExtended {:?}", m);
                match channel_type {
                    SupportedChannelTypes::Standard => Err(Error::UnexpectedMessage(
                        MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED,
                    )),
                    SupportedChannelTypes::Extended => self_mutex
                        .safe_lock(|self_| self_.handle_submit_shares_extended(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Group => Err(Error::UnexpectedMessage(
                        MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED,
                    )),
                    SupportedChannelTypes::GroupAndExtended => self_mutex
                        .safe_lock(|self_| self_.handle_submit_shares_extended(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                }
            }
            Ok(Mining::SetCustomMiningJob(m)) => {
                info!(
                    "Received SetCustomMiningJob message for channel: {}, with id: {}",
                    m.channel_id, m.request_id
                );
                debug!("SetCustomMiningJob: {:?}", m);
                match (channel_type, is_work_selection_enabled) {
                    (SupportedChannelTypes::Extended, true) => self_mutex
                        .safe_lock(|self_| self_.handle_set_custom_mining_job(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    (SupportedChannelTypes::GroupAndExtended, true) => self_mutex
                        .safe_lock(|self_| self_.handle_set_custom_mining_job(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    _ => Err(Error::UnexpectedMessage(MESSAGE_TYPE_SET_CUSTOM_MINING_JOB)),
                }
            }
            Ok(_) => Err(Error::UnexpectedMessage(0)),
            Err(e) => Err(e),
        }
    }

    fn is_work_selection_enabled(&self) -> bool;

    /// returns None if the user is authorized and Open
    fn is_downstream_authorized(
        _self_mutex: Arc<Mutex<Self>>,
        _user_identity: &binary_sv2::Str0255,
    ) -> Result<bool, Error> {
        Ok(true)
    }

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
        match Self::handle_message_mining_deserialized(
            self_mutex,
            (message_type, payload).try_into(),
            routing_logic,
        ) {
            Err(Error::UnexpectedMessage(0)) => Err(Error::UnexpectedMessage(message_type)),
            result => result,
        }
    }

    fn handle_message_mining_deserialized(
        self_mutex: Arc<Mutex<Self>>,
        message: Result<Mining, Error>,
        routing_logic: MiningRoutingLogic<Down, Self, Selector, Router>,
    ) -> Result<SendTo<Down>, Error> {
        let (channel_type, is_work_selection_enabled) = self_mutex
            .safe_lock(|s| (s.get_channel_type(), s.is_work_selection_enabled()))
            .map_err(|e| crate::Error::PoisonLock(e.to_string()))?;

        match message {
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
                    MiningRoutingLogic::_P(_) => panic!("Must use either MiningRoutingLogic::None or MiningRoutingLogic::Proxy for `routing_logic` param"),
                };
                match channel_type {
                    SupportedChannelTypes::Standard => self_mutex
                        .safe_lock(|s| s.handle_open_standard_mining_channel_success(m, remote))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Extended => Err(Error::UnexpectedMessage(
                        MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS,
                    )),
                    SupportedChannelTypes::Group => self_mutex
                        .safe_lock(|s| s.handle_open_standard_mining_channel_success(m, remote))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::GroupAndExtended => self_mutex
                        .safe_lock(|s| s.handle_open_standard_mining_channel_success(m, remote))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                }
            }
            Ok(Mining::OpenExtendedMiningChannelSuccess(m)) => {
                info!("Received OpenExtendedMiningChannelSuccess with request id: {} and channel id: {}", m.request_id, m.channel_id);
                debug!("OpenStandardMiningChannelSuccess: {:?}", m);
                match channel_type {
                    SupportedChannelTypes::Standard => Err(Error::UnexpectedMessage(
                        MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCES,
                    )),
                    SupportedChannelTypes::Extended => self_mutex
                        .safe_lock(|s| s.handle_open_extended_mining_channel_success(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Group => Err(Error::UnexpectedMessage(
                        MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCES,
                    )),
                    SupportedChannelTypes::GroupAndExtended => self_mutex
                        .safe_lock(|s| s.handle_open_extended_mining_channel_success(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                }
            }

            Ok(Mining::OpenMiningChannelError(m)) => {
                error!(
                    "Received OpenExtendedMiningChannelError with error code {}",
                    std::str::from_utf8(m.error_code.as_ref()).unwrap_or("unknown error code")
                );
                match channel_type {
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
                }
            }

            Ok(Mining::UpdateChannelError(m)) => {
                error!(
                    "Received UpdateChannelError with error code {}",
                    std::str::from_utf8(m.error_code.as_ref()).unwrap_or("unknown error code")
                );
                match channel_type {
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
                }
            }

            Ok(Mining::CloseChannel(m)) => {
                info!("Received CloseChannel for channel id: {}", m.channel_id);
                match channel_type {
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
                }
            }

            Ok(Mining::SetExtranoncePrefix(m)) => {
                info!(
                    "Received SetExtranoncePrefix for channel id: {}",
                    m.channel_id
                );
                debug!("SetExtranoncePrefix: {:?}", m);
                match channel_type {
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
                }
            }
            Ok(Mining::SubmitSharesSuccess(m)) => {
                debug!("Received SubmitSharesSuccess");
                trace!("SubmitSharesSuccess: {:?}", m);
                match channel_type {
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
                }
            }
            Ok(Mining::SubmitSharesError(m)) => {
                error!(
                    "Received SubmitSharesError with error code {}",
                    std::str::from_utf8(m.error_code.as_ref()).unwrap_or("unknown error code")
                );
                match channel_type {
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
                }
            }

            Ok(Mining::NewMiningJob(m)) => {
                info!(
                    "Received new mining job for channel id: {} with job id: {} is future: {}",
                    m.channel_id,
                    m.job_id,
                    m.is_future()
                );
                debug!("NewMiningJob: {:?}", m);
                match channel_type {
                    SupportedChannelTypes::Standard => self_mutex
                        .safe_lock(|x| x.handle_new_mining_job(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Extended => {
                        Err(Error::UnexpectedMessage(MESSAGE_TYPE_NEW_MINING_JOB))
                    }
                    SupportedChannelTypes::Group => {
                        Err(Error::UnexpectedMessage(MESSAGE_TYPE_NEW_MINING_JOB))
                    }
                    SupportedChannelTypes::GroupAndExtended => {
                        Err(Error::UnexpectedMessage(MESSAGE_TYPE_NEW_MINING_JOB))
                    }
                }
            }
            Ok(Mining::NewExtendedMiningJob(m)) => {
                info!("Received new extended mining job for channel id: {} with job id: {} is_future: {}",m.channel_id, m.job_id, m.is_future());
                debug!("NewExtendedMiningJob: {:?}", m);
                match channel_type {
                    SupportedChannelTypes::Standard => Err(Error::UnexpectedMessage(
                        MESSAGE_TYPE_NEW_EXTENDED_MINING_JOB,
                    )),
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
                info!(
                    "Received SetNewPrevHash channel id: {}, job id: {}",
                    m.channel_id, m.job_id
                );
                debug!("SetNewPrevHash: {:?}", m);
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
                info!(
                    "Received SetCustomMiningJobSuccess for channel id: {} for job id: {}",
                    m.channel_id, m.job_id
                );
                debug!("SetCustomMiningJobSuccess: {:?}", m);
                match (channel_type, is_work_selection_enabled) {
                    (SupportedChannelTypes::Extended, true) => self_mutex
                        .safe_lock(|x| x.handle_set_custom_mining_job_success(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    (SupportedChannelTypes::GroupAndExtended, true) => self_mutex
                        .safe_lock(|x| x.handle_set_custom_mining_job_success(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    _ => Err(Error::UnexpectedMessage(
                        MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_SUCCESS,
                    )),
                }
            }

            Ok(Mining::SetCustomMiningJobError(m)) => {
                error!(
                    "Received SetCustomMiningJobError with error code {}",
                    std::str::from_utf8(m.error_code.as_ref()).unwrap_or("unknown error code")
                );
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
                    _ => Err(Error::UnexpectedMessage(
                        MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_ERROR,
                    )),
                }
            }
            Ok(Mining::SetTarget(m)) => {
                info!("Received SetTarget for channel id: {}", m.channel_id);
                debug!("SetTarget: {:?}", m);
                match channel_type {
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
                }
            }

            Ok(Mining::Reconnect(m)) => {
                info!("Received Reconnect");
                debug!("Reconnect: {:?}", m);
                match channel_type {
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
                }
            }
            Ok(Mining::SetGroupChannel(m)) => {
                info!("Received SetGroupChannel");
                debug!("SetGroupChannel: {:?}", m);
                match channel_type {
                    SupportedChannelTypes::Standard => {
                        Err(Error::UnexpectedMessage(MESSAGE_TYPE_SET_GROUP_CHANNEL))
                    }
                    SupportedChannelTypes::Extended => {
                        Err(Error::UnexpectedMessage(MESSAGE_TYPE_SET_GROUP_CHANNEL))
                    }
                    SupportedChannelTypes::Group => self_mutex
                        .safe_lock(|x| x.handle_set_group_channel(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::GroupAndExtended => self_mutex
                        .safe_lock(|x| x.handle_set_group_channel(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                }
            }
            Ok(_) => Err(Error::UnexpectedMessage(0)),
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
