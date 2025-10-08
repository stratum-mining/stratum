use stratum_apps::stratum_core::{codec_sv2::StandardEitherFrame, parsers_sv2::AnyMessage};

pub type MessageFrame = StandardEitherFrame<AnyMessage<'static>>;
pub type MsgType = u8;
