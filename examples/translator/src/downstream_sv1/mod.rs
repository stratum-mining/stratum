use v1::utils::{HexBytes, HexU32Be};

pub mod downstream;
pub use downstream::Downstream;

pub fn new_extranonce() -> HexBytes {
    "08000002".try_into().unwrap()
}

pub fn new_extranonce2_size() -> usize {
    8
}

pub fn new_difficulty() -> f64 {
    1.0
    // 0x001
    // "0001".try_into().unwrap()
}

pub fn new_subscription_id() -> HexBytes {
    "ae6812eb4cd7735a302a8a9dd95cf71f".try_into().unwrap()
}

pub fn new_version_rolling_mask() -> HexU32Be {
    HexU32Be(0xffffffff)
}

pub fn new_version_rolling_min() -> HexU32Be {
    HexU32Be(0x00000000)
}
