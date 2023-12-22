use roles_logic_sv2::mining_sv2::Target;
use v1::{client_to_server::Submit, utils::HexU32Be};
pub mod diff_management;
pub mod downstream;
pub use downstream::Downstream;

/// This constant is used as a check to ensure clients
/// do not send a mining.subscribe and never a mining.authorize
/// since they will take up a tcp connection but never be allowed to
/// receive jobs. Without the timeout the TProxy can be exploited by incoming
/// `mining.subscribe` messages that init connections and take up compute
const SUBSCRIBE_TIMEOUT_SECS: u64 = 10;

/// enum of messages sent to the Bridge
#[derive(Debug)]
pub enum DownstreamMessages {
    SubmitShares(SubmitShareWithChannelId),
    SetDownstreamTarget(SetDownstreamTarget),
}

/// wrapper around a `mining.submit` with extra channel informationfor the Bridge to
/// process
#[derive(Debug)]
pub struct SubmitShareWithChannelId {
    pub channel_id: u32,
    pub share: Submit<'static>,
    pub extranonce: Vec<u8>,
    pub extranonce2_len: usize,
    pub version_rolling_mask: Option<HexU32Be>,
}

/// message for notifying the bridge that a downstream target has updated
/// so the Bridge can process the update
#[derive(Debug)]
pub struct SetDownstreamTarget {
    pub channel_id: u32,
    pub new_target: Target,
}

/// This is just a wrapper function to send a message on the Downstream task shutdown channel
/// it does not matter what message is sent because the receiving ends should shutdown on any message
pub async fn kill(sender: &async_channel::Sender<bool>) {
    // safe to unwrap since the only way this can fail is if all receiving channels are dropped
    // meaning all tasks have already dropped
    sender.send(true).await.unwrap();
}

pub fn new_subscription_id() -> String {
    "ae6812eb4cd7735a302a8a9dd95cf71f".into()
}
