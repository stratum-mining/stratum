use codec_sv2::{StandardEitherFrame, StandardSv2Frame};
use roles_logic_sv2::parsers::PoolMessages;

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

/// TODO: Put this value in the config file instead
/// Set the minimum `extranonce_size` to be requested from the Upstream. Request is sent in the
/// SV2 `OpenExtendedMiningChannel` message. The SV2 Upstream sends the final `extranonce_size` to
/// the `Bridge` in the SV2 `OpenExtendedMiningChannelSuccess` message response. This
/// `extranonce_size` is the `extranonce2_size` that is sent to the Downstream in the SV1
/// `mining.subscribe` message response.
pub fn new_extranonce2_size() -> u16 {
    8
}
