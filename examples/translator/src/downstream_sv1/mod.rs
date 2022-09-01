use v1::utils::{HexBytes, HexU32Be};

pub mod downstream;
pub use downstream::Downstream;

pub fn new_extranonce() -> HexBytes {
    "08000002".try_into().unwrap()
}

pub fn new_extranonce2_size() -> usize {
    4
}

pub fn new_difficulty() -> HexBytes {
    "b4b6693b72a50c7116db18d6497cac52".try_into().unwrap()
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
