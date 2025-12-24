#![no_main]

mod common;
use binary_sv2::{Deserialize, GetSize, Serialize};
use common_messages_sv2::*;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: Vec<u8>| {
    test_roundtrip!(SetupConnection, data);
    test_roundtrip!(SetupConnectionError, data);
    test_roundtrip!(SetupConnectionSuccess, data);
    test_roundtrip!(Reconnect, data);
    test_roundtrip!(ChannelEndpointChanged, data);
});
