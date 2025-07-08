use stratum_common::roles_logic_sv2::{codec_sv2::StandardEitherFrame, parsers_sv2::AnyMessage};

pub type MessageFrame = StandardEitherFrame<AnyMessage<'static>>;
pub type MsgType = u8;
