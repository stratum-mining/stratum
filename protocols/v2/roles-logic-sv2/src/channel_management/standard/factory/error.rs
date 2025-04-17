use crate::{
    channel_management::{
        id::error::ChannelIdFactoryError, standard::channel::error::GroupChannelError,
    },
    extranonce_prefix_management::error::ExtranoncePrefixFactoryError,
};

#[derive(Debug)]
pub enum StandardChannelFactoryError {
    MessageSenderError,
    ResponseReceiverError,
    UnexpectedResponse,
    StandardChannelNotFound,
    GroupChannelNotFound,
    RequestedMaxTargetOutOfRange,
    ChainTipNotSet,
    InvalidCoinbaseRewardOutputs,
    ProcessNewTemplateGroupChannelError(GroupChannelError),
    TemplateNotFound,
    InvalidShare,
    StaleShare,
    NoActiveJob,
    InvalidJobId,
    ShareDoesNotMeetTarget,
    DuplicateShare,
    InvalidNominalHashrate,
    FailedToGenerateNextExtranoncePrefixStandard(ExtranoncePrefixFactoryError),
    ExtranoncePrefixLengthMismatch,
    FailedToGenerateNextChannelId(ChannelIdFactoryError),
}
