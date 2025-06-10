use network_helpers_sv2::roles_logic_sv2::{codec_sv2::StandardEitherFrame, parsers::AnyMessage};

pub type MessageFrame = StandardEitherFrame<AnyMessage<'static>>;
pub type MsgType = u8;
