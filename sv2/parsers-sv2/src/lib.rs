#![no_std]

//! # Parsing, Serializing, and Message Type Identification
//!
//! Provides logic to convert raw Stratum V2 (Sv2) message data into Rust types, as well as logic
//! to handle conversions among Sv2 rust types. The crate is `no_std` compatible by default.
//!
//! Most of the logic on this module is tightly coupled with the [`binary_sv2`] crate.
//!
//! ## Responsibilities
//! - **Parsing**: Converts raw Sv2 message bytes into Rust enums ([`CommonMessages`], [`Mining`],
//!   etc.).
//! - **Serialization**: Converts Rust enums back into binary format for transmission.
//! - **Protocol Abstraction**: Separates logic for different Sv2 subprotocols, ensuring modular and
//!   extensible design.
//! - **Message Metadata**: Identifies message types and channel bits for routing and processing.
//!
//! ## Supported Subprotocols
//! - **Common Messages**: Shared across all Sv2 roles.
//! - **Template Distribution**: Handles block templates updates and transaction data.
//! - **Job Declaration**: Manages custom mining job declarations, transactions, and solutions.
//! - **Mining Protocol**: Manages standard mining communication (e.g., job dispatch, shares
//!   submission).

pub mod error;
extern crate alloc;
use alloc::vec::Vec;
use binary_sv2::{
    self,
    decodable::{DecodableField, FieldMarker},
    encodable::EncodableField,
    from_bytes, Deserialize, GetSize,
};
use common_messages_sv2::*;
use core::{
    convert::{TryFrom, TryInto},
    fmt,
};
pub use error::ParserError;
use framing_sv2::framing::Sv2Frame;
use job_declaration_sv2::*;
use mining_sv2::*;
use template_distribution_sv2::*;

use common_messages_sv2::{
    ChannelEndpointChanged, Reconnect, SetupConnection, SetupConnectionError,
    SetupConnectionSuccess,
};
use job_declaration_sv2::{
    AllocateMiningJobToken, AllocateMiningJobTokenSuccess, DeclareMiningJob, DeclareMiningJobError,
    DeclareMiningJobSuccess, ProvideMissingTransactions, ProvideMissingTransactionsSuccess,
    PushSolution,
};
use mining_sv2::{
    CloseChannel, NewExtendedMiningJob, NewMiningJob, OpenExtendedMiningChannel,
    OpenExtendedMiningChannelSuccess, OpenMiningChannelError, OpenStandardMiningChannel,
    OpenStandardMiningChannelSuccess, SetCustomMiningJob, SetCustomMiningJobError,
    SetCustomMiningJobSuccess, SetExtranoncePrefix, SetGroupChannel,
    SetNewPrevHash as MiningSetNewPrevHash, SetTarget, SubmitSharesError, SubmitSharesExtended,
    SubmitSharesStandard, SubmitSharesSuccess, UpdateChannel, UpdateChannelError,
};
use template_distribution_sv2::{
    CoinbaseOutputConstraints, NewTemplate, RequestTransactionData, RequestTransactionDataError,
    RequestTransactionDataSuccess, SetNewPrevHash, SubmitSolution,
};

/// Converts a message type number to a human-readable name
pub fn message_type_to_name(msg_type: u8) -> &'static str {
    match msg_type {
        // Common messages (0x00-0x0F)
        0x00 => "SetupConnection",
        0x01 => "SetupConnectionSuccess",
        0x02 => "SetupConnectionError",
        0x03 => "ChannelEndpointChanged",

        // Mining messages (0x10-0x2F)
        0x10 => "OpenStandardMiningChannel",
        0x11 => "OpenStandardMiningChannelSuccess",
        0x12 => "OpenMiningChannelError",
        0x13 => "OpenExtendedMiningChannel",
        0x14 => "OpenExtendedMiningChannelSuccess",
        0x15 => "NewMiningJob",
        0x16 => "UpdateChannel",
        0x17 => "UpdateChannelError",
        0x18 => "CloseChannel",
        0x19 => "SetExtranoncePrefix",
        0x1a => "SubmitSharesStandard",
        0x1b => "SubmitSharesExtended",
        0x1c => "SubmitSharesSuccess",
        0x1d => "SubmitSharesError",
        0x1f => "NewExtendedMiningJob",
        0x20 => "SetNewPrevHash",
        0x21 => "SetTarget",
        0x22 => "SetCustomMiningJob",
        0x23 => "SetCustomMiningJobSuccess",
        0x24 => "SetCustomMiningJobError",
        0x25 => "Reconnect", // todo: fix this like listed on `const_sv2`
        0x26 => "SetGroupChannel",

        // Job Declaration messages (0x50-0x6F)
        0x50 => "AllocateMiningJobToken",
        0x51 => "AllocateMiningJobTokenSuccess",
        0x55 => "ProvideMissingTransactions",
        0x56 => "ProvideMissingTransactionsSuccess",
        0x57 => "DeclareMiningJob",
        0x58 => "DeclareMiningJobSuccess",
        0x59 => "DeclareMiningJobError",
        0x60 => "PushSolution",

        // Template Distribution messages (0x70-0x7F)
        0x70 => "CoinbaseOutputDataSize",
        0x71 => "NewTemplate",
        0x72 => "SetNewPrevHash",
        0x73 => "RequestTransactionData",
        0x74 => "RequestTransactionDataSuccess",
        0x75 => "RequestTransactionDataError",
        0x76 => "SubmitSolution",

        // Unknown message type
        _ => "Unknown Message",
    }
}

