use v1::utils::HexU32Be;

pub mod downstream;
pub use downstream::Downstream;

pub fn new_difficulty() -> f64 {
    1.0
    // 0x001
    // "0001".try_into().unwrap()
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
