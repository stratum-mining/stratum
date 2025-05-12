use codec_sv2::StandardEitherFrame;
use roles_logic_sv2::parsers::AnyMessage;

pub type MessageFrame = StandardEitherFrame<AnyMessage<'static>>;
pub type MsgType = u8;
