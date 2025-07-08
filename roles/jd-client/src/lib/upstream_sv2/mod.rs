use stratum_common::roles_logic_sv2::{
    codec_sv2::{StandardEitherFrame, StandardSv2Frame},
    parsers_sv2::AnyMessage,
};

pub mod upstream;
pub use upstream::Upstream;

pub type Message = AnyMessage<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;
