//! # Mining Handlers
//!
//! This module defines traits and functions for handling mining-related messages within the Stratum
//! V2 protocol.
//!
//! ## Message Handling
//!
//! Handlers in this module are responsible for:
//! - Parsing and deserializing mining-related messages into the appropriate types.
//! - Dispatching the deserialized messages to specific handler functions based on message type,
//!   such as handling new mining jobs, share submissions, and extranonce updates.
//! - Ensuring the integrity and validity of received messages, while interacting with downstream
//!   mining systems to ensure proper communication and task execution.
//!
//! ## Return Type
//!
//! Functions return `Result<SendTo<Down>, Error>`, where `SendTo<Down>` specifies the next action
//! for the message: whether it should be sent to the downstream node, an error response should be
//! generated, or the message should be ignored.
//!
//! ## Structure
//!
//! This module includes:
//! - Traits for processing mining-related messages for both upstream and downstream communication.
//! - Functions to parse, deserialize, and process messages related to mining, ensuring robust error
//!   handling for unexpected conditions.
//! - Support for managing mining channels, extranonce prefixes, and share submissions, while
//!   handling edge cases and ensuring the correctness of the mining process.

use crate::errors::Error;
use codec_sv2::binary_sv2;
use core::convert::TryInto;
use mining_sv2::{
    CloseChannel, NewExtendedMiningJob, NewMiningJob, OpenExtendedMiningChannel,
    OpenExtendedMiningChannelSuccess, OpenMiningChannelError, OpenStandardMiningChannel,
    OpenStandardMiningChannelSuccess, SetCustomMiningJob, SetCustomMiningJobError,
    SetCustomMiningJobSuccess, SetExtranoncePrefix, SetGroupChannel, SetNewPrevHash, SetTarget,
    SubmitSharesError, SubmitSharesExtended, SubmitSharesStandard, SubmitSharesSuccess,
    UpdateChannel, UpdateChannelError,
};
use parsers_sv2::Mining;

use super::SendTo_;

use crate::utils::Mutex;
use mining_sv2::*;
use std::{fmt::Debug as D, sync::Arc};

/// see [`SendTo_`]
pub type SendTo<Remote> = SendTo_<Mining<'static>, Remote>;

/// Represents supported channel types in a mining connection.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SupportedChannelTypes {
    Standard,
    Extended,
    Group,
    /// Represents a connection that supports both group and extended channels.
    GroupAndExtended,
}

