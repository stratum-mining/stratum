use roles_logic_sv2::utils::Mutex;
use std::sync::Arc;
use v1::utils::HexU32Be;
pub mod downstream;
pub use downstream::Downstream;

const SUBSCRIBE_TIMOUT_SECS: u64 = 10;

pub enum TaskIndex {
    SocketReader,
    SocketWriter,
    Notifier,
}

pub async fn kill(sender: &async_channel::Sender<bool>) {
    sender.send(true).await.unwrap();
}

pub fn new_subscription_id() -> String {
    "ae6812eb4cd7735a302a8a9dd95cf71f".into()
}

pub fn new_version_rolling_mask() -> HexU32Be {
    HexU32Be(u32::MAX)
}

pub fn new_version_rolling_min() -> HexU32Be {
    HexU32Be(0)
}