/// Common Sv2 protocol messages used across all subprotocols.
///
/// These messages are essential
/// for initializing connections and managing endpoints.
/// A parser of messages that are common to all Sv2 subprotocols, to be used for parsing raw
/// messages
#[derive(Clone, Debug, PartialEq)]
pub enum CommonMessages<'a> {
    /// Notifies about changes in channel endpoint configuration.
    ChannelEndpointChanged(ChannelEndpointChanged),
    /// Reconnects a client to a new server
    Reconnect(Reconnect<'a>),
    /// Initiates a connection between a client and server.
    SetupConnection(SetupConnection<'a>),
    /// Indicates an error during connection setup.
    SetupConnectionError(SetupConnectionError<'a>),
    /// Acknowledges successful connection setup.
    SetupConnectionSuccess(SetupConnectionSuccess),
}

impl fmt::Display for CommonMessages<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommonMessages::ChannelEndpointChanged(m) => write!(f, "{m}"),
            CommonMessages::Reconnect(m) => write!(f, "{m}"),
            CommonMessages::SetupConnection(m) => write!(f, "{m}"),
            CommonMessages::SetupConnectionError(m) => write!(f, "{m}"),
            CommonMessages::SetupConnectionSuccess(m) => write!(f, "{m}"),
        }
    }
}

