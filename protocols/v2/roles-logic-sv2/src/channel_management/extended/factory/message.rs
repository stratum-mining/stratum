use binary_sv2::U256;
use mining_sv2::{SetCustomMiningJob, SubmitSharesExtended};
use stratum_common::bitcoin::transaction::TxOut;
use template_distribution_sv2::{NewTemplate, SetNewPrevHash};

/// Message type for the inner factory (Actor Model implementation of the `ExtendedChannelFactory`).
pub enum InnerExtendedChannelFactoryMessage<'a> {
    // NewChannel(user_identity, nominal_hashrate, min_extranonce_size, max_target)
    NewChannel(String, f32, usize, U256<'a>),
    // GetChannel(channel_id)
    GetChannel(u32),
    GetChannelCount,
    GetAllChannels,
    // RemoveChannel(channel_id)
    RemoveChannel(u32),
    // UpdateChannel(channel_id, nominal_hashrate, max_target)
    UpdateChannel(u32, f32, U256<'a>),
    ProcessNewTemplate(NewTemplate<'a>, Vec<TxOut>),
    ProcessSetNewPrevHash(SetNewPrevHash<'a>),
    ProcessSetCustomMiningJob(SetCustomMiningJob<'static>),
    ValidateShare(SubmitSharesExtended<'a>),
    GetShareAccounting(u32),
    SetExtranoncePrefix(u32, Vec<u8>),
    GetChainTip,
    Shutdown,
}
