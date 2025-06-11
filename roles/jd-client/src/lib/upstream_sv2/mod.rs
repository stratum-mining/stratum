use network_helpers_sv2::roles_logic_sv2::{
    codec_sv2::{StandardEitherFrame, StandardSv2Frame},
    parsers::AnyMessage,
};

pub mod upstream;
pub use upstream::Upstream;

pub type Message = AnyMessage<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;
