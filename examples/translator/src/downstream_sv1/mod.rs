use v1::utils::{HexBytes, HexU32Be};

pub(crate) mod downstream;
pub(crate) mod downstream_connection;
pub(crate) use downstream::Downstream;
pub(crate) use downstream_connection::DownstreamConnection;

pub(crate) fn new_extranonce() -> HexBytes {
    "08000002".try_into().unwrap()
}

pub(crate) fn new_extranonce2_size() -> usize {
    4
}

pub(crate) fn new_difficulty() -> HexBytes {
    "b4b6693b72a50c7116db18d6497cac52".try_into().unwrap()
}

pub(crate) fn new_subscription_id() -> HexBytes {
    "ae6812eb4cd7735a302a8a9dd95cf71f".try_into().unwrap()
}

pub(crate) fn new_version_rolling_mask() -> HexU32Be {
    HexU32Be(0xffffffff)
}

pub(crate) fn new_version_rolling_min() -> HexU32Be {
    HexU32Be(0x00000000)
}
