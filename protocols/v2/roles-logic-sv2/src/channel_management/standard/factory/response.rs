use crate::{
    channel_management::{
        id::error::ChannelIdFactoryError,
        share_accounting::ShareAccounting,
        standard::channel::{
            error::GroupChannelError, group::GroupChannel, standard::StandardChannel,
        },
    },
    extranonce_prefix_management::error::ExtranoncePrefixFactoryError,
    job_management::chain_tip::ChainTip,
};
use binary_sv2::U256;
use std::collections::HashMap;

#[derive(Debug)]
pub enum InnerStandardChannelFactoryResponse<'a> {
    InvalidNominalHashrate,
    NewStandardChannelCreated(u32, U256<'a>, Vec<u8>),
    NewGroupChannelCreated(u32),
    RequestedMaxTargetOutOfRange,
    FailedToGenerateNextExtranoncePrefixStandard(ExtranoncePrefixFactoryError),
    GroupChannelIdNotFound,
    StandardChannelIdNotFound,
    StandardChannel(StandardChannel<'a>),
    GroupChannel(GroupChannel<'a>),
    StandardChannelUpdated(U256<'a>),
    StandardChannelRemoved,
    GroupChannelRemoved,
    TemplateNotFound,
    ChainTip(ChainTip),
    StandardChannelCount(u32),
    GroupChannelCount(u32),
    AllStandardChannels(HashMap<u32, StandardChannel<'a>>),
    AllGroupChannels(HashMap<u32, GroupChannel<'a>>),
    ChainTipNotSet,
    InvalidCoinbaseRewardOutputs,
    ProcessNewTemplateGroupChannelError(GroupChannelError),
    ProcessedNewTemplate,
    ProcessedSetNewPrevHash,
    Shutdown,
    ValidShare,
    // last_sequence_number, new_submits_accepted_count, new_shares_sum
    ValidShareWithAcknowledgement(u32, u32, u64),
    // template_id, coinbase
    // template_id is None if custom job
    BlockFound(Option<u64>, Vec<u8>),
    InvalidShare,
    StaleShare,
    InvalidJobId,
    ShareDoesNotMeetTarget,
    VersionRollingNotAllowed,
    DuplicateShare,
    InvalidCoinbase,
    ShareAccounting(ShareAccounting),
    SetExtranoncePrefix,
    ExtranoncePrefixLengthMismatch,
    FailedToGenerateNextChannelId(ChannelIdFactoryError),
}
