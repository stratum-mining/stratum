use crate::{
    channel_management::{
        extended::channel::error::ExtendedChannelError, id::error::ChannelIdFactoryError,
    },
    extranonce_prefix_management::error::ExtranoncePrefixFactoryError,
};
use mining_sv2::ExtendedExtranonceError;

#[derive(Debug)]
pub enum ExtendedChannelFactoryError {
    ExtendedExtranonceError(ExtendedExtranonceError),
    ExtranoncePrefixFactoryError(ExtranoncePrefixFactoryError),
    MessageSenderError,
    ResponseReceiverError,
    InvalidNominalHashrate,
    RequestedMaxTargetOutOfRange,
    ResponseSenderError,
    UnexpectedResponse,
    ChannelNotFound,
    FailedToGenerateNextExtranoncePrefixExtended(ExtranoncePrefixFactoryError),
    FailedToGenerateNextExtranoncePrefixStandard(ExtranoncePrefixFactoryError),
    ChainTipNotSet,
    InvalidCoinbaseRewardOutputs,
    ProcessNewTemplateChannelError(ExtendedChannelError),
    ProcessSetCustomMiningJobChannelError(ExtendedChannelError),
    CustomMiningJobBadChannelId,
    CustomMiningJobBadPrevHash,
    CustomMiningJobBadNbits,
    CustomMiningJobBadNtime,
    CustomMiningJobBadCoinbaseRewardOutputs,
    TemplateNotFound,
    NoActiveJob,
    InvalidShare,
    DuplicateShare,
    StaleShare,
    InvalidJobId,
    ShareDoesNotMeetTarget,
    VersionRollingNotAllowed,
    ExtranoncePrefixLengthMismatch,
    FailedToGenerateNextChannelId(ChannelIdFactoryError),
}