/// A parser of messages of Template Distribution subprotocol, to be used for parsing raw messages
#[derive(Clone, Debug)]
pub enum TemplateDistribution<'a> {
    CoinbaseOutputConstraints(CoinbaseOutputConstraints),
    NewTemplate(NewTemplate<'a>),
    RequestTransactionData(RequestTransactionData),
    RequestTransactionDataError(RequestTransactionDataError<'a>),
    RequestTransactionDataSuccess(RequestTransactionDataSuccess<'a>),
    SetNewPrevHash(SetNewPrevHash<'a>),
    SubmitSolution(SubmitSolution<'a>),
}

impl fmt::Display for TemplateDistribution<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TemplateDistribution::CoinbaseOutputConstraints(m) => {
                write!(f, "CoinbaseOutputConstraints: {m}")
            }
            TemplateDistribution::NewTemplate(m) => write!(f, "{m}"),
            TemplateDistribution::RequestTransactionData(m) => {
                write!(f, "{m}")
            }
            TemplateDistribution::RequestTransactionDataError(m) => {
                write!(f, "{m}")
            }
            TemplateDistribution::RequestTransactionDataSuccess(m) => {
                write!(f, "{m}")
            }
            TemplateDistribution::SetNewPrevHash(m) => write!(f, "{m}"),
            TemplateDistribution::SubmitSolution(m) => write!(f, "{m}"),
        }
    }
}

/// A parser of messages of Job Declaration subprotocol, to be used for parsing raw messages
#[derive(Clone, Debug)]
pub enum JobDeclaration<'a> {
    AllocateMiningJobToken(AllocateMiningJobToken<'a>),
    AllocateMiningJobTokenSuccess(AllocateMiningJobTokenSuccess<'a>),
    DeclareMiningJob(DeclareMiningJob<'a>),
    DeclareMiningJobError(DeclareMiningJobError<'a>),
    DeclareMiningJobSuccess(DeclareMiningJobSuccess<'a>),
    ProvideMissingTransactions(ProvideMissingTransactions<'a>),
    ProvideMissingTransactionsSuccess(ProvideMissingTransactionsSuccess<'a>),
    PushSolution(PushSolution<'a>),
}

impl fmt::Display for JobDeclaration<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobDeclaration::AllocateMiningJobToken(m) => write!(f, "AllocateMiningJobToken: {m}"),
            JobDeclaration::AllocateMiningJobTokenSuccess(m) => {
                write!(f, "AllocateMiningJobTokenSuccess: {m}")
            }
            JobDeclaration::DeclareMiningJob(m) => write!(f, "DeclareMiningJob: {m}"),
            JobDeclaration::DeclareMiningJobError(m) => write!(f, "DeclareMiningJobError: {m}"),
            JobDeclaration::DeclareMiningJobSuccess(m) => {
                write!(f, "DeclareMiningJobSuccess: {m}")
            }
            JobDeclaration::ProvideMissingTransactions(m) => {
                write!(f, "ProvideMissingTransactions: {m}")
            }
            JobDeclaration::ProvideMissingTransactionsSuccess(m) => {
                write!(f, "ProvideMissingTransactionsSuccess: {m}")
            }
            JobDeclaration::PushSolution(m) => write!(f, "PushSolution: {m}"),
        }
    }
}

/// Mining subprotocol messages: categorization, encapsulation, and parsing.
///
/// Encapsulates mining-related Sv2 protocol messages, providing both a structured representation
/// of parsed messages and an abstraction for communication between mining-related roles. These
/// messages are essential for managing mining channels, distributing jobs, and processing shares.
///
/// ## Purpose
/// - **Parsing Raw Messages**:
///   - Converts raw binary Sv2 mining subprotocol messages into strongly-typed Rust
///     representations.
///   - Simplifies deserialization by mapping raw data directly to the appropriate enum variant.
///   - Once parsed, the [`Mining`] enum provides a structured interface that can be passed through
///     routing and processing layers in roles like proxies or pools.
/// - **Encapsulation**:
///   - Groups mining-related messages into a unified type, abstracting away low-level subprotocol
///     details and making it easier to interact with Sv2 protocol messages.
/// - **Facilitating Modular Handling**:
///   - Categorizes mining messages under a single enum, enabling roles (e.g., proxies or pools) to
///     route and process messages more efficiently using pattern matching and centralized logic.
/// - **Bridging Parsed Messages and Role Logic**:
///   - Acts as a bridge between parsed subprotocol messages and role-specific logic, providing a
///     unified interface for handling mining-related communication. This reduces complexity and
///     ensures consistency across roles.
#[derive(Clone, Debug)]
pub enum Mining<'a> {
    CloseChannel(CloseChannel<'a>),
    NewExtendedMiningJob(NewExtendedMiningJob<'a>),
    NewMiningJob(NewMiningJob<'a>),
    OpenExtendedMiningChannel(OpenExtendedMiningChannel<'a>),
    OpenExtendedMiningChannelSuccess(OpenExtendedMiningChannelSuccess<'a>),
    OpenMiningChannelError(OpenMiningChannelError<'a>),
    OpenStandardMiningChannel(OpenStandardMiningChannel<'a>),
    OpenStandardMiningChannelSuccess(OpenStandardMiningChannelSuccess<'a>),
    SetCustomMiningJob(SetCustomMiningJob<'a>),
    SetCustomMiningJobError(SetCustomMiningJobError<'a>),
    SetCustomMiningJobSuccess(SetCustomMiningJobSuccess),
    SetExtranoncePrefix(SetExtranoncePrefix<'a>),
    SetGroupChannel(SetGroupChannel<'a>),
    SetNewPrevHash(MiningSetNewPrevHash<'a>),
    SetTarget(SetTarget<'a>),
    SubmitSharesError(SubmitSharesError<'a>),
    SubmitSharesExtended(SubmitSharesExtended<'a>),
    SubmitSharesStandard(SubmitSharesStandard),
    SubmitSharesSuccess(SubmitSharesSuccess),
    UpdateChannel(UpdateChannel<'a>),
    UpdateChannelError(UpdateChannelError<'a>),
}

impl fmt::Display for Mining<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Mining::CloseChannel(m) => write!(f, "{m}"),
            Mining::NewExtendedMiningJob(m) => write!(f, "{m}"),
            Mining::NewMiningJob(m) => write!(f, "{m}"),
            Mining::OpenExtendedMiningChannel(m) => write!(f, "{m}"),
            Mining::OpenExtendedMiningChannelSuccess(m) => write!(f, "{m}"),
            Mining::OpenMiningChannelError(m) => write!(f, "{m}"),
            Mining::OpenStandardMiningChannel(m) => write!(f, "{m}"),
            Mining::OpenStandardMiningChannelSuccess(m) => write!(f, "{m}"),
            Mining::SetCustomMiningJob(m) => write!(f, "{m}"),
            Mining::SetCustomMiningJobError(m) => write!(f, "{m}"),
            Mining::SetCustomMiningJobSuccess(m) => write!(f, "{m}"),
            Mining::SetExtranoncePrefix(m) => write!(f, "{m}"),
            Mining::SetGroupChannel(m) => write!(f, "{m}"),
            Mining::SetNewPrevHash(m) => write!(f, "{m}"),
            Mining::SetTarget(m) => write!(f, "{m}"),
            Mining::SubmitSharesError(m) => write!(f, "{m}"),
            Mining::SubmitSharesExtended(m) => write!(f, "{m}"),
            Mining::SubmitSharesStandard(m) => write!(f, "{m}"),
            Mining::SubmitSharesSuccess(m) => write!(f, "{m}"),
            Mining::UpdateChannel(m) => write!(f, "{m}"),
            Mining::UpdateChannelError(m) => write!(f, "{m}"),
        }
    }
}

impl Mining<'_> {
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

impl CommonMessages<'_> {
    /// converter into static lifetime
    pub fn into_static(self) -> CommonMessages<'static> {
        match self {
            CommonMessages::ChannelEndpointChanged(m) => CommonMessages::ChannelEndpointChanged(m),
            CommonMessages::Reconnect(m) => CommonMessages::Reconnect(m.into_static()),
            CommonMessages::SetupConnection(m) => CommonMessages::SetupConnection(m.into_static()),
            CommonMessages::SetupConnectionError(m) => {
                CommonMessages::SetupConnectionError(m.into_static())
            }
            CommonMessages::SetupConnectionSuccess(m) => CommonMessages::SetupConnectionSuccess(m),
        }
    }
}

impl TemplateDistribution<'_> {
    /// converter into static lifetime
    pub fn into_static(self) -> TemplateDistribution<'static> {
        match self {
            TemplateDistribution::CoinbaseOutputConstraints(m) => {
                TemplateDistribution::CoinbaseOutputConstraints(m)
            }
            TemplateDistribution::NewTemplate(m) => {
                TemplateDistribution::NewTemplate(m.into_static())
            }
            TemplateDistribution::RequestTransactionData(m) => {
                TemplateDistribution::RequestTransactionData(m)
            }
            TemplateDistribution::RequestTransactionDataError(m) => {
                TemplateDistribution::RequestTransactionDataError(m.into_static())
            }
            TemplateDistribution::RequestTransactionDataSuccess(m) => {
                TemplateDistribution::RequestTransactionDataSuccess(m.into_static())
            }
            TemplateDistribution::SetNewPrevHash(m) => {
                TemplateDistribution::SetNewPrevHash(m.into_static())
            }
            TemplateDistribution::SubmitSolution(m) => {
                TemplateDistribution::SubmitSolution(m.into_static())
            }
        }
    }
}

impl JobDeclaration<'_> {
    /// converter into static lifetime
    pub fn into_static(self) -> JobDeclaration<'static> {
        match self {
            JobDeclaration::AllocateMiningJobToken(m) => {
                JobDeclaration::AllocateMiningJobToken(m.into_static())
            }
            JobDeclaration::AllocateMiningJobTokenSuccess(m) => {
                JobDeclaration::AllocateMiningJobTokenSuccess(m.into_static())
            }
            JobDeclaration::DeclareMiningJob(m) => {
                JobDeclaration::DeclareMiningJob(m.into_static())
            }
            JobDeclaration::DeclareMiningJobError(m) => {
                JobDeclaration::DeclareMiningJobError(m.into_static())
            }
            JobDeclaration::DeclareMiningJobSuccess(m) => {
                JobDeclaration::DeclareMiningJobSuccess(m.into_static())
            }
            JobDeclaration::ProvideMissingTransactions(m) => {
                JobDeclaration::ProvideMissingTransactions(m.into_static())
            }
            JobDeclaration::ProvideMissingTransactionsSuccess(m) => {
                JobDeclaration::ProvideMissingTransactionsSuccess(m.into_static())
            }
            JobDeclaration::PushSolution(m) => JobDeclaration::PushSolution(m.into_static()),
        }
    }
}

impl AnyMessage<'_> {
    /// converter into static lifetime
    pub fn into_static(self) -> AnyMessage<'static> {
        match self {
            AnyMessage::Common(m) => AnyMessage::Common(m.into_static()),
            AnyMessage::Mining(m) => AnyMessage::Mining(m.into_static()),
            AnyMessage::JobDeclaration(m) => AnyMessage::JobDeclaration(m.into_static()),
            AnyMessage::TemplateDistribution(m) => {
                AnyMessage::TemplateDistribution(m.into_static())
            }
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

impl IsSv2Message for CommonMessages<'_> {
    fn message_type(&self) -> u8 {
        match self {
            Self::ChannelEndpointChanged(_) => MESSAGE_TYPE_CHANNEL_ENDPOINT_CHANGED,
            Self::Reconnect(_) => MESSAGE_TYPE_RECONNECT,
            Self::SetupConnection(_) => MESSAGE_TYPE_SETUP_CONNECTION,
            Self::SetupConnectionError(_) => MESSAGE_TYPE_SETUP_CONNECTION_ERROR,
            Self::SetupConnectionSuccess(_) => MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
        }
    }

    fn channel_bit(&self) -> bool {
        match self {
            Self::ChannelEndpointChanged(_) => CHANNEL_BIT_CHANNEL_ENDPOINT_CHANGED,
            Self::Reconnect(_) => CHANNEL_BIT_RECONNECT,
            Self::SetupConnection(_) => CHANNEL_BIT_SETUP_CONNECTION,
            Self::SetupConnectionError(_) => CHANNEL_BIT_SETUP_CONNECTION_ERROR,
            Self::SetupConnectionSuccess(_) => CHANNEL_BIT_SETUP_CONNECTION_SUCCESS,
        }
    }
}

impl IsSv2Message for TemplateDistribution<'_> {
    fn message_type(&self) -> u8 {
        match self {
            Self::CoinbaseOutputConstraints(_) => MESSAGE_TYPE_COINBASE_OUTPUT_CONSTRAINTS,
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
            Self::CoinbaseOutputConstraints(_) => CHANNEL_BIT_COINBASE_OUTPUT_CONSTRAINTS,
            Self::NewTemplate(_) => CHANNEL_BIT_NEW_TEMPLATE,
            Self::RequestTransactionData(_) => CHANNEL_BIT_REQUEST_TRANSACTION_DATA,
            Self::RequestTransactionDataError(_) => CHANNEL_BIT_REQUEST_TRANSACTION_DATA_ERROR,
            Self::RequestTransactionDataSuccess(_) => CHANNEL_BIT_REQUEST_TRANSACTION_DATA_SUCCESS,
            Self::SetNewPrevHash(_) => CHANNEL_BIT_SET_NEW_PREV_HASH,
            Self::SubmitSolution(_) => CHANNEL_BIT_SUBMIT_SOLUTION,
        }
    }
}
impl IsSv2Message for JobDeclaration<'_> {
    fn message_type(&self) -> u8 {
        match self {
            Self::AllocateMiningJobToken(_) => MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN,
            Self::AllocateMiningJobTokenSuccess(_) => {
                MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN_SUCCESS
            }
            Self::DeclareMiningJob(_) => MESSAGE_TYPE_DECLARE_MINING_JOB,
            Self::DeclareMiningJobSuccess(_) => MESSAGE_TYPE_DECLARE_MINING_JOB_SUCCESS,
            Self::DeclareMiningJobError(_) => MESSAGE_TYPE_DECLARE_MINING_JOB_ERROR,
            Self::ProvideMissingTransactions(_) => MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS,
            Self::ProvideMissingTransactionsSuccess(_) => {
                MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS_SUCCESS
            }
            Self::PushSolution(_) => MESSAGE_TYPE_PUSH_SOLUTION,
        }
    }
    fn channel_bit(&self) -> bool {
        match self {
            Self::AllocateMiningJobToken(_) => CHANNEL_BIT_ALLOCATE_MINING_JOB_TOKEN,
            Self::AllocateMiningJobTokenSuccess(_) => CHANNEL_BIT_ALLOCATE_MINING_JOB_TOKEN_SUCCESS,
            Self::DeclareMiningJob(_) => CHANNEL_BIT_DECLARE_MINING_JOB,
            Self::DeclareMiningJobSuccess(_) => CHANNEL_BIT_DECLARE_MINING_JOB_SUCCESS,
            Self::DeclareMiningJobError(_) => CHANNEL_BIT_DECLARE_MINING_JOB_ERROR,
            Self::ProvideMissingTransactions(_) => CHANNEL_BIT_PROVIDE_MISSING_TRANSACTIONS,
            Self::ProvideMissingTransactionsSuccess(_) => {
                CHANNEL_BIT_PROVIDE_MISSING_TRANSACTIONS_SUCCESS
            }
            Self::PushSolution(_) => CHANNEL_BIT_SUBMIT_SOLUTION_JD,
        }
    }
}
impl IsSv2Message for Mining<'_> {
    fn message_type(&self) -> u8 {
        match self {
            Self::CloseChannel(_) => MESSAGE_TYPE_CLOSE_CHANNEL,
            Self::NewExtendedMiningJob(_) => MESSAGE_TYPE_NEW_EXTENDED_MINING_JOB,
            Self::NewMiningJob(_) => MESSAGE_TYPE_NEW_MINING_JOB,
            Self::OpenExtendedMiningChannel(_) => MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL,
            Self::OpenExtendedMiningChannelSuccess(_) => {
                MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCESS
            }
            Self::OpenMiningChannelError(_) => MESSAGE_TYPE_OPEN_MINING_CHANNEL_ERROR,
            Self::OpenStandardMiningChannel(_) => MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL,
            Self::OpenStandardMiningChannelSuccess(_) => {
                MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS
            }
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
                CHANNEL_BIT_OPEN_EXTENDED_MINING_CHANNEL_SUCCESS
            }
            Self::OpenMiningChannelError(_) => CHANNEL_BIT_OPEN_MINING_CHANNEL_ERROR,
            Self::OpenStandardMiningChannel(_) => CHANNEL_BIT_OPEN_STANDARD_MINING_CHANNEL,
            Self::OpenStandardMiningChannelSuccess(_) => {
                CHANNEL_BIT_OPEN_STANDARD_MINING_CHANNEL_SUCCESS
            }
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

impl<'decoder> From<CommonMessages<'decoder>> for EncodableField<'decoder> {
    fn from(m: CommonMessages<'decoder>) -> Self {
        match m {
            CommonMessages::ChannelEndpointChanged(a) => a.into(),
            CommonMessages::Reconnect(a) => a.into(),
            CommonMessages::SetupConnection(a) => a.into(),
            CommonMessages::SetupConnectionError(a) => a.into(),
            CommonMessages::SetupConnectionSuccess(a) => a.into(),
        }
    }
}
impl<'decoder> From<TemplateDistribution<'decoder>> for EncodableField<'decoder> {
    fn from(m: TemplateDistribution<'decoder>) -> Self {
        match m {
            TemplateDistribution::CoinbaseOutputConstraints(a) => a.into(),
            TemplateDistribution::NewTemplate(a) => a.into(),
            TemplateDistribution::RequestTransactionData(a) => a.into(),
            TemplateDistribution::RequestTransactionDataError(a) => a.into(),
            TemplateDistribution::RequestTransactionDataSuccess(a) => a.into(),
            TemplateDistribution::SetNewPrevHash(a) => a.into(),
            TemplateDistribution::SubmitSolution(a) => a.into(),
        }
    }
}
impl<'decoder> From<JobDeclaration<'decoder>> for EncodableField<'decoder> {
    fn from(m: JobDeclaration<'decoder>) -> Self {
        match m {
            JobDeclaration::AllocateMiningJobToken(a) => a.into(),
            JobDeclaration::AllocateMiningJobTokenSuccess(a) => a.into(),
            JobDeclaration::DeclareMiningJob(a) => a.into(),
            JobDeclaration::DeclareMiningJobSuccess(a) => a.into(),
            JobDeclaration::DeclareMiningJobError(a) => a.into(),
            JobDeclaration::ProvideMissingTransactions(a) => a.into(),
            JobDeclaration::ProvideMissingTransactionsSuccess(a) => a.into(),
            JobDeclaration::PushSolution(a) => a.into(),
        }
    }
}

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
            CommonMessages::Reconnect(a) => a.get_size(),
            CommonMessages::SetupConnection(a) => a.get_size(),
            CommonMessages::SetupConnectionError(a) => a.get_size(),
            CommonMessages::SetupConnectionSuccess(a) => a.get_size(),
        }
    }
}
impl GetSize for TemplateDistribution<'_> {
    fn get_size(&self) -> usize {
        match self {
            TemplateDistribution::CoinbaseOutputConstraints(a) => a.get_size(),
            TemplateDistribution::NewTemplate(a) => a.get_size(),
            TemplateDistribution::RequestTransactionData(a) => a.get_size(),
            TemplateDistribution::RequestTransactionDataError(a) => a.get_size(),
            TemplateDistribution::RequestTransactionDataSuccess(a) => a.get_size(),
            TemplateDistribution::SetNewPrevHash(a) => a.get_size(),
            TemplateDistribution::SubmitSolution(a) => a.get_size(),
        }
    }
}
impl GetSize for JobDeclaration<'_> {
    fn get_size(&self) -> usize {
        match self {
            JobDeclaration::AllocateMiningJobToken(a) => a.get_size(),
            JobDeclaration::AllocateMiningJobTokenSuccess(a) => a.get_size(),
            JobDeclaration::DeclareMiningJob(a) => a.get_size(),
            JobDeclaration::DeclareMiningJobSuccess(a) => a.get_size(),
            JobDeclaration::DeclareMiningJobError(a) => a.get_size(),
            JobDeclaration::ProvideMissingTransactions(a) => a.get_size(),
            JobDeclaration::ProvideMissingTransactionsSuccess(a) => a.get_size(),
            JobDeclaration::PushSolution(a) => a.get_size(),
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

impl<'decoder> Deserialize<'decoder> for CommonMessages<'decoder> {
    fn get_structure(_v: &[u8]) -> core::result::Result<Vec<FieldMarker>, binary_sv2::Error> {
        unimplemented!()
    }
    fn from_decoded_fields(
        _v: Vec<DecodableField<'decoder>>,
    ) -> core::result::Result<Self, binary_sv2::Error> {
        unimplemented!()
    }
}
impl<'decoder> Deserialize<'decoder> for TemplateDistribution<'decoder> {
    fn get_structure(_v: &[u8]) -> core::result::Result<Vec<FieldMarker>, binary_sv2::Error> {
        unimplemented!()
    }
    fn from_decoded_fields(
        _v: Vec<DecodableField<'decoder>>,
    ) -> core::result::Result<Self, binary_sv2::Error> {
        unimplemented!()
    }
}
impl<'decoder> Deserialize<'decoder> for JobDeclaration<'decoder> {
    fn get_structure(_v: &[u8]) -> core::result::Result<Vec<FieldMarker>, binary_sv2::Error> {
        unimplemented!()
    }
    fn from_decoded_fields(
        _v: Vec<DecodableField<'decoder>>,
    ) -> core::result::Result<Self, binary_sv2::Error> {
        unimplemented!()
    }
}
impl<'decoder> Deserialize<'decoder> for Mining<'decoder> {
    fn get_structure(_v: &[u8]) -> core::result::Result<Vec<FieldMarker>, binary_sv2::Error> {
        unimplemented!()
    }
    fn from_decoded_fields(
        _v: Vec<DecodableField<'decoder>>,
    ) -> core::result::Result<Self, binary_sv2::Error> {
        unimplemented!()
    }
}

impl<'decoder> Deserialize<'decoder> for AnyMessage<'decoder> {
    fn get_structure(_v: &[u8]) -> core::result::Result<Vec<FieldMarker>, binary_sv2::Error> {
        unimplemented!()
    }
    fn from_decoded_fields(
        _v: Vec<DecodableField<'decoder>>,
    ) -> core::result::Result<Self, binary_sv2::Error> {
        unimplemented!()
    }
}

impl<'decoder> Deserialize<'decoder> for MiningDeviceMessages<'decoder> {
    fn get_structure(_v: &[u8]) -> core::result::Result<Vec<FieldMarker>, binary_sv2::Error> {
        unimplemented!()
    }
    fn from_decoded_fields(
        _v: Vec<DecodableField<'decoder>>,
    ) -> core::result::Result<Self, binary_sv2::Error> {
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
    Reconnect = MESSAGE_TYPE_RECONNECT,
}

impl TryFrom<u8> for CommonMessageTypes {
    type Error = ParserError;

    fn try_from(v: u8) -> Result<CommonMessageTypes, ParserError> {
        match v {
            MESSAGE_TYPE_SETUP_CONNECTION => Ok(CommonMessageTypes::SetupConnection),
            MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS => Ok(CommonMessageTypes::SetupConnectionSuccess),
            MESSAGE_TYPE_SETUP_CONNECTION_ERROR => Ok(CommonMessageTypes::SetupConnectionError),
            MESSAGE_TYPE_CHANNEL_ENDPOINT_CHANGED => Ok(CommonMessageTypes::ChannelEndpointChanged),
            MESSAGE_TYPE_RECONNECT => Ok(CommonMessageTypes::Reconnect),
            _ => Err(ParserError::UnexpectedMessage(v)),
        }
    }
}

impl<'a> TryFrom<(u8, &'a mut [u8])> for CommonMessages<'a> {
    type Error = ParserError;

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
            CommonMessageTypes::Reconnect => {
                let message: Reconnect = from_bytes(v.1)?;
                Ok(CommonMessages::Reconnect(message))
            }
        }
    }
}

