use crate::{
    channel_management::{
        extended::channel::{error::ExtendedChannelError, ExtendedChannel},
        id::error::ChannelIdFactoryError,
        share_accounting::ShareAccounting,
    },
    extranonce_prefix_management::error::ExtranoncePrefixFactoryError,
    job_management::chain_tip::ChainTip,
};
use binary_sv2::U256;
use std::collections::HashMap;
/// Response variants for the inner factory (Actor Model implementation of the
/// `ExtendedChannelFactory`).
#[derive(Debug)]
pub enum InnerExtendedChannelFactoryResponse<'a> {
    // NewChannelCreated(channel_id, target, rollable_extranonce_size, extranonce_prefix)
    NewChannelCreated(u32, U256<'a>, u16, Vec<u8>),
    Channel(ExtendedChannel<'a>),
    ChannelCount(u32),
    AllChannels(HashMap<u32, ExtendedChannel<'a>>),
    // ChannelUpdated(new_max_target)
    ChannelUpdated(U256<'a>),
    RequestedMaxTargetOutOfRange,
    ChannelNotFound,
    InvalidNominalHashrate,
    TemplateNotFound,
    FailedToGenerateNextExtranoncePrefixExtended(ExtranoncePrefixFactoryError),
    Shutdown,
    ChannelRemoved,
    ChainTipNotSet,
    ProcessedNewTemplate,
    InvalidCoinbaseRewardOutputs,
    ProcessNewTemplateChannelError(ExtendedChannelError),
    ProcessedSetNewPrevHash,
    ProcessedSetCustomMiningJob(u32),
    CustomMiningJobBadPrevHash,
    CustomMiningJobBadNbits,
    CustomMiningJobBadNtime,
    CustomMiningJobBadChannelId,
    ProcessSetCustomMiningJobChannelError(ExtendedChannelError),
    DuplicateShare,
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
    ChainTip(ChainTip),
    InvalidCoinbase,
    ShareAccounting(ShareAccounting),
    SetExtranoncePrefix,
    ExtranoncePrefixLengthMismatch,
    FailedToGenerateNextChannelId(ChannelIdFactoryError),
}