/// Trait for parsing downstream mining messages in a Stratum V2 connection.
///
/// This trait defines methods for parsing and routing downstream messages
/// related to mining operations.
pub trait ParseMiningMessagesFromDownstream<Up: D>
where
    Self: Sized + D,
{
    /// Returns the type of channel supported by the downstream connection.
    fn get_channel_type(&self) -> SupportedChannelTypes;

    /// Handles a mining message from the downstream, given its type and payload.
    fn handle_message_mining(
        self_mutex: Arc<Mutex<Self>>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<SendTo<Up>, Error>
    where
        Self: Sized,
    {
        match Self::handle_message_mining_deserialized(
            self_mutex,
            (message_type, payload).try_into().map_err(Into::into),
        ) {
            Err(Error::UnexpectedMessage(0)) => Err(Error::UnexpectedMessage(message_type)),
            result => result,
        }
    }

    /// Deserializes and processes a mining message from the downstream.
    fn handle_message_mining_deserialized(
        self_mutex: Arc<Mutex<Self>>,
        message: Result<Mining<'_>, Error>,
    ) -> Result<SendTo<Up>, Error>
    where
        Self: Sized,
    {
        let (channel_type, is_work_selection_enabled) = self_mutex
            .safe_lock(|self_| (self_.get_channel_type(), self_.is_work_selection_enabled()))?;
        match message {
            Ok(Mining::OpenStandardMiningChannel(m)) => {
                // check user auth
                if !Self::is_downstream_authorized(self_mutex.clone(), &m.user_identity)? {
                    return Ok(SendTo::Respond(Mining::OpenMiningChannelError(
                        OpenMiningChannelError::new_unknown_user(m.get_request_id_as_u32()),
                    )));
                }
                match channel_type {
                    SupportedChannelTypes::Standard => self_mutex
                        .safe_lock(|self_| self_.handle_open_standard_mining_channel(m))?,
                    SupportedChannelTypes::Extended => Err(Error::UnexpectedMessage(
                        MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL,
                    )),
                    SupportedChannelTypes::Group => self_mutex
                        .safe_lock(|self_| self_.handle_open_standard_mining_channel(m))?,
                    SupportedChannelTypes::GroupAndExtended => self_mutex
                        .safe_lock(|self_| self_.handle_open_standard_mining_channel(m))?,
                }
            }
            Ok(Mining::OpenExtendedMiningChannel(m)) => {
                // check user auth
                if !Self::is_downstream_authorized(self_mutex.clone(), &m.user_identity)? {
                    return Ok(SendTo::Respond(Mining::OpenMiningChannelError(
                        OpenMiningChannelError::new_unknown_user(m.get_request_id_as_u32()),
                    )));
                };
                match channel_type {
                    SupportedChannelTypes::Standard => Err(Error::UnexpectedMessage(
                        MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL,
                    )),
                    SupportedChannelTypes::Extended => self_mutex
                        .safe_lock(|self_| self_.handle_open_extended_mining_channel(m))?,
                    SupportedChannelTypes::Group => Err(Error::UnexpectedMessage(
                        MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL,
                    )),
                    SupportedChannelTypes::GroupAndExtended => self_mutex
                        .safe_lock(|self_| self_.handle_open_extended_mining_channel(m))?,
                }
            }
            Ok(Mining::UpdateChannel(m)) => {
                self_mutex.safe_lock(|self_| self_.handle_update_channel(m))?
            }
            Ok(Mining::SubmitSharesStandard(m)) => match channel_type {
                SupportedChannelTypes::Standard => {
                    self_mutex.safe_lock(|self_| self_.handle_submit_shares_standard(m))?
                }
                SupportedChannelTypes::Extended => Err(Error::UnexpectedMessage(
                    MESSAGE_TYPE_SUBMIT_SHARES_STANDARD,
                )),
                SupportedChannelTypes::Group => {
                    self_mutex.safe_lock(|self_| self_.handle_submit_shares_standard(m))?
                }
                SupportedChannelTypes::GroupAndExtended => {
                    self_mutex.safe_lock(|self_| self_.handle_submit_shares_standard(m))?
                }
            },
            Ok(Mining::SubmitSharesExtended(m)) => match channel_type {
                SupportedChannelTypes::Standard => Err(Error::UnexpectedMessage(
                    MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED,
                )),
                SupportedChannelTypes::Extended => {
                    self_mutex.safe_lock(|self_| self_.handle_submit_shares_extended(m))?
                }
                SupportedChannelTypes::Group => Err(Error::UnexpectedMessage(
                    MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED,
                )),
                SupportedChannelTypes::GroupAndExtended => {
                    self_mutex.safe_lock(|self_| self_.handle_submit_shares_extended(m))?
                }
            },
            Ok(Mining::SetCustomMiningJob(m)) => match (channel_type, is_work_selection_enabled) {
                (SupportedChannelTypes::Extended, true) => {
                    self_mutex.safe_lock(|self_| self_.handle_set_custom_mining_job(m))?
                }
                (SupportedChannelTypes::GroupAndExtended, true) => {
                    self_mutex.safe_lock(|self_| self_.handle_set_custom_mining_job(m))?
                }
                _ => Err(Error::UnexpectedMessage(MESSAGE_TYPE_SET_CUSTOM_MINING_JOB)),
            },
            Ok(Mining::CloseChannel(m)) => self_mutex.safe_lock(|x| x.handle_close_channel(m))?,
            Ok(_) => Err(Error::UnexpectedMessage(0)),
            Err(e) => Err(e),
        }
    }

    /// Checks if work selection is enabled for the downstream connection.
    fn is_work_selection_enabled(&self) -> bool;

    /// Checks if the downstream user is authorized.
    fn is_downstream_authorized(
        _self_mutex: Arc<Mutex<Self>>,
        _user_identity: &binary_sv2::Str0255,
    ) -> Result<bool, Error>;

    /// Handles an `OpenStandardMiningChannel` message.
    fn handle_open_standard_mining_channel(
        &mut self,
        m: OpenStandardMiningChannel,
    ) -> Result<SendTo<Up>, Error>;

    /// Handles an `OpenExtendedMiningChannel` message.
    fn handle_open_extended_mining_channel(
        &mut self,
        m: OpenExtendedMiningChannel,
    ) -> Result<SendTo<Up>, Error>;

    /// Handles an `UpdateChannel` message.
    ///
    /// This method processes an `UpdateChannel` message and updates the channel settings.
    fn handle_update_channel(&mut self, m: UpdateChannel) -> Result<SendTo<Up>, Error>;

    /// Handles a `SubmitSharesStandard` message.
    ///
    /// This method processes a `SubmitSharesStandard` message and validates the submitted shares.
    fn handle_submit_shares_standard(
        &mut self,
        m: SubmitSharesStandard,
    ) -> Result<SendTo<Up>, Error>;

    /// Handles a `SubmitSharesExtended` message.
    ///
    /// This method processes a `SubmitSharesExtended` message and validates the submitted shares.
    fn handle_submit_shares_extended(
        &mut self,
        m: SubmitSharesExtended,
    ) -> Result<SendTo<Up>, Error>;

    /// Handles a `SetCustomMiningJob` message.
    ///
    /// This method processes a `SetCustomMiningJob` message and applies the custom mining job
    /// settings.
    fn handle_set_custom_mining_job(&mut self, m: SetCustomMiningJob) -> Result<SendTo<Up>, Error>;

    /// Handles a request to close a mining channel.
    fn handle_close_channel(&mut self, m: CloseChannel) -> Result<SendTo<Up>, Error>;
}

/// A trait defining the parser for upstream mining messages used by a downstream.
///
/// This trait provides the functionality to handle and route various types of mining messages
/// from the upstream based on the message type and payload.
pub trait ParseMiningMessagesFromUpstream<Down: D>
where
    Self: Sized + D,
{
    /// Retrieves the type of the channel supported by this upstream parser.
    fn get_channel_type(&self) -> SupportedChannelTypes;

    /// Parses and routes SV2 mining messages from the upstream based on the message type and
    /// payload. The implementor of DownstreamMining needs to pass a RequestIdMapper if changing
    /// the request ID. Proxies typically need this to ensure the request ID is unique across
    /// the connection.
    fn handle_message_mining(
        self_mutex: Arc<Mutex<Self>>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<SendTo<Down>, Error> {
        match Self::handle_message_mining_deserialized(
            self_mutex,
            (message_type, payload).try_into().map_err(Into::into),
        ) {
            Err(Error::UnexpectedMessage(0)) => Err(Error::UnexpectedMessage(message_type)),
            result => result,
        }
    }

    /// Handles the deserialized mining message from the upstream, processing it according to the
    /// routing logic.
    fn handle_message_mining_deserialized(
        self_mutex: Arc<Mutex<Self>>,
        message: Result<Mining, Error>,
    ) -> Result<SendTo<Down>, Error> {
        let (channel_type, is_work_selection_enabled) =
            self_mutex.safe_lock(|s| (s.get_channel_type(), s.is_work_selection_enabled()))?;

        match message {
            Ok(Mining::OpenStandardMiningChannelSuccess(m)) => {
                match channel_type {
                    SupportedChannelTypes::Standard => self_mutex
                        .safe_lock(|s| s.handle_open_standard_mining_channel_success(m))?,
                    SupportedChannelTypes::Extended => Err(Error::UnexpectedMessage(
                        MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS,
                    )),
                    SupportedChannelTypes::Group => self_mutex
                        .safe_lock(|s| s.handle_open_standard_mining_channel_success(m))?,
                    SupportedChannelTypes::GroupAndExtended => self_mutex
                        .safe_lock(|s| s.handle_open_standard_mining_channel_success(m))?,
                }
            }
            Ok(Mining::OpenExtendedMiningChannelSuccess(m)) => {
                match channel_type {
                    SupportedChannelTypes::Standard => Err(Error::UnexpectedMessage(
                        MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCESS,
                    )),
                    SupportedChannelTypes::Extended => self_mutex
                        .safe_lock(|s| s.handle_open_extended_mining_channel_success(m))?,
                    SupportedChannelTypes::Group => Err(Error::UnexpectedMessage(
                        MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCESS,
                    )),
                    SupportedChannelTypes::GroupAndExtended => self_mutex
                        .safe_lock(|s| s.handle_open_extended_mining_channel_success(m))?,
                }
            }
            Ok(Mining::OpenMiningChannelError(m)) => {
                self_mutex.safe_lock(|x| x.handle_open_mining_channel_error(m))?
            }
            Ok(Mining::UpdateChannelError(m)) => {
                self_mutex.safe_lock(|x| x.handle_update_channel_error(m))?
            }
            Ok(Mining::CloseChannel(m)) => self_mutex.safe_lock(|x| x.handle_close_channel(m))?,

            Ok(Mining::SetExtranoncePrefix(m)) => {
                self_mutex.safe_lock(|x| x.handle_set_extranonce_prefix(m))?
            }
            Ok(Mining::SubmitSharesSuccess(m)) => {
                self_mutex.safe_lock(|x| x.handle_submit_shares_success(m))?
            }
            Ok(Mining::SubmitSharesError(m)) => {
                self_mutex.safe_lock(|x| x.handle_submit_shares_error(m))?
            }
            Ok(Mining::NewMiningJob(m)) => match channel_type {
                SupportedChannelTypes::Standard => {
                    self_mutex.safe_lock(|x| x.handle_new_mining_job(m))?
                }
                SupportedChannelTypes::Extended => {
                    Err(Error::UnexpectedMessage(MESSAGE_TYPE_NEW_MINING_JOB))
                }
                SupportedChannelTypes::Group => {
                    Err(Error::UnexpectedMessage(MESSAGE_TYPE_NEW_MINING_JOB))
                }
                SupportedChannelTypes::GroupAndExtended => {
                    Err(Error::UnexpectedMessage(MESSAGE_TYPE_NEW_MINING_JOB))
                }
            },
            Ok(Mining::NewExtendedMiningJob(m)) => match channel_type {
                SupportedChannelTypes::Standard => Err(Error::UnexpectedMessage(
                    MESSAGE_TYPE_NEW_EXTENDED_MINING_JOB,
                )),
                SupportedChannelTypes::Extended => {
                    self_mutex.safe_lock(|x| x.handle_new_extended_mining_job(m))?
                }
                SupportedChannelTypes::Group => {
                    self_mutex.safe_lock(|x| x.handle_new_extended_mining_job(m))?
                }
                SupportedChannelTypes::GroupAndExtended => {
                    self_mutex.safe_lock(|x| x.handle_new_extended_mining_job(m))?
                }
            },
            Ok(Mining::SetNewPrevHash(m)) => {
                self_mutex.safe_lock(|x| x.handle_set_new_prev_hash(m))?
            }
            Ok(Mining::SetCustomMiningJobSuccess(m)) => {
                match (channel_type, is_work_selection_enabled) {
                    (SupportedChannelTypes::Extended, true) => {
                        self_mutex.safe_lock(|x| x.handle_set_custom_mining_job_success(m))?
                    }
                    (SupportedChannelTypes::GroupAndExtended, true) => {
                        self_mutex.safe_lock(|x| x.handle_set_custom_mining_job_success(m))?
                    }
                    _ => Err(Error::UnexpectedMessage(
                        MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_SUCCESS,
                    )),
                }
            }

            Ok(Mining::SetCustomMiningJobError(m)) => {
                match (channel_type, is_work_selection_enabled) {
                    (SupportedChannelTypes::Extended, true) => {
                        self_mutex.safe_lock(|x| x.handle_set_custom_mining_job_error(m))?
                    }
                    (SupportedChannelTypes::Group, true) => {
                        self_mutex.safe_lock(|x| x.handle_set_custom_mining_job_error(m))?
                    }
                    (SupportedChannelTypes::GroupAndExtended, true) => {
                        self_mutex.safe_lock(|x| x.handle_set_custom_mining_job_error(m))?
                    }
                    _ => Err(Error::UnexpectedMessage(
                        MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_ERROR,
                    )),
                }
            }
            Ok(Mining::SetTarget(m)) => self_mutex.safe_lock(|x| x.handle_set_target(m))?,
            Ok(Mining::SetGroupChannel(m)) => match channel_type {
                SupportedChannelTypes::Standard => {
                    Err(Error::UnexpectedMessage(MESSAGE_TYPE_SET_GROUP_CHANNEL))
                }
                SupportedChannelTypes::Extended => {
                    Err(Error::UnexpectedMessage(MESSAGE_TYPE_SET_GROUP_CHANNEL))
                }
                SupportedChannelTypes::Group => {
                    self_mutex.safe_lock(|x| x.handle_set_group_channel(m))?
                }
                SupportedChannelTypes::GroupAndExtended => {
                    self_mutex.safe_lock(|x| x.handle_set_group_channel(m))?
                }
            },
            Ok(_) => Err(Error::UnexpectedMessage(0)),
            Err(e) => Err(e),
        }
    }

    /// Determines whether work selection is enabled for this upstream.
    fn is_work_selection_enabled(&self) -> bool;

    /// Handles a successful response for opening a standard mining channel.
    fn handle_open_standard_mining_channel_success(
        &mut self,
        m: OpenStandardMiningChannelSuccess,
    ) -> Result<SendTo<Down>, Error>;

    /// Handles a successful response for opening an extended mining channel.
    fn handle_open_extended_mining_channel_success(
        &mut self,
        m: OpenExtendedMiningChannelSuccess,
    ) -> Result<SendTo<Down>, Error>;

    /// Handles an error when opening a mining channel.
    fn handle_open_mining_channel_error(
        &mut self,
        m: OpenMiningChannelError,
    ) -> Result<SendTo<Down>, Error>;

    /// Handles an error when updating a mining channel.
    fn handle_update_channel_error(&mut self, m: UpdateChannelError)
        -> Result<SendTo<Down>, Error>;

    /// Handles a request to close a mining channel.
    fn handle_close_channel(&mut self, m: CloseChannel) -> Result<SendTo<Down>, Error>;

    /// Handles a request to set the extranonce prefix for mining.
    fn handle_set_extranonce_prefix(
        &mut self,
        m: SetExtranoncePrefix,
    ) -> Result<SendTo<Down>, Error>;

    /// Handles a successful submission of shares.
    fn handle_submit_shares_success(
        &mut self,
        m: SubmitSharesSuccess,
    ) -> Result<SendTo<Down>, Error>;

    /// Handles an error when submitting shares.
    fn handle_submit_shares_error(&mut self, m: SubmitSharesError) -> Result<SendTo<Down>, Error>;

    /// Handles a new mining job.
    fn handle_new_mining_job(&mut self, m: NewMiningJob) -> Result<SendTo<Down>, Error>;

    /// Handles a new extended mining job.
    fn handle_new_extended_mining_job(
        &mut self,
        m: NewExtendedMiningJob,
    ) -> Result<SendTo<Down>, Error>;

    /// Handles a request to set the new previous hash.
    fn handle_set_new_prev_hash(&mut self, m: SetNewPrevHash) -> Result<SendTo<Down>, Error>;

    /// Handles a successful response for setting a custom mining job.
    fn handle_set_custom_mining_job_success(
        &mut self,
        m: SetCustomMiningJobSuccess,
    ) -> Result<SendTo<Down>, Error>;

    /// Handles an error when setting a custom mining job.
    fn handle_set_custom_mining_job_error(
        &mut self,
        m: SetCustomMiningJobError,
    ) -> Result<SendTo<Down>, Error>;

    /// Handles a request to set the target for mining.
    fn handle_set_target(&mut self, m: SetTarget) -> Result<SendTo<Down>, Error>;

    /// Handles a request to set the group channel for mining.
    fn handle_set_group_channel(&mut self, _m: SetGroupChannel) -> Result<SendTo<Down>, Error>;
}