/// A list of 8-bit message type variants under Template Distribution subprotocol
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
#[allow(clippy::enum_variant_names)]
pub enum TemplateDistributionTypes {
    CoinbaseOutputConstraints = MESSAGE_TYPE_COINBASE_OUTPUT_CONSTRAINTS,
    NewTemplate = MESSAGE_TYPE_NEW_TEMPLATE,
    SetNewPrevHash = MESSAGE_TYPE_SET_NEW_PREV_HASH,
    RequestTransactionData = MESSAGE_TYPE_REQUEST_TRANSACTION_DATA,
    RequestTransactionDataSuccess = MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_SUCCESS,
    RequestTransactionDataError = MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_ERROR,
    SubmitSolution = MESSAGE_TYPE_SUBMIT_SOLUTION,
}

impl TryFrom<u8> for TemplateDistributionTypes {
    type Error = ParserError;

    fn try_from(v: u8) -> Result<TemplateDistributionTypes, ParserError> {
        match v {
            MESSAGE_TYPE_COINBASE_OUTPUT_CONSTRAINTS => {
                Ok(TemplateDistributionTypes::CoinbaseOutputConstraints)
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
            _ => Err(ParserError::UnexpectedMessage(v)),
        }
    }
}

impl<'a> TryFrom<(u8, &'a mut [u8])> for TemplateDistribution<'a> {
    type Error = ParserError;

    fn try_from(v: (u8, &'a mut [u8])) -> Result<Self, Self::Error> {
        let msg_type: TemplateDistributionTypes = v.0.try_into()?;
        match msg_type {
            TemplateDistributionTypes::CoinbaseOutputConstraints => {
                let message: CoinbaseOutputConstraints = from_bytes(v.1)?;
                Ok(TemplateDistribution::CoinbaseOutputConstraints(message))
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
    ProvideMissingTransactions = MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS,
    ProvideMissingTransactionsSuccess = MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS_SUCCESS,
    PushSolution = MESSAGE_TYPE_PUSH_SOLUTION,
}

impl TryFrom<u8> for JobDeclarationTypes {
    type Error = ParserError;

    fn try_from(v: u8) -> Result<JobDeclarationTypes, ParserError> {
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
            MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS => {
                Ok(JobDeclarationTypes::ProvideMissingTransactions)
            }
            MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS_SUCCESS => {
                Ok(JobDeclarationTypes::ProvideMissingTransactionsSuccess)
            }
            MESSAGE_TYPE_PUSH_SOLUTION => Ok(JobDeclarationTypes::PushSolution),
            _ => Err(ParserError::UnexpectedMessage(v)),
        }
    }
}

impl<'a> TryFrom<(u8, &'a mut [u8])> for JobDeclaration<'a> {
    type Error = ParserError;

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
            JobDeclarationTypes::ProvideMissingTransactions => {
                let message: ProvideMissingTransactions = from_bytes(v.1)?;
                Ok(JobDeclaration::ProvideMissingTransactions(message))
            }
            JobDeclarationTypes::ProvideMissingTransactionsSuccess => {
                let message: ProvideMissingTransactionsSuccess = from_bytes(v.1)?;
                Ok(JobDeclaration::ProvideMissingTransactionsSuccess(message))
            }
            JobDeclarationTypes::PushSolution => {
                let message: PushSolution = from_bytes(v.1)?;
                Ok(JobDeclaration::PushSolution(message))
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
    OpenExtendedMiningChannelSuccess = MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCESS,
    OpenMiningChannelError = MESSAGE_TYPE_OPEN_MINING_CHANNEL_ERROR,
    OpenStandardMiningChannel = MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL,
    OpenStandardMiningChannelSuccess = MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS,
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
    type Error = ParserError;

    fn try_from(v: u8) -> Result<MiningTypes, ParserError> {
        match v {
            MESSAGE_TYPE_CLOSE_CHANNEL => Ok(MiningTypes::CloseChannel),
            MESSAGE_TYPE_NEW_EXTENDED_MINING_JOB => Ok(MiningTypes::NewExtendedMiningJob),
            MESSAGE_TYPE_NEW_MINING_JOB => Ok(MiningTypes::NewMiningJob),
            MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL => Ok(MiningTypes::OpenExtendedMiningChannel),
            MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCESS => {
                Ok(MiningTypes::OpenExtendedMiningChannelSuccess)
            }
            MESSAGE_TYPE_OPEN_MINING_CHANNEL_ERROR => Ok(MiningTypes::OpenMiningChannelError),
            MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL => Ok(MiningTypes::OpenStandardMiningChannel),
            MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS => {
                Ok(MiningTypes::OpenStandardMiningChannelSuccess)
            }
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
            MESSAGE_TYPE_SETUP_CONNECTION => Err(ParserError::UnexpectedMessage(v)),
            _ => Err(ParserError::UnexpectedMessage(v)),
        }
    }
}

impl<'a> TryFrom<(u8, &'a mut [u8])> for Mining<'a> {
    type Error = ParserError;

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
pub enum MiningDeviceMessages<'a> {
    Common(CommonMessages<'a>),
    Mining(Mining<'a>),
}
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
    type Error = ParserError;

    fn try_from(v: (u8, &'a mut [u8])) -> Result<Self, Self::Error> {
        let is_common: Result<CommonMessageTypes, ParserError> = v.0.try_into();
        let is_mining: Result<MiningTypes, ParserError> = v.0.try_into();
        match (is_common, is_mining) {
            (Ok(_), Err(_)) => Ok(Self::Common(v.try_into()?)),
            (Err(_), Ok(_)) => Ok(Self::Mining(v.try_into()?)),
            (Err(e), Err(_)) => Err(e),
            // this is an impossible state is safe to panic here
            (Ok(_), Ok(_)) => panic!(),
        }
    }
}

/// A parser of all possible SV2 messages
#[derive(Clone, Debug)]
pub enum AnyMessage<'a> {
    Common(CommonMessages<'a>),
    Mining(Mining<'a>),
    JobDeclaration(JobDeclaration<'a>),
    TemplateDistribution(TemplateDistribution<'a>),
}

impl fmt::Display for AnyMessage<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AnyMessage::Common(m) => write!(f, "CommonMessage: {m}"),
            AnyMessage::Mining(m) => write!(f, "MiningMessage: {m}"),
            AnyMessage::JobDeclaration(m) => write!(f, "JobDeclarationMessage: {m}"),
            AnyMessage::TemplateDistribution(m) => write!(f, "TemplateDistributionMessage: {m}"),
        }
    }
}

impl<'a> TryFrom<MiningDeviceMessages<'a>> for AnyMessage<'a> {
    type Error = ParserError;

    fn try_from(value: MiningDeviceMessages<'a>) -> Result<Self, Self::Error> {
        match value {
            MiningDeviceMessages::Common(m) => Ok(AnyMessage::Common(m)),
            MiningDeviceMessages::Mining(m) => Ok(AnyMessage::Mining(m)),
        }
    }
}

