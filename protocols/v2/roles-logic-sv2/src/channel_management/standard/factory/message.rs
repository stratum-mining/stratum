use binary_sv2::U256;
use mining_sv2::SubmitSharesStandard;
use stratum_common::bitcoin::transaction::TxOut;
use template_distribution_sv2::{NewTemplate, SetNewPrevHash};

pub enum InnerStandardChannelFactoryMessage<'a> {
    NewStandardChannel(String, f32, U256<'a>, u32),
    UpdateStandardChannel(u32, f32, U256<'a>),
    RemoveStandardChannel(u32),
    GetStandardChannel(u32),
    NewGroupChannel,
    RemoveGroupChannel(u32),
    GetGroupChannel(u32),
    GetStandardChannelCount,
    GetGroupChannelCount,
    GetAllStandardChannels,
    GetAllGroupChannels,
    ProcessNewTemplate(NewTemplate<'a>, Vec<TxOut>),
    ProcessSetNewPrevHash(SetNewPrevHash<'a>),
    ValidateShare(SubmitSharesStandard),
    GetShareAccounting(u32),
    SetExtranoncePrefix(u32, Vec<u8>),
    GetChainTip,
    Shutdown,
}
