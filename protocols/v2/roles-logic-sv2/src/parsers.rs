//! The parsers module provides logic to convert raw Sv2 message data into rust types,
//! as well as logic to handle conversions among Sv2 rust types
//!
//! Most of the logic on this module is tightly coupled with the binary_sv2 crate.

use crate::Error;

#[cfg(not(feature = "with_serde"))]
use binary_sv2::{decodable::DecodableField, decodable::FieldMarker, encodable::EncodableField};

#[cfg(feature = "with_serde")]
use binary_sv2::Serialize;

use binary_sv2::GetSize;

use binary_sv2::{from_bytes, Deserialize};

use framing_sv2::framing::Sv2Frame;

use const_sv2::{
    CHANNEL_BIT_ALLOCATE_MINING_JOB_TOKEN, CHANNEL_BIT_ALLOCATE_MINING_JOB_TOKEN_SUCCESS,
    CHANNEL_BIT_CHANNEL_ENDPOINT_CHANGED, CHANNEL_BIT_CLOSE_CHANNEL,
    CHANNEL_BIT_COINBASE_OUTPUT_DATA_SIZE, CHANNEL_BIT_DECLARE_MINING_JOB,
    CHANNEL_BIT_DECLARE_MINING_JOB_ERROR, CHANNEL_BIT_DECLARE_MINING_JOB_SUCCESS,
    CHANNEL_BIT_IDENTIFY_TRANSACTIONS, CHANNEL_BIT_IDENTIFY_TRANSACTIONS_SUCCESS,
    CHANNEL_BIT_MINING_SET_NEW_PREV_HASH, CHANNEL_BIT_NEW_EXTENDED_MINING_JOB,
    CHANNEL_BIT_NEW_MINING_JOB, CHANNEL_BIT_NEW_TEMPLATE, CHANNEL_BIT_OPEN_EXTENDED_MINING_CHANNEL,
    CHANNEL_BIT_OPEN_EXTENDED_MINING_CHANNEL_SUCCES, CHANNEL_BIT_OPEN_MINING_CHANNEL_ERROR,
    CHANNEL_BIT_OPEN_STANDARD_MINING_CHANNEL, CHANNEL_BIT_OPEN_STANDARD_MINING_CHANNEL_SUCCESS,
    CHANNEL_BIT_PROVIDE_MISSING_TRANSACTIONS, CHANNEL_BIT_PROVIDE_MISSING_TRANSACTIONS_SUCCESS,
    CHANNEL_BIT_RECONNECT, CHANNEL_BIT_REQUEST_TRANSACTION_DATA,
    CHANNEL_BIT_REQUEST_TRANSACTION_DATA_ERROR, CHANNEL_BIT_REQUEST_TRANSACTION_DATA_SUCCESS,
    CHANNEL_BIT_SETUP_CONNECTION, CHANNEL_BIT_SETUP_CONNECTION_ERROR,
    CHANNEL_BIT_SETUP_CONNECTION_SUCCESS, CHANNEL_BIT_SET_CUSTOM_MINING_JOB,
    CHANNEL_BIT_SET_CUSTOM_MINING_JOB_ERROR, CHANNEL_BIT_SET_CUSTOM_MINING_JOB_SUCCESS,
    CHANNEL_BIT_SET_EXTRANONCE_PREFIX, CHANNEL_BIT_SET_GROUP_CHANNEL,
    CHANNEL_BIT_SET_NEW_PREV_HASH, CHANNEL_BIT_SET_TARGET, CHANNEL_BIT_SUBMIT_SHARES_ERROR,
    CHANNEL_BIT_SUBMIT_SHARES_EXTENDED, CHANNEL_BIT_SUBMIT_SHARES_STANDARD,
    CHANNEL_BIT_SUBMIT_SHARES_SUCCESS, CHANNEL_BIT_SUBMIT_SOLUTION, CHANNEL_BIT_SUBMIT_SOLUTION_JD,
    CHANNEL_BIT_UPDATE_CHANNEL, CHANNEL_BIT_UPDATE_CHANNEL_ERROR,
    MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN, MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN_SUCCESS,
    MESSAGE_TYPE_CHANNEL_ENDPOINT_CHANGED, MESSAGE_TYPE_CLOSE_CHANNEL,
    MESSAGE_TYPE_COINBASE_OUTPUT_DATA_SIZE, MESSAGE_TYPE_DECLARE_MINING_JOB,
    MESSAGE_TYPE_DECLARE_MINING_JOB_ERROR, MESSAGE_TYPE_DECLARE_MINING_JOB_SUCCESS,
    MESSAGE_TYPE_IDENTIFY_TRANSACTIONS, MESSAGE_TYPE_IDENTIFY_TRANSACTIONS_SUCCESS,
    MESSAGE_TYPE_MINING_SET_NEW_PREV_HASH, MESSAGE_TYPE_NEW_EXTENDED_MINING_JOB,
    MESSAGE_TYPE_NEW_MINING_JOB, MESSAGE_TYPE_NEW_TEMPLATE,
    MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL, MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCES,
    MESSAGE_TYPE_OPEN_MINING_CHANNEL_ERROR, MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL,
    MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS, MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS,
    MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS_SUCCESS, MESSAGE_TYPE_RECONNECT,
    MESSAGE_TYPE_REQUEST_TRANSACTION_DATA, MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_ERROR,
    MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_SUCCESS, MESSAGE_TYPE_SETUP_CONNECTION,
    MESSAGE_TYPE_SETUP_CONNECTION_ERROR, MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
    MESSAGE_TYPE_SET_CUSTOM_MINING_JOB, MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_ERROR,
    MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_SUCCESS, MESSAGE_TYPE_SET_EXTRANONCE_PREFIX,
    MESSAGE_TYPE_SET_GROUP_CHANNEL, MESSAGE_TYPE_SET_NEW_PREV_HASH, MESSAGE_TYPE_SET_TARGET,
    MESSAGE_TYPE_SUBMIT_SHARES_ERROR, MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED,
    MESSAGE_TYPE_SUBMIT_SHARES_STANDARD, MESSAGE_TYPE_SUBMIT_SHARES_SUCCESS,
    MESSAGE_TYPE_SUBMIT_SOLUTION, MESSAGE_TYPE_SUBMIT_SOLUTION_JD, MESSAGE_TYPE_UPDATE_CHANNEL,
    MESSAGE_TYPE_UPDATE_CHANNEL_ERROR,
};

use common_messages_sv2::{
    ChannelEndpointChanged, SetupConnection, SetupConnectionError, SetupConnectionSuccess,
};

use template_distribution_sv2::{
    CoinbaseOutputDataSize, NewTemplate, RequestTransactionData, RequestTransactionDataError,
    RequestTransactionDataSuccess, SetNewPrevHash, SubmitSolution,
};

use job_declaration_sv2::{
    AllocateMiningJobToken, AllocateMiningJobTokenSuccess, DeclareMiningJob, DeclareMiningJobError,
    DeclareMiningJobSuccess, IdentifyTransactions, IdentifyTransactionsSuccess,
    ProvideMissingTransactions, ProvideMissingTransactionsSuccess, SubmitSolutionJd,
};

use mining_sv2::{
    CloseChannel, NewExtendedMiningJob, NewMiningJob, OpenExtendedMiningChannel,
    OpenExtendedMiningChannelSuccess, OpenMiningChannelError, OpenStandardMiningChannel,
    OpenStandardMiningChannelSuccess, Reconnect, SetCustomMiningJob, SetCustomMiningJobError,
    SetCustomMiningJobSuccess, SetExtranoncePrefix, SetGroupChannel,
    SetNewPrevHash as MiningSetNewPrevHash, SetTarget, SubmitSharesError, SubmitSharesExtended,
    SubmitSharesStandard, SubmitSharesSuccess, UpdateChannel, UpdateChannelError,
};

use core::convert::{TryFrom, TryInto};
use tracing::error;

// todo: fix this, PoolMessages shouldn't be a generic parser
/// An alias to a generic parser
pub type AnyMessage<'a> = PoolMessages<'a>;

