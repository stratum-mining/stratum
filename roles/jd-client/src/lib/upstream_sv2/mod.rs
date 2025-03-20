use roles_logic_sv2::{parsers::AnyMessage, StandardEitherFrame, StandardSv2Frame};

pub mod upstream;
pub use upstream::Upstream;

pub type Message = AnyMessage<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;
