//! # Error Handling
//!
//! This module defines error types and utilities for handling errors in the `roles_logic_sv2`
//! module. It includes the [`Error`] enum for representing various errors.

use crate::{utils::InputError, vardiff::error::VardiffError};
use bitcoin::hashes::FromSliceError;
use channels_sv2::server::error::{ExtendedChannelError, GroupChannelError, StandardChannelError};
use codec_sv2::binary_sv2::Error as BinarySv2Error;
use mining_sv2::ExtendedExtranonceError;
use parsers_sv2::AnyMessage as AllMessages;
use std::{
    fmt::{self, Display, Formatter},
    sync::{MutexGuard, PoisonError},
};

/// Error enum
#[derive(Debug)]
pub enum Error {
    /// Payload size is too big to fit into a frame
    BadPayloadSize,
    /// Expected Length of 32, but received different length
    ExpectedLen32(usize),
    /// Error serializing/deserializing binary format
    BinarySv2Error(BinarySv2Error),
    /// Downstream is not connected anymore
    DownstreamDown,
    /// A channel was attempted to be added to an Upstream, but no groups are specified
    NoGroupsFound,
    /// Unexpected message received.
    UnexpectedMessage(u8),
    /// Extended channels do not have group IDs
    NoGroupIdOnExtendedChannel,
    /// No pairable upstream. Parameters are: (`min_v`, `max_v`, all flags supported)
    NoPairableUpstream((u16, u16, u32)),
    /// Error if the hashmap `future_jobs` field in the `GroupChannelJobDispatcher` is empty.
    NoFutureJobs,
    /// No Downstream's connected
    NoDownstreamsConnected,
    /// PrevHash requires non-existent Job Id
    PrevHashRequireNonExistentJobId(u32),
    /// Request Id not mapped
    RequestIdNotMapped(u32),
    /// There are no upstream connected
    NoUpstreamsConnected,
    /// Protocol has not been implemented, but should be
    UnimplementedProtocol,
    /// Unexpected `PoolMessage` type
    UnexpectedPoolMessage,
    /// Upstream is answering with a wrong request ID {} or
    /// `DownstreamMiningSelector::on_open_standard_channel_request` has not been called
    /// before relaying open channel request to upstream
    UnknownRequestId(u32),
    /// No more extranonces
    NoMoreExtranonces,
    /// A non future job always expect a previous new prev hash
    JobIsNotFutureButPrevHashNotPresent,
    /// If a channel is neither extended or part of a pool,
    /// the only thing to do when a OpenStandardChannel is received
    /// is to relay it upstream with and updated request id
    ChannelIsNeitherExtendedNeitherInAPool,
    /// No more available extranonces for downstream"
    ExtranonceSpaceEnded,
    /// Impossible to calculate merkle root
    ImpossibleToCalculateMerkleRoot,
    /// Group Id not found
    GroupIdNotFound,
    /// A share has been received but no job for it exist
    ShareDoNotMatchAnyJob,
    /// A share has been received but no channel for it exist
    ShareDoNotMatchAnyChannel,
    /// Coinbase prefix + extranonce + coinbase suffix is not a valid coinbase
    InvalidCoinbase,
    /// Value remaining in coinbase output was not correctly updated (it's equal to 0)
    ValueRemainingNotUpdated,
    /// Block header version cannot be bigger than `i32::MAX`
    VersionTooBig,
    /// Tx version cannot be bigger than `i32::MAX`
    TxVersionTooBig,
    /// Tx version cannot be lower than 1
    TxVersionTooLow,
    /// Impossible to decode tx
    TxDecodingError(String),
    /// No downstream has been registered for this channel id
    NotFoundChannelId,
    /// Impossible to create a standard job for channel
    /// because no valid job has been received from upstream yet
    NoValidJob,
    /// Impossible to create an extended job for channel
    /// because no valid job has been received from upstream yet
    NoValidTranslatorJob,
    /// Impossible to retrieve a template for the required job id
    NoTemplateForId,
    /// Impossible to retrieve a template for the required template id
    NoValidTemplate(String),
    /// Invalid extranonce size. Params: (required min, requested)
    InvalidExtranonceSize(u16, u16),
    /// Failed to create ExtendedExtranonce. Param: (error message)
    ExtendedExtranonceCreationFailed(String),
    /// Poison Lock
    PoisonLock(String),
    /// Channel Factory did not update job. Params: (downstream_job_id, upstream_job_id)
    JobNotUpdated(u32, u32),
    /// Impossible to get Target
    TargetError(InputError),
    /// Impossible to get Hashrate
    HashrateError(InputError),
    /// Message is well formatted but can not be handled
    LogicErrorMessage(std::boxed::Box<AllMessages<'static>>),
    /// JD server cannot propagate the block due to missing transactions
    JDSMissingTransactions,
    IoError(std::io::Error),
    FromSliceError(FromSliceError),
    /// Invalid user identity
    InvalidUserIdentity(String),
    ExtranoncePrefixFactoryError(ExtendedExtranonceError),
    FailedToCreateStandardChannel(StandardChannelError),
    Vardiff(VardiffError),
    FailedToCreateExtendedChannel(ExtendedChannelError),
    FailedToUpdateStandardChannel(StandardChannelError),
    FailedToUpdateExtendedChannel(ExtendedChannelError),
    FailedToProcessNewTemplateGroupChannel(GroupChannelError),
    FailedToProcessSetNewPrevHashGroupChannel(GroupChannelError),
    FailedToProcessNewTemplateExtendedChannel(ExtendedChannelError),
    FailedToProcessNewTemplateStandardChannel(StandardChannelError),
    FailedToProcessSetNewPrevHashExtendedChannel(ExtendedChannelError),
    FailedToProcessSetNewPrevHashStandardChannel(StandardChannelError),
    FailedToProcessGroupChannelJob(StandardChannelError),
    NoActiveJob,
    FailedToSendSolution,
    FailedToSetCustomMiningJob(ExtendedChannelError),
    FailedToDeserializeCoinbaseOutputs,
    /// Error from parsers_sv2
    ParserError(parsers_sv2::ParserError),
}

impl From<BinarySv2Error> for Error {
    fn from(v: BinarySv2Error) -> Error {
        Error::BinarySv2Error(v)
    }
}

impl From<std::io::Error> for Error {
    fn from(v: std::io::Error) -> Error {
        Error::IoError(v)
    }
}

impl From<FromSliceError> for Error {
    fn from(v: FromSliceError) -> Error {
        Error::FromSliceError(v)
    }
}

impl From<VardiffError> for Error {
    fn from(value: VardiffError) -> Self {
        Error::Vardiff(value)
    }
}

impl From<parsers_sv2::ParserError> for Error {
    fn from(v: parsers_sv2::ParserError) -> Error {
        Error::ParserError(v)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        use Error::*;
        match self {
            BadPayloadSize => write!(f, "Payload is too big to fit into the frame"),
            BinarySv2Error(v) => write!(
                f,
                "BinarySv2Error: error in serializing/deserializing binary format {v:?}"
            ),
            DownstreamDown => {
                write!(
                    f,
                    "Downstream is not connected anymore"
                )
            }
            ExpectedLen32(l) => write!(f, "Expected length of 32, but received length of {l}"),
            NoGroupsFound => write!(
                f,
                "A channel was attempted to be added to an Upstream, but no groups are specified"
            ),
            UnexpectedMessage(type_) => write!(f, "Error: Unexpected message received. Recv m type: {type_:x}"),
            NoGroupIdOnExtendedChannel => write!(f, "Extended channels do not have group IDs"),
            NoPairableUpstream(a) => {
                write!(f, "No pairable upstream node: {a:?}")
            }
            NoFutureJobs => write!(f, "GroupChannelJobDispatcher does not have any future jobs"),
            NoDownstreamsConnected => write!(f, "NoDownstreamsConnected"),
            PrevHashRequireNonExistentJobId(id) => {
                write!(f, "PrevHashRequireNonExistentJobId {id}")
            }
            RequestIdNotMapped(id) => write!(f, "RequestIdNotMapped {id}"),
            NoUpstreamsConnected => write!(f, "There are no upstream connected"),
            UnexpectedPoolMessage => write!(f, "Unexpected `PoolMessage` type"),
            UnimplementedProtocol => write!(
                f,
                "TODO: `Protocol` has not been implemented, but should be"
            ),
            UnknownRequestId(id) => write!(
                f,
                "Upstream is answering with a wrong request ID {id} or
                DownstreamMiningSelector::on_open_standard_channel_request has not been called
                before relaying open channel request to upstream"
            ),
            InvalidExtranonceSize(required_min, requested) => {
                write!(
                    f,
                    "Invalid extranonce size: required min {required_min}, requested {requested}"
                )
            },
            NoMoreExtranonces => write!(f, "No more extranonces"),
            JobIsNotFutureButPrevHashNotPresent => write!(f, "A non future job always expect a previous new prev hash"),
            ChannelIsNeitherExtendedNeitherInAPool => write!(f, "If a channel is neither extended neither is part of a pool the only thing to do when a OpenStandardChannel is received is to relay it upstream with and updated request id"),
            ExtranonceSpaceEnded => write!(f, "No more available extranonces for downstream"),
            ImpossibleToCalculateMerkleRoot => write!(f, "Impossible to calculate merkle root"),
            GroupIdNotFound => write!(f, "Group id not found"),
            ShareDoNotMatchAnyJob => write!(f, "A share has been received but no job for it exist"),
            ShareDoNotMatchAnyChannel => write!(f, "A share has been received but no channel for it exist"),
            InvalidCoinbase => write!(f, "Coinbase prefix + extranonce + coinbase suffix is not a valid coinbase"),
            ValueRemainingNotUpdated => write!(f, "Value remaining in coinbase output was not correctly updated (it's equal to 0)"),
            VersionTooBig => write!(f, "We are trying to construct a block header with version bigger than i32::MAX"),
            TxVersionTooBig => write!(f, "Tx version can not be greater than i32::MAX"),
            TxVersionTooLow => write!(f, "Tx version can not be lower than 1"),
            TxDecodingError(e) => write!(f, "Impossible to decode tx: {e:?}"),
            NotFoundChannelId => write!(f, "No downstream has been registered for this channel id"),
            NoValidJob => write!(f, "Impossible to create a standard job for channelA cause no valid job has been received from upstream yet"),
            NoValidTranslatorJob => write!(f, "Impossible to create a extended job for channel cause no valid job has been received from upstream yet"),
            NoTemplateForId => write!(f, "Impossible to retrieve a template for the required job id"),
            NoValidTemplate(e) => write!(f, "Impossible to retrieve a template for the required template id: {e}"),
            PoisonLock(e) => write!(f, "Poison lock: {e}"),
            JobNotUpdated(ds_job_id, us_job_id) => write!(f, "Channel Factory did not update job: Downstream job id = {ds_job_id}, Upstream job id = {us_job_id}"),
            TargetError(e) => write!(f, "Impossible to get Target: {e:?}"),
            HashrateError(e) => write!(f, "Impossible to get Hashrate: {e:?}"),
            LogicErrorMessage(e) => write!(f, "Message is well formatted but can not be handled: {e:?}"),
            JDSMissingTransactions => write!(f, "JD server cannot propagate the block: missing transactions"),
            IoError(e) => write!(f, "IO error: {e:?}"),
            ExtendedExtranonceCreationFailed(e) => write!(f, "Failed to create ExtendedExtranonce: {e}"),
            FromSliceError(e) => write!(f, "Failed to hash from slice: {e}"),
            InvalidUserIdentity(e) => write!(f, "Invalid user identity: {e}"),
            ExtranoncePrefixFactoryError(e) => write!(f, "Failed to create ExtranoncePrefixFactory: {e:?}"),
            Vardiff(e) => write!(f, "Failed to adjust diff in vardiff module: {e:?}"),
            FailedToCreateStandardChannel(e) => write!(f, "Failed to create StandardChannel: {e:?}"),
            FailedToCreateExtendedChannel(e) => write!(f, "Failed to create ExtendedChannel: {e:?}"),
            FailedToProcessNewTemplateGroupChannel(e) => write!(f, "Failed to process NewTemplate: {e:?}"),
            FailedToProcessSetNewPrevHashGroupChannel(e) => write!(f, "Failed to process SetNewPrevHash: {e:?}"),
            NoActiveJob => write!(f, "No active job"),
            FailedToUpdateStandardChannel(e) => write!(f, "Failed to update StandardChannel: {e:?}"),
            FailedToUpdateExtendedChannel(e) => write!(f, "Failed to update ExtendedChannel: {e:?}"),
            FailedToSendSolution => write!(f, "Failed to send solution"),
            FailedToSetCustomMiningJob(e) => write!(f, "Failed to set custom mining job: {e:?}"),
            FailedToProcessNewTemplateExtendedChannel(e) => write!(f, "Failed to process NewTemplate: {e:?}"),
            FailedToProcessNewTemplateStandardChannel(e) => write!(f, "Failed to process NewTemplate: {e:?}"),
            FailedToProcessSetNewPrevHashExtendedChannel(e) => write!(f, "Failed to process SetNewPrevHash: {e:?}"),
            FailedToProcessSetNewPrevHashStandardChannel(e) => write!(f, "Failed to process SetNewPrevHash: {e:?}"),
            FailedToDeserializeCoinbaseOutputs => write!(f, "Failed to deserialize coinbase outputs"),
            FailedToProcessGroupChannelJob(e) => write!(f, "Failed to process group channel job: {e:?}"),
            ParserError(v) => write!(f, "Parser error: {v}"),
        }
    }
}

impl<T> From<PoisonError<MutexGuard<'_, T>>> for Error {
    fn from(value: PoisonError<MutexGuard<'_, T>>) -> Self {
        Error::PoisonLock(value.to_string())
    }
}
