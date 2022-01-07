use crate::common_properties::RequestIdMapper;
use crate::errors::Error;
use crate::parsers::Mining;
use core::convert::TryInto;
use mining_sv2::{
    CloseChannel, NewExtendedMiningJob, NewMiningJob, OpenExtendedMiningChannel,
    OpenExtendedMiningChannelSuccess, OpenMiningChannelError, OpenStandardMiningChannel,
    OpenStandardMiningChannelSuccess, Reconnect, SetCustomMiningJob, SetCustomMiningJobError,
    SetCustomMiningJobSuccess, SetExtranoncePrefix, SetNewPrevHash, SetTarget, SubmitSharesError,
    SubmitSharesExtended, SubmitSharesStandard, SubmitSharesSuccess, UpdateChannel,
    UpdateChannelError,
};

use crate::common_properties::{IsMiningDownstream, IsMiningUpstream};
use crate::routing_logic::{MiningRouter, MiningRoutingLogic};
use crate::selectors::DownstreamMiningSelector;

use super::SendTo_;

use crate::utils::Mutex;
use std::fmt::Debug as D;
use std::sync::Arc;

pub type SendTo<Remote> = SendTo_<Mining<'static>, Remote>;

pub enum ChannelType {
    Standard,
    Extended,
    Group,
    // Non header only connection can have both group and extended channels.
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
    fn get_channel_type(&self) -> ChannelType;

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
            .unwrap();
        match routing_logic.clone() {
            MiningRoutingLogic::None => (),
            MiningRoutingLogic::Proxy(r_logic) => {
                r_logic
                    .safe_lock(|r_logic| {
                        r_logic.update_id_downstream(message_type, payload, &downstream_mining_data)
                    })
                    .unwrap();
            }
            MiningRoutingLogic::_P(_) => panic!(),
        }
        match (message_type, payload).try_into() {
            Ok(Mining::OpenStandardMiningChannel(m)) => {
                let upstream = match routing_logic {
                    MiningRoutingLogic::None => None,
                    MiningRoutingLogic::Proxy(r_logic) => Some(
                        r_logic
                            .safe_lock(|r_logic| {
                                r_logic
                                    .on_open_standard_channel(self_mutex.clone(), &m)
                                    .unwrap()
                            })
                            .unwrap(),
                    ),
                    MiningRoutingLogic::_P(_) => panic!(),
                };
                match channel_type {
                    ChannelType::Standard => self_mutex
                        .safe_lock(|self_| self_.handle_open_standard_mining_channel(m, upstream))
                        .unwrap(),
                    ChannelType::Extended => Err(Error::UnexpectedMessage),
                    ChannelType::Group => self_mutex
                        .safe_lock(|self_| self_.handle_open_standard_mining_channel(m, upstream))
                        .unwrap(),
                    ChannelType::GroupAndExtended => todo!(),
                }
            }
            Ok(Mining::OpenExtendedMiningChannel(m)) => match channel_type {
                ChannelType::Standard => Err(Error::UnexpectedMessage),
                ChannelType::Extended => self_mutex
                    .safe_lock(|self_| self_.handle_open_extended_mining_channel(m))
                    .unwrap(),
                ChannelType::Group => Err(Error::UnexpectedMessage),
                ChannelType::GroupAndExtended => todo!(),
            },
            Ok(Mining::UpdateChannel(m)) => match channel_type {
                ChannelType::Standard => self_mutex
                    .safe_lock(|self_| self_.handle_update_channel(m))
                    .unwrap(),
                ChannelType::Extended => self_mutex
                    .safe_lock(|self_| self_.handle_update_channel(m))
                    .unwrap(),
                ChannelType::Group => self_mutex
                    .safe_lock(|self_| self_.handle_update_channel(m))
                    .unwrap(),
                ChannelType::GroupAndExtended => todo!(),
            },
            Ok(Mining::SubmitSharesStandard(m)) => match channel_type {
                ChannelType::Standard => self_mutex
                    .safe_lock(|self_| self_.handle_submit_shares_standard(m))
                    .unwrap(),
                ChannelType::Extended => Err(Error::UnexpectedMessage),
                ChannelType::Group => self_mutex
                    .safe_lock(|self_| self_.handle_submit_shares_standard(m))
                    .unwrap(),
                ChannelType::GroupAndExtended => todo!(),
            },
            Ok(Mining::SubmitSharesExtended(m)) => match channel_type {
                ChannelType::Standard => Err(Error::UnexpectedMessage),
                ChannelType::Extended => self_mutex
                    .safe_lock(|self_| self_.handle_submit_shares_extended(m))
                    .unwrap(),
                ChannelType::Group => Err(Error::UnexpectedMessage),
                ChannelType::GroupAndExtended => todo!(),
            },
            Ok(Mining::SetCustomMiningJob(m)) => match (channel_type, is_work_selection_enabled) {
                (ChannelType::Extended, true) => self_mutex
                    .safe_lock(|self_| self_.handle_set_custom_mining_job(m))
                    .unwrap(),
                (ChannelType::Group, true) => self_mutex
                    .safe_lock(|self_| self_.handle_set_custom_mining_job(m))
                    .unwrap(),
                (ChannelType::GroupAndExtended, _) => todo!(),
                _ => Err(Error::UnexpectedMessage),
            },
            Ok(_) => Err(Error::UnexpectedMessage),
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
    fn get_channel_type(&self) -> ChannelType;

    fn get_request_id_mapper(&mut self) -> Option<Arc<Mutex<RequestIdMapper>>> {
        None
    }

    /// Proxies likely would want to update a downstream req id to a new one as req id must be
    /// connection-wide unique
    /// The implementor of DownstreamMining need to pass a RequestIdMapper if want to change the req id
    fn handle_message_mining(
        self_mutex: Arc<Mutex<Self>>,
        message_type: u8,
        payload: &mut [u8],
        routing_logic: MiningRoutingLogic<Down, Self, Selector, Router>,
    ) -> Result<SendTo<Down>, Error> {
        let original_request_id = match routing_logic.clone() {
            MiningRoutingLogic::None => 0,
            MiningRoutingLogic::Proxy(r_logic) => r_logic
                .safe_lock(|r_logic| {
                    r_logic.update_id_upstream(message_type, payload, self_mutex.clone())
                })
                .unwrap(),
            MiningRoutingLogic::_P(_) => panic!(),
        };

        let (channel_type, is_work_selection_enabled) = self_mutex
            .safe_lock(|s| (s.get_channel_type(), s.is_work_selection_enabled()))
            .unwrap();

        match (message_type, payload).try_into() {
            Ok(Mining::OpenStandardMiningChannelSuccess(m)) => {
                let remote = match routing_logic {
                    MiningRoutingLogic::None => None,
                    MiningRoutingLogic::Proxy(r_logic) => r_logic
                        .safe_lock(|r_logic| {
                            Some(
                                r_logic
                                    .on_open_standard_channel_success(
                                        self_mutex.clone(),
                                        original_request_id,
                                        &m,
                                    )
                                    .unwrap(),
                            )
                        })
                        .unwrap(),
                    MiningRoutingLogic::_P(_) => panic!(),
                };
                match channel_type {
                    ChannelType::Standard => self_mutex
                        .safe_lock(|s| s.handle_open_standard_mining_channel_success(m, remote))
                        .unwrap(),
                    ChannelType::Extended => Err(Error::UnexpectedMessage),
                    ChannelType::Group => self_mutex
                        .safe_lock(|s| s.handle_open_standard_mining_channel_success(m, remote))
                        .unwrap(),
                    ChannelType::GroupAndExtended => todo!(),
                }
            }
            Ok(Mining::OpenExtendedMiningChannelSuccess(m)) => match channel_type {
                ChannelType::Standard => Err(Error::UnexpectedMessage),
                ChannelType::Extended => self_mutex
                    .safe_lock(|s| s.handle_open_extended_mining_channel_success(m))
                    .unwrap(),
                ChannelType::Group => Err(Error::UnexpectedMessage),
                ChannelType::GroupAndExtended => todo!(),
            },
            Ok(Mining::OpenMiningChannelError(m)) => match channel_type {
                ChannelType::Standard => self_mutex
                    .safe_lock(|x| x.handle_open_mining_channel_error(m))
                    .unwrap(),
                ChannelType::Extended => self_mutex
                    .safe_lock(|x| x.handle_open_mining_channel_error(m))
                    .unwrap(),
                ChannelType::Group => self_mutex
                    .safe_lock(|x| x.handle_open_mining_channel_error(m))
                    .unwrap(),
                ChannelType::GroupAndExtended => todo!(),
            },
            Ok(Mining::UpdateChannelError(m)) => match channel_type {
                ChannelType::Standard => self_mutex
                    .safe_lock(|x| x.handle_update_channel_error(m))
                    .unwrap(),
                ChannelType::Extended => self_mutex
                    .safe_lock(|x| x.handle_update_channel_error(m))
                    .unwrap(),
                ChannelType::Group => self_mutex
                    .safe_lock(|x| x.handle_update_channel_error(m))
                    .unwrap(),
                ChannelType::GroupAndExtended => todo!(),
            },
            Ok(Mining::CloseChannel(m)) => match channel_type {
                ChannelType::Standard => {
                    self_mutex.safe_lock(|x| x.handle_close_channel(m)).unwrap()
                }
                ChannelType::Extended => {
                    self_mutex.safe_lock(|x| x.handle_close_channel(m)).unwrap()
                }
                ChannelType::Group => self_mutex.safe_lock(|x| x.handle_close_channel(m)).unwrap(),
                ChannelType::GroupAndExtended => todo!(),
            },
            Ok(Mining::SetExtranoncePrefix(m)) => match channel_type {
                ChannelType::Standard => self_mutex
                    .safe_lock(|x| x.handle_set_extranonce_prefix(m))
                    .unwrap(),
                ChannelType::Extended => self_mutex
                    .safe_lock(|x| x.handle_set_extranonce_prefix(m))
                    .unwrap(),
                ChannelType::Group => self_mutex
                    .safe_lock(|x| x.handle_set_extranonce_prefix(m))
                    .unwrap(),
                ChannelType::GroupAndExtended => todo!(),
            },
            Ok(Mining::SubmitSharesSuccess(m)) => match channel_type {
                ChannelType::Standard => self_mutex
                    .safe_lock(|x| x.handle_submit_shares_success(m))
                    .unwrap(),
                ChannelType::Extended => self_mutex
                    .safe_lock(|x| x.handle_submit_shares_success(m))
                    .unwrap(),
                ChannelType::Group => self_mutex
                    .safe_lock(|x| x.handle_submit_shares_success(m))
                    .unwrap(),
                ChannelType::GroupAndExtended => todo!(),
            },
            Ok(Mining::SubmitSharesError(m)) => match channel_type {
                ChannelType::Standard => self_mutex
                    .safe_lock(|x| x.handle_submit_shares_error(m))
                    .unwrap(),
                ChannelType::Extended => self_mutex
                    .safe_lock(|x| x.handle_submit_shares_error(m))
                    .unwrap(),
                ChannelType::Group => self_mutex
                    .safe_lock(|x| x.handle_submit_shares_error(m))
                    .unwrap(),
                ChannelType::GroupAndExtended => todo!(),
            },
            Ok(Mining::NewMiningJob(m)) => match channel_type {
                ChannelType::Standard => self_mutex
                    .safe_lock(|x| x.handle_new_mining_job(m))
                    .unwrap(),
                ChannelType::Extended => Err(Error::UnexpectedMessage),
                ChannelType::Group => Err(Error::UnexpectedMessage),
                ChannelType::GroupAndExtended => todo!(),
            },
            Ok(Mining::NewExtendedMiningJob(m)) => match channel_type {
                ChannelType::Standard => Err(Error::UnexpectedMessage),
                ChannelType::Extended => self_mutex
                    .safe_lock(|x| x.handle_new_extended_mining_job(m))
                    .unwrap(),
                ChannelType::Group => self_mutex
                    .safe_lock(|x| x.handle_new_extended_mining_job(m))
                    .unwrap(),
                ChannelType::GroupAndExtended => todo!(),
            },
            Ok(Mining::SetNewPrevHash(m)) => match channel_type {
                ChannelType::Standard => self_mutex
                    .safe_lock(|x| x.handle_set_new_prev_hash(m))
                    .unwrap(),
                ChannelType::Extended => self_mutex
                    .safe_lock(|x| x.handle_set_new_prev_hash(m))
                    .unwrap(),
                ChannelType::Group => self_mutex
                    .safe_lock(|x| x.handle_set_new_prev_hash(m))
                    .unwrap(),
                ChannelType::GroupAndExtended => todo!(),
            },
            Ok(Mining::SetCustomMiningJobSuccess(m)) => {
                match (channel_type, is_work_selection_enabled) {
                    (ChannelType::Extended, true) => self_mutex
                        .safe_lock(|x| x.handle_set_custom_mining_job_success(m))
                        .unwrap(),
                    (ChannelType::Group, true) => self_mutex
                        .safe_lock(|x| x.handle_set_custom_mining_job_success(m))
                        .unwrap(),
                    (ChannelType::GroupAndExtended, _) => todo!(),
                    _ => Err(Error::UnexpectedMessage),
                }
            }
            Ok(Mining::SetCustomMiningJobError(m)) => {
                match (channel_type, is_work_selection_enabled) {
                    (ChannelType::Extended, true) => self_mutex
                        .safe_lock(|x| x.handle_set_custom_mining_job_error(m))
                        .unwrap(),
                    (ChannelType::Group, true) => self_mutex
                        .safe_lock(|x| x.handle_set_custom_mining_job_error(m))
                        .unwrap(),
                    (ChannelType::GroupAndExtended, _) => todo!(),
                    _ => Err(Error::UnexpectedMessage),
                }
            }
            Ok(Mining::SetTarget(m)) => match channel_type {
                ChannelType::Standard => self_mutex.safe_lock(|x| x.handle_set_target(m)).unwrap(),
                ChannelType::Extended => self_mutex.safe_lock(|x| x.handle_set_target(m)).unwrap(),
                ChannelType::Group => self_mutex.safe_lock(|x| x.handle_set_target(m)).unwrap(),
                ChannelType::GroupAndExtended => todo!(),
            },
            Ok(Mining::Reconnect(m)) => match channel_type {
                ChannelType::Standard => self_mutex.safe_lock(|x| x.handle_reconnect(m)).unwrap(),
                ChannelType::Extended => self_mutex.safe_lock(|x| x.handle_reconnect(m)).unwrap(),
                ChannelType::Group => self_mutex.safe_lock(|x| x.handle_reconnect(m)).unwrap(),
                ChannelType::GroupAndExtended => todo!(),
            },
            Ok(Mining::SetGroupChannel(_)) => todo!(),
            Ok(_) => Err(Error::UnexpectedMessage),
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
}