impl<'decoder> From<AnyMessage<'decoder>> for EncodableField<'decoder> {
    fn from(m: AnyMessage<'decoder>) -> Self {
        match m {
            AnyMessage::Common(a) => a.into(),
            AnyMessage::Mining(a) => a.into(),
            AnyMessage::JobDeclaration(a) => a.into(),
            AnyMessage::TemplateDistribution(a) => a.into(),
        }
    }
}
impl GetSize for AnyMessage<'_> {
    fn get_size(&self) -> usize {
        match self {
            AnyMessage::Common(a) => a.get_size(),
            AnyMessage::Mining(a) => a.get_size(),
            AnyMessage::JobDeclaration(a) => a.get_size(),
            AnyMessage::TemplateDistribution(a) => a.get_size(),
        }
    }
}

impl IsSv2Message for AnyMessage<'_> {
    fn message_type(&self) -> u8 {
        match self {
            AnyMessage::Common(a) => a.message_type(),
            AnyMessage::Mining(a) => a.message_type(),
            AnyMessage::JobDeclaration(a) => a.message_type(),
            AnyMessage::TemplateDistribution(a) => a.message_type(),
        }
    }

    fn channel_bit(&self) -> bool {
        match self {
            AnyMessage::Common(a) => a.channel_bit(),
            AnyMessage::Mining(a) => a.channel_bit(),
            AnyMessage::JobDeclaration(a) => a.channel_bit(),
            AnyMessage::TemplateDistribution(a) => a.channel_bit(),
        }
    }
}

