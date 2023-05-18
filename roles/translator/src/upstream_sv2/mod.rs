use codec_sv2::{StandardEitherFrame, StandardSv2Frame};
use roles_logic_sv2::parsers::PoolMessages;

pub mod diff_management;
pub mod upstream;
pub mod upstream_connection;
pub use upstream::Upstream;
pub use upstream_connection::UpstreamConnection;

pub type Message = PoolMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;

#[derive(Clone, Copy, Debug)]
pub struct Sv2MiningConnection {
    _version: u16,
    _setup_connection_flags: u32,
    #[allow(dead_code)]
    setup_connection_success_flags: u32,
}