/// A parser of messages that are common to all Sv2 subprotocols, to be used for parsing raw
/// messages
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "with_serde", derive(Serialize, Deserialize))]
pub enum CommonMessages<'a> {
    ChannelEndpointChanged(ChannelEndpointChanged),
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    SetupConnection(SetupConnection<'a>),
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    SetupConnectionError(SetupConnectionError<'a>),
    SetupConnectionSuccess(SetupConnectionSuccess),
}

/// A parser of messages of Template Distribution subprotocol, to be used for parsing raw messages
#[derive(Clone, Debug)]
#[cfg_attr(feature = "with_serde", derive(Serialize, Deserialize))]
pub enum TemplateDistribution<'a> {
    CoinbaseOutputDataSize(CoinbaseOutputDataSize),
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    NewTemplate(NewTemplate<'a>),
    RequestTransactionData(RequestTransactionData),
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    RequestTransactionDataError(RequestTransactionDataError<'a>),
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    RequestTransactionDataSuccess(RequestTransactionDataSuccess<'a>),
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    SetNewPrevHash(SetNewPrevHash<'a>),
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    SubmitSolution(SubmitSolution<'a>),
}

/// A parser of messages of Job Declaration subprotocol, to be used for parsing raw messages
#[derive(Clone, Debug)]
#[cfg_attr(feature = "with_serde", derive(Serialize, Deserialize))]
pub enum JobDeclaration<'a> {
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    AllocateMiningJobToken(AllocateMiningJobToken<'a>),
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    AllocateMiningJobTokenSuccess(AllocateMiningJobTokenSuccess<'a>),
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    DeclareMiningJob(DeclareMiningJob<'a>),
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    DeclareMiningJobError(DeclareMiningJobError<'a>),
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    DeclareMiningJobSuccess(DeclareMiningJobSuccess<'a>),
    IdentifyTransactions(IdentifyTransactions),
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    IdentifyTransactionsSuccess(IdentifyTransactionsSuccess<'a>),
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    ProvideMissingTransactions(ProvideMissingTransactions<'a>),
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    ProvideMissingTransactionsSuccess(ProvideMissingTransactionsSuccess<'a>),
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    SubmitSolution(SubmitSolutionJd<'a>),
}

/// A parser of messages of Mining subprotocol, to be used for parsing raw messages
#[derive(Clone, Debug)]
#[cfg_attr(feature = "with_serde", derive(Serialize, Deserialize))]
pub enum Mining<'a> {
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    CloseChannel(CloseChannel<'a>),
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    NewExtendedMiningJob(NewExtendedMiningJob<'a>),
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    NewMiningJob(NewMiningJob<'a>),
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    OpenExtendedMiningChannel(OpenExtendedMiningChannel<'a>),
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    OpenExtendedMiningChannelSuccess(OpenExtendedMiningChannelSuccess<'a>),
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    OpenMiningChannelError(OpenMiningChannelError<'a>),
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    OpenStandardMiningChannel(OpenStandardMiningChannel<'a>),
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    OpenStandardMiningChannelSuccess(OpenStandardMiningChannelSuccess<'a>),
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    Reconnect(Reconnect<'a>),
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    SetCustomMiningJob(SetCustomMiningJob<'a>),
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    SetCustomMiningJobError(SetCustomMiningJobError<'a>),
    SetCustomMiningJobSuccess(SetCustomMiningJobSuccess),
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    SetExtranoncePrefix(SetExtranoncePrefix<'a>),
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    SetGroupChannel(SetGroupChannel<'a>),
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    SetNewPrevHash(MiningSetNewPrevHash<'a>),
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    SetTarget(SetTarget<'a>),
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    SubmitSharesError(SubmitSharesError<'a>),
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    SubmitSharesExtended(SubmitSharesExtended<'a>),
    SubmitSharesStandard(SubmitSharesStandard),
    SubmitSharesSuccess(SubmitSharesSuccess),
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    UpdateChannel(UpdateChannel<'a>),
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    UpdateChannelError(UpdateChannelError<'a>),
}

impl<'a> Mining<'a> {
    /// converter into static lifetime
    pub fn into_static(self) -> Mining<'static> {
        match self {
            Mining::CloseChannel(m) => Mining::CloseChannel(m.into_static()),
            Mining::NewExtendedMiningJob(m) => Mining::NewExtendedMiningJob(m.into_static()),
            Mining::NewMiningJob(m) => Mining::NewMiningJob(m.into_static()),
            Mining::OpenExtendedMiningChannel(m) => {
                Mining::OpenExtendedMiningChannel(m.into_static())
            }
            Mining::OpenExtendedMiningChannelSuccess(m) => {
                Mining::OpenExtendedMiningChannelSuccess(m.into_static())
            }
            Mining::OpenMiningChannelError(m) => Mining::OpenMiningChannelError(m.into_static()),
            Mining::OpenStandardMiningChannel(m) => {
                Mining::OpenStandardMiningChannel(m.into_static())
            }
            Mining::OpenStandardMiningChannelSuccess(m) => {
                Mining::OpenStandardMiningChannelSuccess(m.into_static())
            }
            Mining::Reconnect(m) => Mining::Reconnect(m.into_static()),
            Mining::SetCustomMiningJob(m) => Mining::SetCustomMiningJob(m.into_static()),
            Mining::SetCustomMiningJobError(m) => Mining::SetCustomMiningJobError(m.into_static()),
            Mining::SetCustomMiningJobSuccess(m) => {
                Mining::SetCustomMiningJobSuccess(m.into_static())
            }
            Mining::SetExtranoncePrefix(m) => Mining::SetExtranoncePrefix(m.into_static()),
            Mining::SetGroupChannel(m) => Mining::SetGroupChannel(m.into_static()),
            Mining::SetNewPrevHash(m) => Mining::SetNewPrevHash(m.into_static()),
            Mining::SetTarget(m) => Mining::SetTarget(m.into_static()),
            Mining::SubmitSharesError(m) => Mining::SubmitSharesError(m.into_static()),
            Mining::SubmitSharesExtended(m) => Mining::SubmitSharesExtended(m.into_static()),
            Mining::SubmitSharesStandard(m) => Mining::SubmitSharesStandard(m),
            Mining::SubmitSharesSuccess(m) => Mining::SubmitSharesSuccess(m),
            Mining::UpdateChannel(m) => Mining::UpdateChannel(m.into_static()),
            Mining::UpdateChannelError(m) => Mining::UpdateChannelError(m.into_static()),
        }
    }
}

/// A trait that every Sv2 message parser must implement.
/// It helps parsing from Rust types to raw messages.
pub trait IsSv2Message {
    /// get message type
    fn message_type(&self) -> u8;
    /// get channel bit
    fn channel_bit(&self) -> bool;
}

impl<'a> IsSv2Message for CommonMessages<'a> {
    fn message_type(&self) -> u8 {
        match self {
            Self::ChannelEndpointChanged(_) => MESSAGE_TYPE_CHANNEL_ENDPOINT_CHANGED,
            Self::SetupConnection(_) => MESSAGE_TYPE_SETUP_CONNECTION,
            Self::SetupConnectionError(_) => MESSAGE_TYPE_SETUP_CONNECTION_ERROR,
            Self::SetupConnectionSuccess(_) => MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
        }
    }

    fn channel_bit(&self) -> bool {
        match self {
            Self::ChannelEndpointChanged(_) => CHANNEL_BIT_CHANNEL_ENDPOINT_CHANGED,
            Self::SetupConnection(_) => CHANNEL_BIT_SETUP_CONNECTION,
            Self::SetupConnectionError(_) => CHANNEL_BIT_SETUP_CONNECTION_ERROR,
            Self::SetupConnectionSuccess(_) => CHANNEL_BIT_SETUP_CONNECTION_SUCCESS,
        }
    }
}

impl<'a> IsSv2Message for TemplateDistribution<'a> {
    fn message_type(&self) -> u8 {
        match self {
            Self::CoinbaseOutputDataSize(_) => MESSAGE_TYPE_COINBASE_OUTPUT_DATA_SIZE,
            Self::NewTemplate(_) => MESSAGE_TYPE_NEW_TEMPLATE,
            Self::RequestTransactionData(_) => MESSAGE_TYPE_REQUEST_TRANSACTION_DATA,
            Self::RequestTransactionDataError(_) => MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_ERROR,
            Self::RequestTransactionDataSuccess(_) => MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_SUCCESS,
            Self::SetNewPrevHash(_) => MESSAGE_TYPE_SET_NEW_PREV_HASH,
            Self::SubmitSolution(_) => MESSAGE_TYPE_SUBMIT_SOLUTION,
        }
    }
    fn channel_bit(&self) -> bool {
        match self {
            Self::CoinbaseOutputDataSize(_) => CHANNEL_BIT_COINBASE_OUTPUT_DATA_SIZE,
            Self::NewTemplate(_) => CHANNEL_BIT_NEW_TEMPLATE,
            Self::RequestTransactionData(_) => CHANNEL_BIT_REQUEST_TRANSACTION_DATA,
            Self::RequestTransactionDataError(_) => CHANNEL_BIT_REQUEST_TRANSACTION_DATA_ERROR,
            Self::RequestTransactionDataSuccess(_) => CHANNEL_BIT_REQUEST_TRANSACTION_DATA_SUCCESS,
            Self::SetNewPrevHash(_) => CHANNEL_BIT_SET_NEW_PREV_HASH,
            Self::SubmitSolution(_) => CHANNEL_BIT_SUBMIT_SOLUTION,
        }
    }
}
impl<'a> IsSv2Message for JobDeclaration<'a> {
    fn message_type(&self) -> u8 {
        match self {
            Self::AllocateMiningJobToken(_) => MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN,
            Self::AllocateMiningJobTokenSuccess(_) => {
                MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN_SUCCESS
            }
            Self::DeclareMiningJob(_) => MESSAGE_TYPE_DECLARE_MINING_JOB,
            Self::DeclareMiningJobSuccess(_) => MESSAGE_TYPE_DECLARE_MINING_JOB_SUCCESS,
            Self::DeclareMiningJobError(_) => MESSAGE_TYPE_DECLARE_MINING_JOB_ERROR,
            Self::IdentifyTransactions(_) => MESSAGE_TYPE_IDENTIFY_TRANSACTIONS,
            Self::IdentifyTransactionsSuccess(_) => MESSAGE_TYPE_IDENTIFY_TRANSACTIONS_SUCCESS,
            Self::ProvideMissingTransactions(_) => MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS,
            Self::ProvideMissingTransactionsSuccess(_) => {
                MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS_SUCCESS
            }
            Self::SubmitSolution(_) => MESSAGE_TYPE_SUBMIT_SOLUTION_JD,
        }
    }
    fn channel_bit(&self) -> bool {
        match self {
            Self::AllocateMiningJobToken(_) => CHANNEL_BIT_ALLOCATE_MINING_JOB_TOKEN,
            Self::AllocateMiningJobTokenSuccess(_) => CHANNEL_BIT_ALLOCATE_MINING_JOB_TOKEN_SUCCESS,
            Self::DeclareMiningJob(_) => CHANNEL_BIT_DECLARE_MINING_JOB,
            Self::DeclareMiningJobSuccess(_) => CHANNEL_BIT_DECLARE_MINING_JOB_SUCCESS,
            Self::DeclareMiningJobError(_) => CHANNEL_BIT_DECLARE_MINING_JOB_ERROR,
            Self::IdentifyTransactions(_) => CHANNEL_BIT_IDENTIFY_TRANSACTIONS,
            Self::IdentifyTransactionsSuccess(_) => CHANNEL_BIT_IDENTIFY_TRANSACTIONS_SUCCESS,
            Self::ProvideMissingTransactions(_) => CHANNEL_BIT_PROVIDE_MISSING_TRANSACTIONS,
            Self::ProvideMissingTransactionsSuccess(_) => {
                CHANNEL_BIT_PROVIDE_MISSING_TRANSACTIONS_SUCCESS
            }
            Self::SubmitSolution(_) => CHANNEL_BIT_SUBMIT_SOLUTION_JD,
        }
    }
}
impl<'a> IsSv2Message for Mining<'a> {
    fn message_type(&self) -> u8 {
        match self {
            Self::CloseChannel(_) => MESSAGE_TYPE_CLOSE_CHANNEL,
            Self::NewExtendedMiningJob(_) => MESSAGE_TYPE_NEW_EXTENDED_MINING_JOB,
            Self::NewMiningJob(_) => MESSAGE_TYPE_NEW_MINING_JOB,
            Self::OpenExtendedMiningChannel(_) => MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL,
            Self::OpenExtendedMiningChannelSuccess(_) => {
                MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCES
            }
            Self::OpenMiningChannelError(_) => MESSAGE_TYPE_OPEN_MINING_CHANNEL_ERROR,
            Self::OpenStandardMiningChannel(_) => MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL,
            Self::OpenStandardMiningChannelSuccess(_) => {
                MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS
            }
            Self::Reconnect(_) => MESSAGE_TYPE_RECONNECT,
            Self::SetCustomMiningJob(_) => MESSAGE_TYPE_SET_CUSTOM_MINING_JOB,
            Self::SetCustomMiningJobError(_) => MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_ERROR,
            Self::SetCustomMiningJobSuccess(_) => MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_SUCCESS,
            Self::SetExtranoncePrefix(_) => MESSAGE_TYPE_SET_EXTRANONCE_PREFIX,
            Self::SetGroupChannel(_) => MESSAGE_TYPE_SET_GROUP_CHANNEL,
            Self::SetNewPrevHash(_) => MESSAGE_TYPE_MINING_SET_NEW_PREV_HASH,
            Self::SetTarget(_) => MESSAGE_TYPE_SET_TARGET,
            Self::SubmitSharesError(_) => MESSAGE_TYPE_SUBMIT_SHARES_ERROR,
            Self::SubmitSharesExtended(_) => MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED,
            Self::SubmitSharesStandard(_) => MESSAGE_TYPE_SUBMIT_SHARES_STANDARD,
            Self::SubmitSharesSuccess(_) => MESSAGE_TYPE_SUBMIT_SHARES_SUCCESS,
            Self::UpdateChannel(_) => MESSAGE_TYPE_UPDATE_CHANNEL,
            Self::UpdateChannelError(_) => MESSAGE_TYPE_UPDATE_CHANNEL_ERROR,
        }
    }

    fn channel_bit(&self) -> bool {
        match self {
            Self::CloseChannel(_) => CHANNEL_BIT_CLOSE_CHANNEL,
            Self::NewExtendedMiningJob(_) => CHANNEL_BIT_NEW_EXTENDED_MINING_JOB,
            Self::NewMiningJob(_) => CHANNEL_BIT_NEW_MINING_JOB,
            Self::OpenExtendedMiningChannel(_) => CHANNEL_BIT_OPEN_EXTENDED_MINING_CHANNEL,
            Self::OpenExtendedMiningChannelSuccess(_) => {
                CHANNEL_BIT_OPEN_EXTENDED_MINING_CHANNEL_SUCCES
            }
            Self::OpenMiningChannelError(_) => CHANNEL_BIT_OPEN_MINING_CHANNEL_ERROR,
            Self::OpenStandardMiningChannel(_) => CHANNEL_BIT_OPEN_STANDARD_MINING_CHANNEL,
            Self::OpenStandardMiningChannelSuccess(_) => {
                CHANNEL_BIT_OPEN_STANDARD_MINING_CHANNEL_SUCCESS
            }
            Self::Reconnect(_) => CHANNEL_BIT_RECONNECT,
            Self::SetCustomMiningJob(_) => CHANNEL_BIT_SET_CUSTOM_MINING_JOB,
            Self::SetCustomMiningJobError(_) => CHANNEL_BIT_SET_CUSTOM_MINING_JOB_ERROR,
            Self::SetCustomMiningJobSuccess(_) => CHANNEL_BIT_SET_CUSTOM_MINING_JOB_SUCCESS,
            Self::SetExtranoncePrefix(_) => CHANNEL_BIT_SET_EXTRANONCE_PREFIX,
            Self::SetGroupChannel(_) => CHANNEL_BIT_SET_GROUP_CHANNEL,
            Self::SetNewPrevHash(_) => CHANNEL_BIT_MINING_SET_NEW_PREV_HASH,
            Self::SetTarget(_) => CHANNEL_BIT_SET_TARGET,
            Self::SubmitSharesError(_) => CHANNEL_BIT_SUBMIT_SHARES_ERROR,
            Self::SubmitSharesExtended(_) => CHANNEL_BIT_SUBMIT_SHARES_EXTENDED,
            Self::SubmitSharesStandard(_) => CHANNEL_BIT_SUBMIT_SHARES_STANDARD,
            Self::SubmitSharesSuccess(_) => CHANNEL_BIT_SUBMIT_SHARES_SUCCESS,
            Self::UpdateChannel(_) => CHANNEL_BIT_UPDATE_CHANNEL,
            Self::UpdateChannelError(_) => CHANNEL_BIT_UPDATE_CHANNEL_ERROR,
        }
    }
}

#[cfg(not(feature = "with_serde"))]
impl<'decoder> From<CommonMessages<'decoder>> for EncodableField<'decoder> {
    fn from(m: CommonMessages<'decoder>) -> Self {
        match m {
            CommonMessages::ChannelEndpointChanged(a) => a.into(),
            CommonMessages::SetupConnection(a) => a.into(),
            CommonMessages::SetupConnectionError(a) => a.into(),
            CommonMessages::SetupConnectionSuccess(a) => a.into(),
        }
    }
}
#[cfg(not(feature = "with_serde"))]
impl<'decoder> From<TemplateDistribution<'decoder>> for EncodableField<'decoder> {
    fn from(m: TemplateDistribution<'decoder>) -> Self {
        match m {
            TemplateDistribution::CoinbaseOutputDataSize(a) => a.into(),
            TemplateDistribution::NewTemplate(a) => a.into(),
            TemplateDistribution::RequestTransactionData(a) => a.into(),
            TemplateDistribution::RequestTransactionDataError(a) => a.into(),
            TemplateDistribution::RequestTransactionDataSuccess(a) => a.into(),
            TemplateDistribution::SetNewPrevHash(a) => a.into(),
            TemplateDistribution::SubmitSolution(a) => a.into(),
        }
    }
}
#[cfg(not(feature = "with_serde"))]
impl<'decoder> From<JobDeclaration<'decoder>> for EncodableField<'decoder> {
    fn from(m: JobDeclaration<'decoder>) -> Self {
        match m {
            JobDeclaration::AllocateMiningJobToken(a) => a.into(),
            JobDeclaration::AllocateMiningJobTokenSuccess(a) => a.into(),
            JobDeclaration::DeclareMiningJob(a) => a.into(),
            JobDeclaration::DeclareMiningJobSuccess(a) => a.into(),
            JobDeclaration::DeclareMiningJobError(a) => a.into(),
            JobDeclaration::IdentifyTransactions(a) => a.into(),
            JobDeclaration::IdentifyTransactionsSuccess(a) => a.into(),
            JobDeclaration::ProvideMissingTransactions(a) => a.into(),
            JobDeclaration::ProvideMissingTransactionsSuccess(a) => a.into(),
            JobDeclaration::SubmitSolution(a) => a.into(),
        }
    }
}

#[cfg(not(feature = "with_serde"))]
impl<'decoder> From<Mining<'decoder>> for EncodableField<'decoder> {
    fn from(m: Mining<'decoder>) -> Self {
        match m {
            Mining::CloseChannel(a) => a.into(),
            Mining::NewExtendedMiningJob(a) => a.into(),
            Mining::NewMiningJob(a) => a.into(),
            Mining::OpenExtendedMiningChannel(a) => a.into(),
            Mining::OpenExtendedMiningChannelSuccess(a) => a.into(),
            Mining::OpenMiningChannelError(a) => a.into(),
            Mining::OpenStandardMiningChannel(a) => a.into(),
            Mining::OpenStandardMiningChannelSuccess(a) => a.into(),
            Mining::Reconnect(a) => a.into(),
            Mining::SetCustomMiningJob(a) => a.into(),
            Mining::SetCustomMiningJobError(a) => a.into(),
            Mining::SetCustomMiningJobSuccess(a) => a.into(),
            Mining::SetExtranoncePrefix(a) => a.into(),
            Mining::SetGroupChannel(a) => a.into(),
            Mining::SetNewPrevHash(a) => a.into(),
            Mining::SetTarget(a) => a.into(),
            Mining::SubmitSharesError(a) => a.into(),
            Mining::SubmitSharesExtended(a) => a.into(),
            Mining::SubmitSharesStandard(a) => a.into(),
            Mining::SubmitSharesSuccess(a) => a.into(),
            Mining::UpdateChannel(a) => a.into(),
            Mining::UpdateChannelError(a) => a.into(),
        }
    }
}

impl GetSize for CommonMessages<'_> {
    fn get_size(&self) -> usize {
        match self {
            CommonMessages::ChannelEndpointChanged(a) => a.get_size(),
            CommonMessages::SetupConnection(a) => a.get_size(),
            CommonMessages::SetupConnectionError(a) => a.get_size(),
            CommonMessages::SetupConnectionSuccess(a) => a.get_size(),
        }
    }
}
impl GetSize for TemplateDistribution<'_> {
    fn get_size(&self) -> usize {
        match self {
            TemplateDistribution::CoinbaseOutputDataSize(a) => a.get_size(),
            TemplateDistribution::NewTemplate(a) => a.get_size(),
            TemplateDistribution::RequestTransactionData(a) => a.get_size(),
            TemplateDistribution::RequestTransactionDataError(a) => a.get_size(),
            TemplateDistribution::RequestTransactionDataSuccess(a) => a.get_size(),
            TemplateDistribution::SetNewPrevHash(a) => a.get_size(),
            TemplateDistribution::SubmitSolution(a) => a.get_size(),
        }
    }
}
impl<'a> GetSize for JobDeclaration<'a> {
    fn get_size(&self) -> usize {
        match self {
            JobDeclaration::AllocateMiningJobToken(a) => a.get_size(),
            JobDeclaration::AllocateMiningJobTokenSuccess(a) => a.get_size(),
            JobDeclaration::DeclareMiningJob(a) => a.get_size(),
            JobDeclaration::DeclareMiningJobSuccess(a) => a.get_size(),
            JobDeclaration::DeclareMiningJobError(a) => a.get_size(),
            JobDeclaration::IdentifyTransactions(a) => a.get_size(),
            JobDeclaration::IdentifyTransactionsSuccess(a) => a.get_size(),
            JobDeclaration::ProvideMissingTransactions(a) => a.get_size(),
            JobDeclaration::ProvideMissingTransactionsSuccess(a) => a.get_size(),
            JobDeclaration::SubmitSolution(a) => a.get_size(),
        }
    }
}
impl GetSize for Mining<'_> {
    fn get_size(&self) -> usize {
        match self {
            Mining::CloseChannel(a) => a.get_size(),
            Mining::NewExtendedMiningJob(a) => a.get_size(),
            Mining::NewMiningJob(a) => a.get_size(),
            Mining::OpenExtendedMiningChannel(a) => a.get_size(),
            Mining::OpenExtendedMiningChannelSuccess(a) => a.get_size(),
            Mining::OpenMiningChannelError(a) => a.get_size(),
            Mining::OpenStandardMiningChannel(a) => a.get_size(),
            Mining::OpenStandardMiningChannelSuccess(a) => a.get_size(),
            Mining::Reconnect(a) => a.get_size(),
            Mining::SetCustomMiningJob(a) => a.get_size(),
            Mining::SetCustomMiningJobError(a) => a.get_size(),
            Mining::SetCustomMiningJobSuccess(a) => a.get_size(),
            Mining::SetExtranoncePrefix(a) => a.get_size(),
            Mining::SetGroupChannel(a) => a.get_size(),
            Mining::SetNewPrevHash(a) => a.get_size(),
            Mining::SetTarget(a) => a.get_size(),
            Mining::SubmitSharesError(a) => a.get_size(),
            Mining::SubmitSharesExtended(a) => a.get_size(),
            Mining::SubmitSharesStandard(a) => a.get_size(),
            Mining::SubmitSharesSuccess(a) => a.get_size(),
            Mining::UpdateChannel(a) => a.get_size(),
            Mining::UpdateChannelError(a) => a.get_size(),
        }
    }
}

#[cfg(not(feature = "with_serde"))]
impl<'decoder> Deserialize<'decoder> for CommonMessages<'decoder> {
    fn get_structure(_v: &[u8]) -> std::result::Result<Vec<FieldMarker>, binary_sv2::Error> {
        unimplemented!()
    }
    fn from_decoded_fields(
        _v: Vec<DecodableField<'decoder>>,
    ) -> std::result::Result<Self, binary_sv2::Error> {
        unimplemented!()
    }
}
#[cfg(not(feature = "with_serde"))]
impl<'decoder> Deserialize<'decoder> for TemplateDistribution<'decoder> {
    fn get_structure(_v: &[u8]) -> std::result::Result<Vec<FieldMarker>, binary_sv2::Error> {
        unimplemented!()
    }
    fn from_decoded_fields(
        _v: Vec<DecodableField<'decoder>>,
    ) -> std::result::Result<Self, binary_sv2::Error> {
        unimplemented!()
    }
}
#[cfg(not(feature = "with_serde"))]
impl<'decoder> Deserialize<'decoder> for JobDeclaration<'decoder> {
    fn get_structure(_v: &[u8]) -> std::result::Result<Vec<FieldMarker>, binary_sv2::Error> {
        unimplemented!()
    }
    fn from_decoded_fields(
        _v: Vec<DecodableField<'decoder>>,
    ) -> std::result::Result<Self, binary_sv2::Error> {
        unimplemented!()
    }
}
#[cfg(not(feature = "with_serde"))]
impl<'decoder> Deserialize<'decoder> for Mining<'decoder> {
    fn get_structure(_v: &[u8]) -> std::result::Result<Vec<FieldMarker>, binary_sv2::Error> {
        unimplemented!()
    }
    fn from_decoded_fields(
        _v: Vec<DecodableField<'decoder>>,
    ) -> std::result::Result<Self, binary_sv2::Error> {
        unimplemented!()
    }
}

#[cfg(not(feature = "with_serde"))]
impl<'decoder> Deserialize<'decoder> for PoolMessages<'decoder> {
    fn get_structure(_v: &[u8]) -> std::result::Result<Vec<FieldMarker>, binary_sv2::Error> {
        unimplemented!()
    }
    fn from_decoded_fields(
        _v: Vec<DecodableField<'decoder>>,
    ) -> std::result::Result<Self, binary_sv2::Error> {
        unimplemented!()
    }
}

#[cfg(not(feature = "with_serde"))]
impl<'decoder> Deserialize<'decoder> for MiningDeviceMessages<'decoder> {
    fn get_structure(_v: &[u8]) -> std::result::Result<Vec<FieldMarker>, binary_sv2::Error> {
        unimplemented!()
    }
    fn from_decoded_fields(
        _v: Vec<DecodableField<'decoder>>,
    ) -> std::result::Result<Self, binary_sv2::Error> {
        unimplemented!()
    }
}

/// A list of 8-bit message type variants that are common to all Sv2 subprotocols
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
#[allow(clippy::enum_variant_names)]
pub enum CommonMessageTypes {
    SetupConnection = MESSAGE_TYPE_SETUP_CONNECTION,
    SetupConnectionSuccess = MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
    SetupConnectionError = MESSAGE_TYPE_SETUP_CONNECTION_ERROR,
    ChannelEndpointChanged = MESSAGE_TYPE_CHANNEL_ENDPOINT_CHANGED,
}

impl TryFrom<u8> for CommonMessageTypes {
    type Error = Error;

    fn try_from(v: u8) -> Result<CommonMessageTypes, Error> {
        match v {
            MESSAGE_TYPE_SETUP_CONNECTION => Ok(CommonMessageTypes::SetupConnection),
            MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS => Ok(CommonMessageTypes::SetupConnectionSuccess),
            MESSAGE_TYPE_SETUP_CONNECTION_ERROR => Ok(CommonMessageTypes::SetupConnectionError),
            MESSAGE_TYPE_CHANNEL_ENDPOINT_CHANGED => Ok(CommonMessageTypes::ChannelEndpointChanged),
            _ => Err(Error::UnexpectedMessage(v)),
        }
    }
}

impl<'a> TryFrom<(u8, &'a mut [u8])> for CommonMessages<'a> {
    type Error = Error;

    fn try_from(v: (u8, &'a mut [u8])) -> Result<Self, Self::Error> {
        let msg_type: CommonMessageTypes = v.0.try_into()?;
        match msg_type {
            CommonMessageTypes::SetupConnection => {
                let message: SetupConnection<'a> = from_bytes(v.1)?;
                Ok(CommonMessages::SetupConnection(message))
            }
            CommonMessageTypes::SetupConnectionSuccess => {
                let message: SetupConnectionSuccess = from_bytes(v.1)?;
                Ok(CommonMessages::SetupConnectionSuccess(message))
            }
            CommonMessageTypes::SetupConnectionError => {
                let message: SetupConnectionError<'a> = from_bytes(v.1)?;
                Ok(CommonMessages::SetupConnectionError(message))
            }
            CommonMessageTypes::ChannelEndpointChanged => {
                let message: ChannelEndpointChanged = from_bytes(v.1)?;
                Ok(CommonMessages::ChannelEndpointChanged(message))
            }
        }
    }
}

/// A list of 8-bit message type variants under Template Distribution subprotocol
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
#[allow(clippy::enum_variant_names)]
pub enum TemplateDistributionTypes {
    CoinbaseOutputDataSize = MESSAGE_TYPE_COINBASE_OUTPUT_DATA_SIZE,
    NewTemplate = MESSAGE_TYPE_NEW_TEMPLATE,
    SetNewPrevHash = MESSAGE_TYPE_SET_NEW_PREV_HASH,
    RequestTransactionData = MESSAGE_TYPE_REQUEST_TRANSACTION_DATA,
    RequestTransactionDataSuccess = MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_SUCCESS,
    RequestTransactionDataError = MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_ERROR,
    SubmitSolution = MESSAGE_TYPE_SUBMIT_SOLUTION,
}

impl TryFrom<u8> for TemplateDistributionTypes {
    type Error = Error;

    fn try_from(v: u8) -> Result<TemplateDistributionTypes, Error> {
        match v {
            MESSAGE_TYPE_COINBASE_OUTPUT_DATA_SIZE => {
                Ok(TemplateDistributionTypes::CoinbaseOutputDataSize)
            }
            MESSAGE_TYPE_NEW_TEMPLATE => Ok(TemplateDistributionTypes::NewTemplate),
            MESSAGE_TYPE_SET_NEW_PREV_HASH => Ok(TemplateDistributionTypes::SetNewPrevHash),
            MESSAGE_TYPE_REQUEST_TRANSACTION_DATA => {
                Ok(TemplateDistributionTypes::RequestTransactionData)
            }
            MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_SUCCESS => {
                Ok(TemplateDistributionTypes::RequestTransactionDataSuccess)
            }
            MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_ERROR => {
                Ok(TemplateDistributionTypes::RequestTransactionDataError)
            }
            MESSAGE_TYPE_SUBMIT_SOLUTION => Ok(TemplateDistributionTypes::SubmitSolution),
            _ => Err(Error::UnexpectedMessage(v)),
        }
    }
}

impl<'a> TryFrom<(u8, &'a mut [u8])> for TemplateDistribution<'a> {
    type Error = Error;

    fn try_from(v: (u8, &'a mut [u8])) -> Result<Self, Self::Error> {
        let msg_type: TemplateDistributionTypes = v.0.try_into()?;
        match msg_type {
            TemplateDistributionTypes::CoinbaseOutputDataSize => {
                let message: CoinbaseOutputDataSize = from_bytes(v.1)?;
                Ok(TemplateDistribution::CoinbaseOutputDataSize(message))
            }
            TemplateDistributionTypes::NewTemplate => {
                let message: NewTemplate<'a> = from_bytes(v.1)?;
                Ok(TemplateDistribution::NewTemplate(message))
            }
            TemplateDistributionTypes::SetNewPrevHash => {
                let message: SetNewPrevHash<'a> = from_bytes(v.1)?;
                Ok(TemplateDistribution::SetNewPrevHash(message))
            }
            TemplateDistributionTypes::RequestTransactionData => {
                let message: RequestTransactionData = from_bytes(v.1)?;
                Ok(TemplateDistribution::RequestTransactionData(message))
            }
            TemplateDistributionTypes::RequestTransactionDataSuccess => {
                let message: RequestTransactionDataSuccess = from_bytes(v.1)?;
                Ok(TemplateDistribution::RequestTransactionDataSuccess(message))
            }
            TemplateDistributionTypes::RequestTransactionDataError => {
                let message: RequestTransactionDataError = from_bytes(v.1)?;
                Ok(TemplateDistribution::RequestTransactionDataError(message))
            }
            TemplateDistributionTypes::SubmitSolution => {
                let message: SubmitSolution = from_bytes(v.1)?;
                Ok(TemplateDistribution::SubmitSolution(message))
            }
        }
    }
}

/// A list of 8-bit message type variants under Job Declaration subprotocol
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
#[allow(clippy::enum_variant_names)]
pub enum JobDeclarationTypes {
    AllocateMiningJobToken = MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN,
    AllocateMiningJobTokenSuccess = MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN_SUCCESS,
    DeclareMiningJob = MESSAGE_TYPE_DECLARE_MINING_JOB,
    DeclareMiningJobSuccess = MESSAGE_TYPE_DECLARE_MINING_JOB_SUCCESS,
    DeclareMiningJobError = MESSAGE_TYPE_DECLARE_MINING_JOB_ERROR,
    IdentifyTransactions = MESSAGE_TYPE_IDENTIFY_TRANSACTIONS,
    IdentifyTransactionsSuccess = MESSAGE_TYPE_IDENTIFY_TRANSACTIONS_SUCCESS,
    ProvideMissingTransactions = MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS,
    ProvideMissingTransactionsSuccess = MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS_SUCCESS,
    SubmitSolution = MESSAGE_TYPE_SUBMIT_SOLUTION_JD,
}

impl TryFrom<u8> for JobDeclarationTypes {
    type Error = Error;

    fn try_from(v: u8) -> Result<JobDeclarationTypes, Error> {
        match v {
            MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN => {
                Ok(JobDeclarationTypes::AllocateMiningJobToken)
            }
            MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN_SUCCESS => {
                Ok(JobDeclarationTypes::AllocateMiningJobTokenSuccess)
            }
            MESSAGE_TYPE_DECLARE_MINING_JOB => Ok(JobDeclarationTypes::DeclareMiningJob),
            MESSAGE_TYPE_DECLARE_MINING_JOB_SUCCESS => {
                Ok(JobDeclarationTypes::DeclareMiningJobSuccess)
            }
            MESSAGE_TYPE_DECLARE_MINING_JOB_ERROR => Ok(JobDeclarationTypes::DeclareMiningJobError),
            MESSAGE_TYPE_IDENTIFY_TRANSACTIONS => Ok(JobDeclarationTypes::IdentifyTransactions),
            MESSAGE_TYPE_IDENTIFY_TRANSACTIONS_SUCCESS => {
                Ok(JobDeclarationTypes::IdentifyTransactionsSuccess)
            }
            MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS => {
                Ok(JobDeclarationTypes::ProvideMissingTransactions)
            }
            MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS_SUCCESS => {
                Ok(JobDeclarationTypes::ProvideMissingTransactionsSuccess)
            }
            MESSAGE_TYPE_SUBMIT_SOLUTION_JD => Ok(JobDeclarationTypes::SubmitSolution),
            _ => Err(Error::UnexpectedMessage(v)),
        }
    }
}

impl<'a> TryFrom<(u8, &'a mut [u8])> for JobDeclaration<'a> {
    type Error = Error;

    fn try_from(v: (u8, &'a mut [u8])) -> Result<Self, Self::Error> {
        let msg_type: JobDeclarationTypes = v.0.try_into()?;
        match msg_type {
            JobDeclarationTypes::AllocateMiningJobToken => {
                let message: AllocateMiningJobToken = from_bytes(v.1)?;
                Ok(JobDeclaration::AllocateMiningJobToken(message))
            }
            JobDeclarationTypes::AllocateMiningJobTokenSuccess => {
                let message: AllocateMiningJobTokenSuccess = from_bytes(v.1)?;
                Ok(JobDeclaration::AllocateMiningJobTokenSuccess(message))
            }
            JobDeclarationTypes::DeclareMiningJob => {
                let message: DeclareMiningJob = from_bytes(v.1)?;
                Ok(JobDeclaration::DeclareMiningJob(message))
            }
            JobDeclarationTypes::DeclareMiningJobSuccess => {
                let message: DeclareMiningJobSuccess = from_bytes(v.1)?;
                Ok(JobDeclaration::DeclareMiningJobSuccess(message))
            }
            JobDeclarationTypes::DeclareMiningJobError => {
                let message: DeclareMiningJobError = from_bytes(v.1)?;
                Ok(JobDeclaration::DeclareMiningJobError(message))
            }
            JobDeclarationTypes::IdentifyTransactions => {
                let message: IdentifyTransactions = from_bytes(v.1)?;
                Ok(JobDeclaration::IdentifyTransactions(message))
            }
            JobDeclarationTypes::IdentifyTransactionsSuccess => {
                let message: IdentifyTransactionsSuccess = from_bytes(v.1)?;
                Ok(JobDeclaration::IdentifyTransactionsSuccess(message))
            }
            JobDeclarationTypes::ProvideMissingTransactions => {
                let message: ProvideMissingTransactions = from_bytes(v.1)?;
                Ok(JobDeclaration::ProvideMissingTransactions(message))
            }
            JobDeclarationTypes::ProvideMissingTransactionsSuccess => {
                let message: ProvideMissingTransactionsSuccess = from_bytes(v.1)?;
                Ok(JobDeclaration::ProvideMissingTransactionsSuccess(message))
            }
            JobDeclarationTypes::SubmitSolution => {
                let message: SubmitSolutionJd = from_bytes(v.1)?;
                Ok(JobDeclaration::SubmitSolution(message))
            }
        }
    }
}

/// A list of 8-bit message type variants under Mining subprotocol
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
#[allow(clippy::enum_variant_names)]
pub enum MiningTypes {
    CloseChannel = MESSAGE_TYPE_CLOSE_CHANNEL,
    NewExtendedMiningJob = MESSAGE_TYPE_NEW_EXTENDED_MINING_JOB,
    NewMiningJob = MESSAGE_TYPE_NEW_MINING_JOB,
    OpenExtendedMiningChannel = MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL,
    OpenExtendedMiningChannelSuccess = MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCES,
    OpenMiningChannelError = MESSAGE_TYPE_OPEN_MINING_CHANNEL_ERROR,
    OpenStandardMiningChannel = MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL,
    OpenStandardMiningChannelSuccess = MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS,
    Reconnect = MESSAGE_TYPE_RECONNECT,
    SetCustomMiningJob = MESSAGE_TYPE_SET_CUSTOM_MINING_JOB,
    SetCustomMiningJobError = MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_ERROR,
    SetCustomMiningJobSuccess = MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_SUCCESS,
    SetExtranoncePrefix = MESSAGE_TYPE_SET_EXTRANONCE_PREFIX,
    SetGroupChannel = MESSAGE_TYPE_SET_GROUP_CHANNEL,
    SetNewPrevHash = MESSAGE_TYPE_MINING_SET_NEW_PREV_HASH,
    SetTarget = MESSAGE_TYPE_SET_TARGET,
    SubmitSharesError = MESSAGE_TYPE_SUBMIT_SHARES_ERROR,
    SubmitSharesExtended = MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED,
    SubmitSharesStandard = MESSAGE_TYPE_SUBMIT_SHARES_STANDARD,
    SubmitSharesSuccess = MESSAGE_TYPE_SUBMIT_SHARES_SUCCESS,
    UpdateChannel = MESSAGE_TYPE_UPDATE_CHANNEL,
    UpdateChannelError = MESSAGE_TYPE_UPDATE_CHANNEL_ERROR,
}

impl TryFrom<u8> for MiningTypes {
    type Error = Error;

    fn try_from(v: u8) -> Result<MiningTypes, Error> {
        match v {
            MESSAGE_TYPE_CLOSE_CHANNEL => Ok(MiningTypes::CloseChannel),
            MESSAGE_TYPE_NEW_EXTENDED_MINING_JOB => Ok(MiningTypes::NewExtendedMiningJob),
            MESSAGE_TYPE_NEW_MINING_JOB => Ok(MiningTypes::NewMiningJob),
            MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL => Ok(MiningTypes::OpenExtendedMiningChannel),
            MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCES => {
                Ok(MiningTypes::OpenExtendedMiningChannelSuccess)
            }
            MESSAGE_TYPE_OPEN_MINING_CHANNEL_ERROR => Ok(MiningTypes::OpenMiningChannelError),
            MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL => Ok(MiningTypes::OpenStandardMiningChannel),
            MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS => {
                Ok(MiningTypes::OpenStandardMiningChannelSuccess)
            }
            MESSAGE_TYPE_RECONNECT => Ok(MiningTypes::Reconnect),
            MESSAGE_TYPE_SET_CUSTOM_MINING_JOB => Ok(MiningTypes::SetCustomMiningJob),
            MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_ERROR => Ok(MiningTypes::SetCustomMiningJobError),
            MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_SUCCESS => {
                Ok(MiningTypes::SetCustomMiningJobSuccess)
            }
            MESSAGE_TYPE_SET_EXTRANONCE_PREFIX => Ok(MiningTypes::SetExtranoncePrefix),
            MESSAGE_TYPE_SET_GROUP_CHANNEL => Ok(MiningTypes::SetGroupChannel),
            MESSAGE_TYPE_MINING_SET_NEW_PREV_HASH => Ok(MiningTypes::SetNewPrevHash),
            MESSAGE_TYPE_SET_TARGET => Ok(MiningTypes::SetTarget),
            MESSAGE_TYPE_SUBMIT_SHARES_ERROR => Ok(MiningTypes::SubmitSharesError),
            MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED => Ok(MiningTypes::SubmitSharesExtended),
            MESSAGE_TYPE_SUBMIT_SHARES_STANDARD => Ok(MiningTypes::SubmitSharesStandard),
            MESSAGE_TYPE_SUBMIT_SHARES_SUCCESS => Ok(MiningTypes::SubmitSharesSuccess),
            MESSAGE_TYPE_UPDATE_CHANNEL => Ok(MiningTypes::UpdateChannel),
            MESSAGE_TYPE_UPDATE_CHANNEL_ERROR => Ok(MiningTypes::UpdateChannelError),
            MESSAGE_TYPE_SETUP_CONNECTION => Err(Error::UnexpectedMessage(v)),
            _ => {
                error!("Invalid message type: {}", v);
                Err(Error::UnexpectedMessage(v))
            }
        }
    }
}

impl<'a> TryFrom<(u8, &'a mut [u8])> for Mining<'a> {
    type Error = Error;

    fn try_from(v: (u8, &'a mut [u8])) -> Result<Self, Self::Error> {
        let msg_type: MiningTypes = v.0.try_into()?;
        match msg_type {
            MiningTypes::CloseChannel => {
                let message: CloseChannel = from_bytes(v.1)?;
                Ok(Mining::CloseChannel(message))
            }
            MiningTypes::NewExtendedMiningJob => {
                let message: NewExtendedMiningJob = from_bytes(v.1)?;
                Ok(Mining::NewExtendedMiningJob(message))
            }
            MiningTypes::NewMiningJob => {
                let message: NewMiningJob = from_bytes(v.1)?;
                Ok(Mining::NewMiningJob(message))
            }
            MiningTypes::OpenExtendedMiningChannel => {
                let message: OpenExtendedMiningChannel = from_bytes(v.1)?;
                Ok(Mining::OpenExtendedMiningChannel(message))
            }
            MiningTypes::OpenExtendedMiningChannelSuccess => {
                let message: OpenExtendedMiningChannelSuccess = from_bytes(v.1)?;
                Ok(Mining::OpenExtendedMiningChannelSuccess(message))
            }
            MiningTypes::OpenMiningChannelError => {
                let message: OpenMiningChannelError = from_bytes(v.1)?;
                Ok(Mining::OpenMiningChannelError(message))
            }
            MiningTypes::OpenStandardMiningChannel => {
                let message: OpenStandardMiningChannel = from_bytes(v.1)?;
                Ok(Mining::OpenStandardMiningChannel(message))
            }
            MiningTypes::OpenStandardMiningChannelSuccess => {
                let message: OpenStandardMiningChannelSuccess = from_bytes(v.1)?;
                Ok(Mining::OpenStandardMiningChannelSuccess(message))
            }
            MiningTypes::Reconnect => {
                let message: Reconnect = from_bytes(v.1)?;
                Ok(Mining::Reconnect(message))
            }
            MiningTypes::SetCustomMiningJob => {
                let message: SetCustomMiningJob = from_bytes(v.1)?;
                Ok(Mining::SetCustomMiningJob(message))
            }
            MiningTypes::SetCustomMiningJobError => {
                let message: SetCustomMiningJobError = from_bytes(v.1)?;
                Ok(Mining::SetCustomMiningJobError(message))
            }
            MiningTypes::SetCustomMiningJobSuccess => {
                let message: SetCustomMiningJobSuccess = from_bytes(v.1)?;
                Ok(Mining::SetCustomMiningJobSuccess(message))
            }
            MiningTypes::SetExtranoncePrefix => {
                let message: SetExtranoncePrefix = from_bytes(v.1)?;
                Ok(Mining::SetExtranoncePrefix(message))
            }
            MiningTypes::SetGroupChannel => {
                let message: SetGroupChannel = from_bytes(v.1)?;
                Ok(Mining::SetGroupChannel(message))
            }
            MiningTypes::SetNewPrevHash => {
                let message: MiningSetNewPrevHash = from_bytes(v.1)?;
                Ok(Mining::SetNewPrevHash(message))
            }
            MiningTypes::SetTarget => {
                let message: SetTarget = from_bytes(v.1)?;
                Ok(Mining::SetTarget(message))
            }
            MiningTypes::SubmitSharesError => {
                let message: SubmitSharesError = from_bytes(v.1)?;
                Ok(Mining::SubmitSharesError(message))
            }
            MiningTypes::SubmitSharesExtended => {
                let message: SubmitSharesExtended = from_bytes(v.1)?;
                Ok(Mining::SubmitSharesExtended(message))
            }
            MiningTypes::SubmitSharesStandard => {
                let message: SubmitSharesStandard = from_bytes(v.1)?;
                Ok(Mining::SubmitSharesStandard(message))
            }
            MiningTypes::SubmitSharesSuccess => {
                let message: SubmitSharesSuccess = from_bytes(v.1)?;
                Ok(Mining::SubmitSharesSuccess(message))
            }
            MiningTypes::UpdateChannel => {
                let message: UpdateChannel = from_bytes(v.1)?;
                Ok(Mining::UpdateChannel(message))
            }
            MiningTypes::UpdateChannelError => {
                let message: UpdateChannelError = from_bytes(v.1)?;
                Ok(Mining::UpdateChannelError(message))
            }
        }
    }
}

/// A parser of messages that a Mining Device could send
#[derive(Clone, Debug)]
#[cfg_attr(feature = "with_serde", derive(Serialize, Deserialize))]
pub enum MiningDeviceMessages<'a> {
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    Common(CommonMessages<'a>),
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    Mining(Mining<'a>),
}
#[cfg(not(feature = "with_serde"))]
impl<'decoder> From<MiningDeviceMessages<'decoder>> for EncodableField<'decoder> {
    fn from(m: MiningDeviceMessages<'decoder>) -> Self {
        match m {
            MiningDeviceMessages::Common(a) => a.into(),
            MiningDeviceMessages::Mining(a) => a.into(),
        }
    }
}
impl GetSize for MiningDeviceMessages<'_> {
    fn get_size(&self) -> usize {
        match self {
            MiningDeviceMessages::Common(a) => a.get_size(),
            MiningDeviceMessages::Mining(a) => a.get_size(),
        }
    }
}
impl<'a> TryFrom<(u8, &'a mut [u8])> for MiningDeviceMessages<'a> {
    type Error = Error;

    fn try_from(v: (u8, &'a mut [u8])) -> Result<Self, Self::Error> {
        let is_common: Result<CommonMessageTypes, Error> = v.0.try_into();
        let is_mining: Result<MiningTypes, Error> = v.0.try_into();
        match (is_common, is_mining) {
            (Ok(_), Err(_)) => Ok(Self::Common(v.try_into()?)),
            (Err(_), Ok(_)) => Ok(Self::Mining(v.try_into()?)),
            (Err(e), Err(_)) => Err(e),
            // this is an impossible state is safe to panic here
            (Ok(_), Ok(_)) => panic!(),
        }
    }
}

// todo: fix this, PoolMessages should only contain Mining and Common
/// A parser of all messages a Pool could send
#[derive(Clone, Debug)]
#[cfg_attr(feature = "with_serde", derive(Serialize, Deserialize))]
pub enum PoolMessages<'a> {
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    Common(CommonMessages<'a>),
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    Mining(Mining<'a>),
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    JobDeclaration(JobDeclaration<'a>),
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    TemplateDistribution(TemplateDistribution<'a>),
}

impl<'a> TryFrom<MiningDeviceMessages<'a>> for PoolMessages<'a> {
    type Error = Error;

    fn try_from(value: MiningDeviceMessages<'a>) -> Result<Self, Self::Error> {
        match value {
            MiningDeviceMessages::Common(m) => Ok(PoolMessages::Common(m)),
            MiningDeviceMessages::Mining(m) => Ok(PoolMessages::Mining(m)),
        }
    }
}

#[cfg(not(feature = "with_serde"))]
impl<'decoder> From<PoolMessages<'decoder>> for EncodableField<'decoder> {
    fn from(m: PoolMessages<'decoder>) -> Self {
        match m {
            PoolMessages::Common(a) => a.into(),
            PoolMessages::Mining(a) => a.into(),
            PoolMessages::JobDeclaration(a) => a.into(),
            PoolMessages::TemplateDistribution(a) => a.into(),
        }
    }
}
impl GetSize for PoolMessages<'_> {
    fn get_size(&self) -> usize {
        match self {
            PoolMessages::Common(a) => a.get_size(),
            PoolMessages::Mining(a) => a.get_size(),
            PoolMessages::JobDeclaration(a) => a.get_size(),
            PoolMessages::TemplateDistribution(a) => a.get_size(),
        }
    }
}

impl<'a> IsSv2Message for PoolMessages<'a> {
    fn message_type(&self) -> u8 {
        match self {
            PoolMessages::Common(a) => a.message_type(),
            PoolMessages::Mining(a) => a.message_type(),
            PoolMessages::JobDeclaration(a) => a.message_type(),
            PoolMessages::TemplateDistribution(a) => a.message_type(),
        }
    }

    fn channel_bit(&self) -> bool {
        match self {
            PoolMessages::Common(a) => a.channel_bit(),
            PoolMessages::Mining(a) => a.channel_bit(),
            PoolMessages::JobDeclaration(a) => a.channel_bit(),
            PoolMessages::TemplateDistribution(a) => a.channel_bit(),
        }
    }
}

impl<'a> IsSv2Message for MiningDeviceMessages<'a> {
    fn message_type(&self) -> u8 {
        match self {
            MiningDeviceMessages::Common(a) => a.message_type(),
            MiningDeviceMessages::Mining(a) => a.message_type(),
        }
    }

    fn channel_bit(&self) -> bool {
        match self {
            MiningDeviceMessages::Common(a) => a.channel_bit(),
            MiningDeviceMessages::Mining(a) => a.channel_bit(),
        }
    }
}

impl<'a> TryFrom<(u8, &'a mut [u8])> for PoolMessages<'a> {
    type Error = Error;

    fn try_from(v: (u8, &'a mut [u8])) -> Result<Self, Self::Error> {
        let is_common: Result<CommonMessageTypes, Error> = v.0.try_into();
        let is_mining: Result<MiningTypes, Error> = v.0.try_into();
        let is_job_declaration: Result<JobDeclarationTypes, Error> = v.0.try_into();
        let is_template_distribution: Result<TemplateDistributionTypes, Error> = v.0.try_into();
        match (
            is_common,
            is_mining,
            is_job_declaration,
            is_template_distribution,
        ) {
            (Ok(_), Err(_), Err(_), Err(_)) => Ok(Self::Common(v.try_into()?)),
            (Err(_), Ok(_), Err(_), Err(_)) => Ok(Self::Mining(v.try_into()?)),
            (Err(_), Err(_), Ok(_), Err(_)) => Ok(Self::JobDeclaration(v.try_into()?)),
            (Err(_), Err(_), Err(_), Ok(_)) => Ok(Self::TemplateDistribution(v.try_into()?)),
            (Err(e), Err(_), Err(_), Err(_)) => Err(e),
            // This is an impossible state is safe to panic here
            _ => panic!(),
        }
    }
}

impl<'a> From<SetupConnection<'a>> for CommonMessages<'a> {
    fn from(v: SetupConnection<'a>) -> Self {
        CommonMessages::SetupConnection(v)
    }
}

impl<'a> From<SetupConnectionSuccess> for CommonMessages<'a> {
    fn from(v: SetupConnectionSuccess) -> Self {
        CommonMessages::SetupConnectionSuccess(v)
    }
}

impl<'a> From<SetupConnectionError<'a>> for CommonMessages<'a> {
    fn from(v: SetupConnectionError<'a>) -> Self {
        CommonMessages::SetupConnectionError(v)
    }
}

impl<'a> From<OpenStandardMiningChannel<'a>> for Mining<'a> {
    fn from(v: OpenStandardMiningChannel<'a>) -> Self {
        Mining::OpenStandardMiningChannel(v)
    }
}
impl<'a> From<UpdateChannel<'a>> for Mining<'a> {
    fn from(v: UpdateChannel<'a>) -> Self {
        Mining::UpdateChannel(v)
    }
}
impl<'a> From<OpenStandardMiningChannelSuccess<'a>> for Mining<'a> {
    fn from(v: OpenStandardMiningChannelSuccess<'a>) -> Self {
        Mining::OpenStandardMiningChannelSuccess(v)
    }
}

impl<'a, T: Into<CommonMessages<'a>>> From<T> for PoolMessages<'a> {
    fn from(v: T) -> Self {
        PoolMessages::Common(v.into())
    }
}

impl<'a, T: Into<CommonMessages<'a>>> From<T> for MiningDeviceMessages<'a> {
    fn from(v: T) -> Self {
        MiningDeviceMessages::Common(v.into())
    }
}

impl<'decoder, B: AsMut<[u8]> + AsRef<[u8]>> TryFrom<PoolMessages<'decoder>>
    for Sv2Frame<PoolMessages<'decoder>, B>
{
    type Error = Error;

    fn try_from(v: PoolMessages<'decoder>) -> Result<Self, Error> {
        let extension_type = 0;
        let channel_bit = v.channel_bit();
        let message_type = v.message_type();
        Sv2Frame::from_message(v, message_type, extension_type, channel_bit)
            .ok_or(Error::BadPayloadSize)
    }
}

impl<'decoder, B: AsMut<[u8]> + AsRef<[u8]>> TryFrom<MiningDeviceMessages<'decoder>>
    for Sv2Frame<MiningDeviceMessages<'decoder>, B>
{
    type Error = Error;

    fn try_from(v: MiningDeviceMessages<'decoder>) -> Result<Self, Error> {
        let extension_type = 0;
        let channel_bit = v.channel_bit();
        let message_type = v.message_type();
        Sv2Frame::from_message(v, message_type, extension_type, channel_bit)
            .ok_or(Error::BadPayloadSize)
    }
}

impl<'decoder, B: AsMut<[u8]> + AsRef<[u8]>> TryFrom<TemplateDistribution<'decoder>>
    for Sv2Frame<TemplateDistribution<'decoder>, B>
{
    type Error = Error;

    fn try_from(v: TemplateDistribution<'decoder>) -> Result<Self, Error> {
        let extension_type = 0;
        let channel_bit = v.channel_bit();
        let message_type = v.message_type();
        Sv2Frame::from_message(v, message_type, extension_type, channel_bit)
            .ok_or(Error::BadPayloadSize)
    }
}

impl<'a> TryFrom<PoolMessages<'a>> for MiningDeviceMessages<'a> {
    type Error = Error;

    fn try_from(value: PoolMessages<'a>) -> Result<Self, Error> {
        match value {
            PoolMessages::Common(message) => Ok(Self::Common(message)),
            PoolMessages::Mining(message) => Ok(Self::Mining(message)),
            PoolMessages::JobDeclaration(_) => Err(Error::UnexpectedPoolMessage),
            PoolMessages::TemplateDistribution(_) => Err(Error::UnexpectedPoolMessage),
        }
    }
}