impl IsSv2Message for MiningDeviceMessages<'_> {
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

impl<'a> TryFrom<(u8, &'a mut [u8])> for AnyMessage<'a> {
    type Error = ParserError;

    fn try_from(v: (u8, &'a mut [u8])) -> Result<Self, Self::Error> {
        let is_common: Result<CommonMessageTypes, ParserError> = v.0.try_into();
        let is_mining: Result<MiningTypes, ParserError> = v.0.try_into();
        let is_job_declaration: Result<JobDeclarationTypes, ParserError> = v.0.try_into();
        let is_template_distribution: Result<TemplateDistributionTypes, ParserError> =
            v.0.try_into();
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

impl From<SetupConnectionSuccess> for CommonMessages<'_> {
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

impl<'a, T: Into<CommonMessages<'a>>> From<T> for AnyMessage<'a> {
    fn from(v: T) -> Self {
        AnyMessage::Common(v.into())
    }
}

impl<'a, T: Into<CommonMessages<'a>>> From<T> for MiningDeviceMessages<'a> {
    fn from(v: T) -> Self {
        MiningDeviceMessages::Common(v.into())
    }
}

impl<'decoder, B: AsMut<[u8]> + AsRef<[u8]>> TryFrom<AnyMessage<'decoder>>
    for Sv2Frame<AnyMessage<'decoder>, B>
{
    type Error = ParserError;

    fn try_from(v: AnyMessage<'decoder>) -> Result<Self, ParserError> {
        let extension_type = 0;
        let channel_bit = v.channel_bit();
        let message_type = v.message_type();
        Sv2Frame::from_message(v, message_type, extension_type, channel_bit)
            .ok_or(ParserError::BadPayloadSize)
    }
}

