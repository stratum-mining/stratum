use crate::Error;
pub use crate::Mining;
use core::convert::TryInto;
pub use mining_sv2::{
    CloseChannel, NewExtendedMiningJob, NewMiningJob, OpenExtendedMiningChannel,
    OpenExtendedMiningChannelSuccess, OpenMiningChannelError, OpenStandardMiningChannel,
    OpenStandardMiningChannelSuccess, Reconnect, SetCustomMiningJob, SetCustomMiningJobError,
    SetCustomMiningJobSuccess, SetExtranoncePrefix, SetGroupChannel, SetNewPrevHash, SetTarget,
    SubmitSharesError, SubmitSharesExtended, SubmitSharesStandard, SubmitSharesSuccess,
    UpdateChannel, UpdateChannelError,
};

pub use super::RemoteSelector;
use std::collections::HashMap;

use super::SendTo_;
pub type SendTo<Remote> = SendTo_<crate::Mining<'static>, Remote>;

pub enum ChannelType {
    Standard,
    Extended,
    Group,
    // Non header only connection can have both group and extended channels.
    GroupAndExtended,
}

/// Proxyies likely need to change the request ids of downsteam's messages. They also need to
/// remeber original id to patch the upstream's response with it
#[derive(Debug)]
pub struct RequestIdMapper {
    // upstream id -> downstream id
    request_ids_map: Arc<Mutex<HashMap<u32, u32>>>,
    next_id: u32,
}

impl Default for RequestIdMapper {
    fn default() -> Self {
        Self::new()
    }
}

impl RequestIdMapper {
    pub fn new() -> Self {
        Self {
            request_ids_map: Arc::new(Mutex::new(HashMap::new())),
            next_id: 0,
        }
    }

    pub fn on_open_channel(&mut self, id: u32) -> u32 {
        let new_id = self.next_id;
        self.next_id += 1;

        let mut inner = self.request_ids_map.lock().unwrap();
        inner.insert(new_id, id);
        new_id
    }

    pub fn remove(&mut self, upstream_id: u32) -> u32 {
        let mut inner = self.request_ids_map.lock().unwrap();
        inner.remove(&upstream_id).unwrap()
    }
}

/// WARNING this function assume that request id are the first 2 bytes of the
/// payload
/// TODO this function should probably stay in another crate
fn update_request_id(payload: &mut [u8], id: u32) {
    let bytes = id.to_le_bytes();
    payload[0] = bytes[0];
    payload[1] = bytes[1];
    payload[2] = bytes[2];
    payload[3] = bytes[3];
}

/// WARNING this function assume that request id are the first 2 bytes of the
/// payload
/// TODO this function should probably stay in another crate
fn get_request_id(payload: &mut [u8]) -> u32 {
    let bytes = [payload[0], payload[1], payload[2], payload[3]];
    u32::from_le_bytes(bytes)
}

use std::sync::{Arc, Mutex};

/// Connection-wide downtream's messages parser implemented by an upstream.
pub trait UpstreamMining<Remote, Selector: RemoteSelector<Remote>> {
    fn get_channel_type(&self) -> ChannelType;

