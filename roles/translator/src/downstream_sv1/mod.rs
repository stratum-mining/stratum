<<<<<<< HEAD
use v1::utils::{HexBytes, HexU32Be};
=======
use v1::utils::HexU32Be;
>>>>>>> e45f4adc1ef1104031c71ef3519f1cfaaff130c3

pub mod downstream;
pub use downstream::Downstream;

<<<<<<< HEAD
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

=======
>>>>>>> e45f4adc1ef1104031c71ef3519f1cfaaff130c3
pub fn new_subscription_id() -> String {
    "ae6812eb4cd7735a302a8a9dd95cf71f".into()
}

pub fn new_version_rolling_mask() -> HexU32Be {
<<<<<<< HEAD
    HexU32Be(0xffffffff)
}

pub fn new_version_rolling_min() -> HexU32Be {
    HexU32Be(0x00000000)
=======
    HexU32Be(u32::MAX)
}

pub fn new_version_rolling_min() -> HexU32Be {
    HexU32Be(0)
>>>>>>> e45f4adc1ef1104031c71ef3519f1cfaaff130c3
}
