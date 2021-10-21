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

use super::SendTo_;
pub type SendTo = SendTo_<crate::Mining<'static>>;

pub enum ChannelType {
    Standard,
    Extended,
    Group,
}

pub trait UpstreamMining {
    fn get_channel_type(&self) -> ChannelType;

    fn handle_message(&mut self, message_type: u8, payload: &mut [u8]) -> Result<SendTo, Error> {
        match (message_type, payload).try_into() {
            Ok(Mining::OpenStandardMiningChannel(m)) => match self.get_channel_type() {
                ChannelType::Standard => self.handle_open_standard_mining_channel(m),
                ChannelType::Extended => Err(Error::UnexpectedMessage),
                ChannelType::Group => self.handle_open_standard_mining_channel(m),
            },
            Ok(Mining::OpenExtendedMiningChannel(m)) => match self.get_channel_type() {
                ChannelType::Standard => Err(Error::UnexpectedMessage),
                ChannelType::Extended => self.handle_open_extended_mining_channel(m),
                ChannelType::Group => Err(Error::UnexpectedMessage),
            },
            Ok(Mining::UpdateChannel(m)) => match self.get_channel_type() {
                ChannelType::Standard => self.handle_update_channel(m),
                ChannelType::Extended => self.handle_update_channel(m),
                ChannelType::Group => self.handle_update_channel(m),
            },
            Ok(Mining::SubmitSharesStandard(m)) => match self.get_channel_type() {
                ChannelType::Standard => self.handle_submit_shares_standard(m),
                ChannelType::Extended => Err(Error::UnexpectedMessage),
                ChannelType::Group => self.handle_submit_shares_standard(m),
            },
            Ok(Mining::SubmitSharesExtended(m)) => match self.get_channel_type() {
                ChannelType::Standard => Err(Error::UnexpectedMessage),
                ChannelType::Extended => self.handle_submit_shares_extended(m),
                ChannelType::Group => Err(Error::UnexpectedMessage),
            },
            Ok(Mining::SetCustomMiningJob(m)) => {
                match (self.get_channel_type(), self.is_work_selection_enabled()) {
                    (ChannelType::Extended, true) => self.handle_set_custom_mining_job(m),
                    (ChannelType::Group, true) => self.handle_set_custom_mining_job(m),
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
    ) -> Result<SendTo, Error>;

    fn handle_open_extended_mining_channel(
        &mut self,
        m: OpenExtendedMiningChannel,
    ) -> Result<SendTo, Error>;

    fn handle_update_channel(&mut self, m: UpdateChannel) -> Result<SendTo, Error>;

    fn handle_submit_shares_standard(&mut self, m: SubmitSharesStandard) -> Result<SendTo, Error>;

    fn handle_submit_shares_extended(&mut self, m: SubmitSharesExtended) -> Result<SendTo, Error>;

    fn handle_set_custom_mining_job(&mut self, m: SetCustomMiningJob) -> Result<SendTo, Error>;
}

pub trait DownstreamMining {
    fn get_channel_type(&self) -> ChannelType;

    fn handle_message(&mut self, message_type: u8, payload: &mut [u8]) -> Result<SendTo, Error> {
        match (message_type, payload).try_into() {
            Ok(Mining::OpenStandardMiningChannelSuccess(m)) => match self.get_channel_type() {
                ChannelType::Standard => self.handle_open_standard_mining_channel_success(m),
                ChannelType::Extended => Err(Error::UnexpectedMessage),
                ChannelType::Group => self.handle_open_standard_mining_channel_success(m),
            },
            Ok(Mining::OpenExtendedMiningChannelSuccess(m)) => match self.get_channel_type() {
                ChannelType::Standard => Err(Error::UnexpectedMessage),
                ChannelType::Extended => self.handle_open_extended_mining_channel_success(m),
                ChannelType::Group => Err(Error::UnexpectedMessage),
            },
            Ok(Mining::OpenMiningChannelError(m)) => match self.get_channel_type() {
                ChannelType::Standard => self.handle_open_mining_channel_error(m),
                ChannelType::Extended => self.handle_open_mining_channel_error(m),
                ChannelType::Group => self.handle_open_mining_channel_error(m),
            },
            Ok(Mining::UpdateChannelError(m)) => match self.get_channel_type() {
                ChannelType::Standard => self.handle_update_channel_error(m),
                ChannelType::Extended => self.handle_update_channel_error(m),
                ChannelType::Group => self.handle_update_channel_error(m),
            },
            Ok(Mining::CloseChannel(m)) => match self.get_channel_type() {
                ChannelType::Standard => self.handle_close_channel(m),
                ChannelType::Extended => self.handle_close_channel(m),
                ChannelType::Group => self.handle_close_channel(m),
            },
            Ok(Mining::SetExtranoncePrefix(m)) => match self.get_channel_type() {
                ChannelType::Standard => self.handle_set_extranonce_prefix(m),
                ChannelType::Extended => self.handle_set_extranonce_prefix(m),
                ChannelType::Group => self.handle_set_extranonce_prefix(m),
            },
            Ok(Mining::SubmitSharesSuccess(m)) => match self.get_channel_type() {
                ChannelType::Standard => self.handle_submit_shares_success(m),
                ChannelType::Extended => self.handle_submit_shares_success(m),
                ChannelType::Group => self.handle_submit_shares_success(m),
            },
            Ok(Mining::SubmitSharesError(m)) => match self.get_channel_type() {
                ChannelType::Standard => self.handle_submit_shares_error(m),
                ChannelType::Extended => self.handle_submit_shares_error(m),
                ChannelType::Group => self.handle_submit_shares_error(m),
            },
            Ok(Mining::NewMiningJob(m)) => match self.get_channel_type() {
                ChannelType::Standard => self.handle_new_mining_job(m),
                ChannelType::Extended => Err(Error::UnexpectedMessage),
                ChannelType::Group => Err(Error::UnexpectedMessage),
            },
            Ok(Mining::NewExtendedMiningJob(m)) => match self.get_channel_type() {
                ChannelType::Standard => Err(Error::UnexpectedMessage),
                ChannelType::Extended => self.handle_new_extended_mining_job(m),
                ChannelType::Group => self.handle_new_extended_mining_job(m),
            },
            Ok(Mining::SetNewPrevHash(m)) => match self.get_channel_type() {
                ChannelType::Standard => self.handle_set_new_prev_hash(m),
                ChannelType::Extended => self.handle_set_new_prev_hash(m),
                ChannelType::Group => self.handle_set_new_prev_hash(m),
            },
            Ok(Mining::SetCustomMiningJobSuccess(m)) => {
                match (self.get_channel_type(), self.is_work_selection_enabled()) {
                    (ChannelType::Extended, true) => self.handle_set_custom_mining_job_success(m),
                    (ChannelType::Group, true) => self.handle_set_custom_mining_job_success(m),
                    _ => Err(Error::UnexpectedMessage),
                }
            }
            Ok(Mining::SetCustomMiningJobError(m)) => {
                match (self.get_channel_type(), self.is_work_selection_enabled()) {
                    (ChannelType::Extended, true) => self.handle_set_custom_mining_job_error(m),
                    (ChannelType::Group, true) => self.handle_set_custom_mining_job_error(m),
                    _ => Err(Error::UnexpectedMessage),
                }
            }
            Ok(Mining::SetTarget(m)) => match self.get_channel_type() {
                ChannelType::Standard => self.handle_set_target(m),
                ChannelType::Extended => self.handle_set_target(m),
                ChannelType::Group => self.handle_set_target(m),
            },
            Ok(Mining::Reconnect(m)) => match self.get_channel_type() {
                ChannelType::Standard => self.handle_reconnect(m),
                ChannelType::Extended => self.handle_reconnect(m),
                ChannelType::Group => self.handle_reconnect(m),
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
    ) -> Result<SendTo, Error>;

    fn handle_open_extended_mining_channel_success(
        &mut self,
        m: OpenExtendedMiningChannelSuccess,
    ) -> Result<SendTo, Error>;

    fn handle_open_mining_channel_error(
        &mut self,
        m: OpenMiningChannelError,
    ) -> Result<SendTo, Error>;

    fn handle_update_channel_error(&mut self, m: UpdateChannelError) -> Result<SendTo, Error>;

    fn handle_close_channel(&mut self, m: CloseChannel) -> Result<SendTo, Error>;

    fn handle_set_extranonce_prefix(&mut self, m: SetExtranoncePrefix) -> Result<SendTo, Error>;

    fn handle_submit_shares_success(&mut self, m: SubmitSharesSuccess) -> Result<SendTo, Error>;

    fn handle_submit_shares_error(&mut self, m: SubmitSharesError) -> Result<SendTo, Error>;

    fn handle_new_mining_job(&mut self, m: NewMiningJob) -> Result<SendTo, Error>;

    fn handle_new_extended_mining_job(&mut self, m: NewExtendedMiningJob) -> Result<SendTo, Error>;

    fn handle_set_new_prev_hash(&mut self, m: SetNewPrevHash) -> Result<SendTo, Error>;

    fn handle_set_custom_mining_job_success(
        &mut self,
        m: SetCustomMiningJobSuccess,
    ) -> Result<SendTo, Error>;

    fn handle_set_custom_mining_job_error(
        &mut self,
        m: SetCustomMiningJobError,
    ) -> Result<SendTo, Error>;

    fn handle_set_target(&mut self, m: SetTarget) -> Result<SendTo, Error>;

    fn handle_reconnect(&mut self, m: Reconnect) -> Result<SendTo, Error>;
}