    /// Proxies likely would  want to update a downstream req id to a new one as req id must be
    /// connection-wide unique
    /// The implementor of UpstreamMining need to pass a RequestIdMapper if want to change the req id
    fn handle_message(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
        selector: Option<Arc<Mutex<Selector>>>,
        downstream_connection: Remote,
        mapper: Option<&mut RequestIdMapper>,
    ) -> Result<SendTo<Remote>, Error> {
        // Update request ids
        if let Some(id_map) = mapper {
            match message_type {
                const_sv2::MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL => {
                    let old_id = get_request_id(payload);
                    let new_req_id = id_map.on_open_channel(old_id);
                    update_request_id(payload, new_req_id);
                }
                const_sv2::MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL => {
                    let old_id = get_request_id(payload);
                    let new_req_id = id_map.on_open_channel(old_id);
                    update_request_id(payload, new_req_id);
                }
                const_sv2::MESSAGE_TYPE_SET_CUSTOM_MINING_JOB => {
                    let old_id = get_request_id(payload);
                    let new_req_id = id_map.on_open_channel(old_id);
                    update_request_id(payload, new_req_id);
                }
                _ => (),
            }
        }
        match (message_type, payload).try_into() {
            Ok(Mining::OpenStandardMiningChannel(m)) => {
                if let Some(selector) = selector {
                    let mut selector = selector.lock().unwrap();
                    selector.on_open_standard_channel_request(m.request_id, downstream_connection);
                    drop(selector);
                }
                match self.get_channel_type() {
                    ChannelType::Standard => self.handle_open_standard_mining_channel(m),
                    ChannelType::Extended => Err(Error::UnexpectedMessage),
                    ChannelType::Group => self.handle_open_standard_mining_channel(m),
                    ChannelType::GroupAndExtended => todo!(),
                }
            }
            Ok(Mining::OpenExtendedMiningChannel(m)) => match self.get_channel_type() {
                ChannelType::Standard => Err(Error::UnexpectedMessage),
                ChannelType::Extended => self.handle_open_extended_mining_channel(m),
                ChannelType::Group => Err(Error::UnexpectedMessage),
                ChannelType::GroupAndExtended => todo!(),
            },
            Ok(Mining::UpdateChannel(m)) => match self.get_channel_type() {
                ChannelType::Standard => self.handle_update_channel(m),
                ChannelType::Extended => self.handle_update_channel(m),
                ChannelType::Group => self.handle_update_channel(m),
                ChannelType::GroupAndExtended => todo!(),
            },
            Ok(Mining::SubmitSharesStandard(m)) => match self.get_channel_type() {
                ChannelType::Standard => self.handle_submit_shares_standard(m),
                ChannelType::Extended => Err(Error::UnexpectedMessage),
                ChannelType::Group => self.handle_submit_shares_standard(m),
                ChannelType::GroupAndExtended => todo!(),
            },
            Ok(Mining::SubmitSharesExtended(m)) => match self.get_channel_type() {
                ChannelType::Standard => Err(Error::UnexpectedMessage),
                ChannelType::Extended => self.handle_submit_shares_extended(m),
                ChannelType::Group => Err(Error::UnexpectedMessage),
                ChannelType::GroupAndExtended => todo!(),
            },
            Ok(Mining::SetCustomMiningJob(m)) => {
                match (self.get_channel_type(), self.is_work_selection_enabled()) {
                    (ChannelType::Extended, true) => self.handle_set_custom_mining_job(m),
                    (ChannelType::Group, true) => self.handle_set_custom_mining_job(m),
                    (ChannelType::GroupAndExtended, _) => todo!(),
                    _ => Err(Error::UnexpectedMessage),
                }
            }
            Ok(_) => Err(Error::UnexpectedMessage),
            Err(e) => Err(e),
        }
    }

    fn is_work_selection_enabled(&self) -> bool;

    fn handle_open_standard_mining_channel(
        &mut self,
        m: OpenStandardMiningChannel,
    ) -> Result<SendTo<Remote>, Error>;

    fn handle_open_extended_mining_channel(
        &mut self,
        m: OpenExtendedMiningChannel,
    ) -> Result<SendTo<Remote>, Error>;

    fn handle_update_channel(&mut self, m: UpdateChannel) -> Result<SendTo<Remote>, Error>;

    fn handle_submit_shares_standard(
        &mut self,
        m: SubmitSharesStandard,
    ) -> Result<SendTo<Remote>, Error>;

    fn handle_submit_shares_extended(
        &mut self,
        m: SubmitSharesExtended,
    ) -> Result<SendTo<Remote>, Error>;

    fn handle_set_custom_mining_job(
        &mut self,
        m: SetCustomMiningJob,
    ) -> Result<SendTo<Remote>, Error>;
}
/// Connection-wide upstream's messages parser implemented by a downstream.
pub trait DownstreamMining<Remote, Selector: RemoteSelector<Remote>> {
    fn get_channel_type(&self) -> ChannelType;

    fn get_request_id_mapper(&mut self) -> Option<&mut RequestIdMapper> {
        None
    }