impl<'decoder, B: AsMut<[u8]> + AsRef<[u8]>> TryFrom<MiningDeviceMessages<'decoder>>
    for Sv2Frame<MiningDeviceMessages<'decoder>, B>
{
    type Error = ParserError;

    fn try_from(v: MiningDeviceMessages<'decoder>) -> Result<Self, ParserError> {
        let extension_type = 0;
        let channel_bit = v.channel_bit();
        let message_type = v.message_type();
        Sv2Frame::from_message(v, message_type, extension_type, channel_bit)
            .ok_or(ParserError::BadPayloadSize)
    }
}

impl<'decoder, B: AsMut<[u8]> + AsRef<[u8]>> TryFrom<TemplateDistribution<'decoder>>
    for Sv2Frame<TemplateDistribution<'decoder>, B>
{
    type Error = ParserError;

    fn try_from(v: TemplateDistribution<'decoder>) -> Result<Self, ParserError> {
        let extension_type = 0;
        let channel_bit = v.channel_bit();
        let message_type = v.message_type();
        Sv2Frame::from_message(v, message_type, extension_type, channel_bit)
            .ok_or(ParserError::BadPayloadSize)
    }
}

impl<'a> TryFrom<AnyMessage<'a>> for MiningDeviceMessages<'a> {
    type Error = ParserError;

    fn try_from(value: AnyMessage<'a>) -> Result<Self, ParserError> {
        match value {
            AnyMessage::Common(message) => Ok(Self::Common(message)),
            AnyMessage::Mining(message) => Ok(Self::Mining(message)),
            AnyMessage::JobDeclaration(_) => Err(ParserError::UnexpectedPoolMessage),
            AnyMessage::TemplateDistribution(_) => Err(ParserError::UnexpectedPoolMessage),
        }
    }
}

