use v1::utils::HexU32Be;
pub mod diff_management;
pub mod downstream;
pub use downstream::Downstream;

/// This constant is used as a check to ensure clients
/// do not send a mining.subscribe and never a mining.authorize
/// since they will take up a tcp connection but never be allowed to
/// receive jobs. Without the timeout the TProxy can be exploited by incoming
/// `mining.subscribe` messages that init connections and take up compute
const SUBSCRIBE_TIMEOUT_SECS: u64 = 10;

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

pub fn new_version_rolling_mask() -> HexU32Be {
    HexU32Be(0b00011111111111111110000000000000)
}

pub fn new_version_rolling_min() -> HexU32Be {
    HexU32Be(0)
}