    /// Proxies likely would want to update a downstream req id to a new one as req id must be
    /// connection-wide unique
    /// The implementor of DownstreamMining need to pass a RequestIdMapper if want to change the req id
    fn handle_message(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
        selector: Option<Arc<Mutex<Selector>>>,
        //mut request_id_mapper: Option<&mut RequestIdMapper>,
    ) -> Result<SendTo<Remote>, Error> {
        // Update request ids with original requests ids.
        let mut request_id_mapper = self.get_request_id_mapper();
        let mut original_request_id = 0;
        if let Some(id_map) = request_id_mapper.as_mut() {
            match message_type {
                const_sv2::MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS => {
                    let upstream_id = get_request_id(payload);
                    original_request_id = upstream_id;
                    let downstream_id = id_map.remove(upstream_id);
                    update_request_id(payload, downstream_id);
                }
                const_sv2::MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCES => {
                    let upstream_id = get_request_id(payload);
                    original_request_id = upstream_id;
                    let downstream_id = id_map.remove(upstream_id);
                    update_request_id(payload, downstream_id);
                }
                const_sv2::MESSAGE_TYPE_OPEN_MINING_CHANNEL_ERROR => {
                    let upstream_id = get_request_id(payload);
                    original_request_id = upstream_id;
                    let downstream_id = id_map.remove(upstream_id);
                    update_request_id(payload, downstream_id);
                }
                const_sv2::MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_SUCCESS => {
                    let upstream_id = get_request_id(payload);
                    original_request_id = upstream_id;
                    let downstream_id = id_map.remove(upstream_id);
                    update_request_id(payload, downstream_id);
                }
                const_sv2::MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_ERROR => {
                    let upstream_id = get_request_id(payload);
                    original_request_id = upstream_id;
                    let downstream_id = id_map.remove(upstream_id);
                    update_request_id(payload, downstream_id);
                }
                _ => (),
            }
        }
        match (message_type, payload).try_into() {
            Ok(Mining::OpenStandardMiningChannelSuccess(m)) => {
                let remote = match selector {
                    Some(selector) => {
                        let mut remote = Vec::with_capacity(1);
                        let mut selector = selector.lock().unwrap();
                        remote.push(selector.on_open_standard_channel_success(
                            original_request_id,
                            m.group_channel_id,
                        ));
                        remote
                    }
                    None => Vec::with_capacity(0),
                };
                match self.get_channel_type() {
                    ChannelType::Standard => {
                        self.handle_open_standard_mining_channel_success(m, remote)
                    }
                    ChannelType::Extended => Err(Error::UnexpectedMessage),
                    ChannelType::Group => {
                        self.handle_open_standard_mining_channel_success(m, remote)
                    }
                    ChannelType::GroupAndExtended => todo!(),
                }
            }
            Ok(Mining::OpenExtendedMiningChannelSuccess(m)) => match self.get_channel_type() {
                ChannelType::Standard => Err(Error::UnexpectedMessage),
                ChannelType::Extended => self.handle_open_extended_mining_channel_success(m),
                ChannelType::Group => Err(Error::UnexpectedMessage),
                ChannelType::GroupAndExtended => todo!(),
            },
            Ok(Mining::OpenMiningChannelError(m)) => match self.get_channel_type() {
                ChannelType::Standard => self.handle_open_mining_channel_error(m),
                ChannelType::Extended => self.handle_open_mining_channel_error(m),
                ChannelType::Group => self.handle_open_mining_channel_error(m),
                ChannelType::GroupAndExtended => todo!(),
            },
            Ok(Mining::UpdateChannelError(m)) => match self.get_channel_type() {
                ChannelType::Standard => self.handle_update_channel_error(m),
                ChannelType::Extended => self.handle_update_channel_error(m),
                ChannelType::Group => self.handle_update_channel_error(m),
                ChannelType::GroupAndExtended => todo!(),
            },
            Ok(Mining::CloseChannel(m)) => match self.get_channel_type() {
                ChannelType::Standard => self.handle_close_channel(m),
                ChannelType::Extended => self.handle_close_channel(m),
                ChannelType::Group => self.handle_close_channel(m),
                ChannelType::GroupAndExtended => todo!(),
            },
            Ok(Mining::SetExtranoncePrefix(m)) => match self.get_channel_type() {
                ChannelType::Standard => self.handle_set_extranonce_prefix(m),
                ChannelType::Extended => self.handle_set_extranonce_prefix(m),
                ChannelType::Group => self.handle_set_extranonce_prefix(m),
                ChannelType::GroupAndExtended => todo!(),
            },
            Ok(Mining::SubmitSharesSuccess(m)) => match self.get_channel_type() {
                ChannelType::Standard => self.handle_submit_shares_success(m),
                ChannelType::Extended => self.handle_submit_shares_success(m),
                ChannelType::Group => self.handle_submit_shares_success(m),
                ChannelType::GroupAndExtended => todo!(),
            },
            Ok(Mining::SubmitSharesError(m)) => match self.get_channel_type() {
                ChannelType::Standard => self.handle_submit_shares_error(m),
                ChannelType::Extended => self.handle_submit_shares_error(m),
                ChannelType::Group => self.handle_submit_shares_error(m),
                ChannelType::GroupAndExtended => todo!(),
            },
            Ok(Mining::NewMiningJob(m)) => match self.get_channel_type() {
                ChannelType::Standard => self.handle_new_mining_job(m),
                ChannelType::Extended => Err(Error::UnexpectedMessage),
                ChannelType::Group => Err(Error::UnexpectedMessage),
                ChannelType::GroupAndExtended => todo!(),
            },
            Ok(Mining::NewExtendedMiningJob(m)) => match self.get_channel_type() {
                ChannelType::Standard => Err(Error::UnexpectedMessage),
                ChannelType::Extended => self.handle_new_extended_mining_job(m),
                ChannelType::Group => self.handle_new_extended_mining_job(m),
                ChannelType::GroupAndExtended => todo!(),
            },
            Ok(Mining::SetNewPrevHash(m)) => match self.get_channel_type() {
                ChannelType::Standard => self.handle_set_new_prev_hash(m),
                ChannelType::Extended => self.handle_set_new_prev_hash(m),
                ChannelType::Group => self.handle_set_new_prev_hash(m),
                ChannelType::GroupAndExtended => todo!(),
            },
            Ok(Mining::SetCustomMiningJobSuccess(m)) => {
                match (self.get_channel_type(), self.is_work_selection_enabled()) {
                    (ChannelType::Extended, true) => self.handle_set_custom_mining_job_success(m),
                    (ChannelType::Group, true) => self.handle_set_custom_mining_job_success(m),
                    (ChannelType::GroupAndExtended, _) => todo!(),
                    _ => Err(Error::UnexpectedMessage),
                }
            }
            Ok(Mining::SetCustomMiningJobError(m)) => {
                match (self.get_channel_type(), self.is_work_selection_enabled()) {
                    (ChannelType::Extended, true) => self.handle_set_custom_mining_job_error(m),
                    (ChannelType::Group, true) => self.handle_set_custom_mining_job_error(m),
                    (ChannelType::GroupAndExtended, _) => todo!(),
                    _ => Err(Error::UnexpectedMessage),
                }
            }
            Ok(Mining::SetTarget(m)) => match self.get_channel_type() {
                ChannelType::Standard => self.handle_set_target(m),
                ChannelType::Extended => self.handle_set_target(m),
                ChannelType::Group => self.handle_set_target(m),
                ChannelType::GroupAndExtended => todo!(),
            },
            Ok(Mining::Reconnect(m)) => match self.get_channel_type() {
                ChannelType::Standard => self.handle_reconnect(m),
                ChannelType::Extended => self.handle_reconnect(m),
                ChannelType::Group => self.handle_reconnect(m),
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
        remote: Vec<Remote>,
    ) -> Result<SendTo<Remote>, Error>;

    fn handle_open_extended_mining_channel_success(
        &mut self,
        m: OpenExtendedMiningChannelSuccess,
    ) -> Result<SendTo<Remote>, Error>;

    fn handle_open_mining_channel_error(
        &mut self,
        m: OpenMiningChannelError,
    ) -> Result<SendTo<Remote>, Error>;

    fn handle_update_channel_error(
        &mut self,
        m: UpdateChannelError,
    ) -> Result<SendTo<Remote>, Error>;

    fn handle_close_channel(&mut self, m: CloseChannel) -> Result<SendTo<Remote>, Error>;

    fn handle_set_extranonce_prefix(
        &mut self,
        m: SetExtranoncePrefix,
    ) -> Result<SendTo<Remote>, Error>;

    fn handle_submit_shares_success(
        &mut self,
        m: SubmitSharesSuccess,
    ) -> Result<SendTo<Remote>, Error>;

    fn handle_submit_shares_error(&mut self, m: SubmitSharesError)
        -> Result<SendTo<Remote>, Error>;

    fn handle_new_mining_job(&mut self, m: NewMiningJob) -> Result<SendTo<Remote>, Error>;

    fn handle_new_extended_mining_job(
        &mut self,
        m: NewExtendedMiningJob,
    ) -> Result<SendTo<Remote>, Error>;

    fn handle_set_new_prev_hash(&mut self, m: SetNewPrevHash) -> Result<SendTo<Remote>, Error>;

    fn handle_set_custom_mining_job_success(
        &mut self,
        m: SetCustomMiningJobSuccess,
    ) -> Result<SendTo<Remote>, Error>;

    fn handle_set_custom_mining_job_error(
        &mut self,
        m: SetCustomMiningJobError,
    ) -> Result<SendTo<Remote>, Error>;

    fn handle_set_target(&mut self, m: SetTarget) -> Result<SendTo<Remote>, Error>;

    fn handle_reconnect(&mut self, m: Reconnect) -> Result<SendTo<Remote>, Error>;
}