#[cfg(test)]
mod test {
    use crate::{AnyMessage, Mining};
    use alloc::vec::Vec;
    use binary_sv2::{Sv2Option, U256};
    use codec_sv2::StandardSv2Frame;
    use core::convert::{TryFrom, TryInto};
    use mining_sv2::NewMiningJob;

    pub type Message = AnyMessage<'static>;
    pub type StdFrame = StandardSv2Frame<Message>;

    #[test]
    fn new_mining_job_serialization() {
        const CORRECTLY_SERIALIZED_MSG: &[u8] = &[
            0, 128, 21, 49, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 1, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18,
            19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40,
            41, 42, 43, 44, 45, 46, 47, 48,
        ];
        let mining_message = AnyMessage::Mining(Mining::NewMiningJob(NewMiningJob {
            channel_id: u32::from_le_bytes([1, 2, 3, 4]),
            job_id: u32::from_le_bytes([5, 6, 7, 8]),
            min_ntime: Sv2Option::new(Some(u32::from_le_bytes([9, 10, 11, 12]))),
            version: u32::from_le_bytes([13, 14, 15, 16]),
            merkle_root: U256::try_from((17_u8..(17 + 32)).collect::<Vec<u8>>()).unwrap(),
        }));
        message_serialization_check(mining_message, CORRECTLY_SERIALIZED_MSG);
    }

    fn message_serialization_check(message: AnyMessage<'static>, expected_result: &[u8]) {
        let frame = StdFrame::try_from(message).unwrap();
        let encoded_frame_length = frame.encoded_length();

        let mut buffer = [0; 0xffff];
        frame.serialize(&mut buffer).unwrap();
        check_length_consistency(&buffer[..encoded_frame_length]);
        check_length_consistency(expected_result);
        assert_eq!(
            is_channel_msg(&buffer),
            is_channel_msg(expected_result),
            "Unexpected channel_message flag",
        );
        assert_eq!(
            extract_extension_type(&buffer),
            extract_extension_type(expected_result),
            "Unexpected extension type",
        );
        assert_eq!(
            extract_message_type(&buffer),
            extract_message_type(expected_result),
            "Unexpected message type",
        );
        assert_eq!(
            extract_payload_length(&buffer),
            extract_payload_length(expected_result),
            "Unexpected message length",
        );
        assert_eq!(
            encoded_frame_length as u32,
            expected_result.len() as u32,
            "Method encoded_length() returned different length than the actual message length",
        );
        assert_eq!(
            extract_payload(&buffer[..encoded_frame_length]),
            extract_payload(expected_result),
            "Unexpected payload",
        )
    }

    fn is_channel_msg(serialized_frame: &[u8]) -> bool {
        let array_repre = serialized_frame[..2].try_into().unwrap();
        let decoded_extension_type = u16::from_le_bytes(array_repre);
        (decoded_extension_type & (1 << 15)) != 0
    }
    fn extract_extension_type(serialized_frame: &[u8]) -> u16 {
        let array_repre = serialized_frame[..2].try_into().unwrap();
        let decoded_extension_type = u16::from_le_bytes(array_repre);
        decoded_extension_type & (u16::MAX >> 1)
    }
    fn extract_message_type(serialized_frame: &[u8]) -> u8 {
        serialized_frame[2]
    }
    fn extract_payload_length(serialized_frame: &[u8]) -> u32 {
        let mut array_repre = [0; 4];
        array_repre[..3].copy_from_slice(&serialized_frame[3..6]);
        u32::from_le_bytes(array_repre)
    }
    fn extract_payload(serialized_frame: &[u8]) -> &[u8] {
        assert!(serialized_frame.len() > 6);
        &serialized_frame[6..]
    }

    fn check_length_consistency(serialized_frame: &[u8]) {
        let message_length = extract_payload_length(serialized_frame) as usize;
        let payload_length = extract_payload(serialized_frame).len();
        assert_eq!(
            message_length, payload_length,
            "Header declared length [{message_length} bytes] differs from the actual payload length [{payload_length} bytes]",
        );
    }
}
