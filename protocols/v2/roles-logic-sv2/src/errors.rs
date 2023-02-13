use binary_sv2::Error as BinarySv2Error;
use std::fmt::{self, Display, Formatter};

#[derive(Debug)]
/// No NoPairableUpstream((min_v, max_v, all falgs supported))
pub enum Error {
    /// Errors if payload size is too big to fit into a frame.
    BadPayloadSize,
    ExpectedLen32(usize),
    BinarySv2Error(BinarySv2Error),
    /// Errors if a `SendTo::RelaySameMessageSv1` request is made on a SV2-only application.
    CannotRelaySv1Message,
    NoGroupsFound,
    UnexpectedMessage(u8),
    NoGroupIdOnExtendedChannel,
    /// (`min_v`, `max_v`, all flags supported)
    NoPairableUpstream((u16, u16, u32)),
    /// Error if the hashmap `future_jobs` field in the `GroupChannelJobDispatcher` is empty.
    NoFutureJobs,
    NoDownstreamsConnected,
    PrevHashRequireNonExistentJobId(u32),
    RequestIdNotMapped(u32),
    NoUpstreamsConnected,
    UnimplementedProtocol,
    UnexpectedPoolMessage,
    UnknownRequestId(u32),
    NoMoreExtranonces,
    JobIsNotFutureButPrevHashNotPresent,
    ChannelIsNeitherExtendedNeitherInAPool,
    ExtranonceSpaceEnded,
    ImpossibleToCalculateMerkleRoot,
    GroupIdNotFound,
    ShareDoNotMatchAnyJob,
    ShareDoNotMatchAnyChannel,
    InvalidCoinbase,
    VersionTooBig,
    TxVersionTooBig,
    TxVersionTooLow,
    NotFoundChannelId,
    NoValidJob,
    NoTemplateForId,
    InvalidExtranonceSize(u16, u16),
    PoisonLock(String),
    InvalidBip34Bytes(Vec<u8>),
}

impl From<BinarySv2Error> for Error {
    fn from(v: BinarySv2Error) -> Error {
        Error::BinarySv2Error(v)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        use Error::*;
        match self {
            BadPayloadSize => write!(f, "Payload is too big to fit into the frame"),
            BinarySv2Error(v) => write!(
                f,
                "BinarySv2Error: error in serializing/deserilizing binary format {:?}",
                v
            ),
            CannotRelaySv1Message => {
                write!(
                    f,
                    "Cannot process request: Received SV1 relay request on a SV2-only application"
                )
            }
            ExpectedLen32(l) => write!(f, "Expected length of 32, but received length of {}", l),
            NoGroupsFound => write!(
                f,
                "A channel was attempted to be added to an Upstream, but no groups are specified"
            ),
            UnexpectedMessage(type_) => write!(f, "Error: Unexpected message received. Recv m type: {:x}", type_),
            NoGroupIdOnExtendedChannel => write!(f, "Extended channels do not have group IDs"),
            NoPairableUpstream(a) => {
                write!(f, "No pairable upstream node: {:?}", a)
            }
            NoFutureJobs => write!(f, "GroupChannelJobDispatcher does not have any future jobs"),
            NoDownstreamsConnected => write!(f, "NoDownstreamsConnected"),
            PrevHashRequireNonExistentJobId(id) => {
                write!(f, "PrevHashRequireNonExistentJobId {}", id)
            }
            RequestIdNotMapped(id) => write!(f, "RequestIdNotMapped {}", id),
            NoUpstreamsConnected => write!(f, "There are no upstream connected"),
            UnexpectedPoolMessage => write!(f, "Unexpected `PoolMessage` type"),
            UnimplementedProtocol => write!(
                f,
                "TODO: `Protocol` has not been implemented, but should be"
            ),
            UnknownRequestId(id) => write!(
                f,
                "Upstream is answering with a wrong request ID {} or
                DownstreamMiningSelector::on_open_standard_channel_request has not been called
                before relaying open channel request to upstream",
                id
            ),
            InvalidExtranonceSize(required_min, requested) => {
                write!(
                    f,
                    "Invalid extranonce size: required min {}, requested {}",
                    required_min, requested
                )
            },
            NoMoreExtranonces => write!(f, "No more extranonces"),
            JobIsNotFutureButPrevHashNotPresent => write!(f, "A non future job always expect a previous new prev hash"),
            ChannelIsNeitherExtendedNeitherInAPool => write!(f, "If a channel is neither extended neither is part of a pool the only thing to do when a OpenStandardChannle is received is to relay it upstream with and updated request id"),
            ExtranonceSpaceEnded => write!(f, "No more avaible extranonces for downstream"),
            ImpossibleToCalculateMerkleRoot => write!(f, "Impossible to calculate merkle root"),
            GroupIdNotFound => write!(f, "Group id not found"),
            ShareDoNotMatchAnyJob => write!(f, "A share has been recived but no job for it exist"),
            ShareDoNotMatchAnyChannel => write!(f, "A share has been recived but no channel for it exist"),
            InvalidCoinbase => write!(f, "Coinbase prefix + extranonce + coinbase suffix is not a valid coinbase"),
            VersionTooBig => write!(f, "We are trying to construct a block header with version bigger than i32::MAX"),
            TxVersionTooBig => write!(f, "Tx version can not be greater than i32::MAX"),
            TxVersionTooLow => write!(f, "Tx version can not be lower than 1"),
            NotFoundChannelId => write!(f, "No downstream has been registred for this channel id"),
            NoValidJob => write!(f, "Impossible to create a standard job for channelA cause no valid job has been received from upstream yet"),
            NoTemplateForId => write!(f, "Impossible a template for the required job id"),
            PoisonLock(e) => write!(f, "Poison lock: {}", e),
            InvalidBip34Bytes(e) => write!(f, "Invalid Bip34 bytes {:?}", e),
        }
    }
}
