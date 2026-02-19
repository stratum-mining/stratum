#![no_main]

mod common;
use arbitrary::Arbitrary;
use binary_sv2::{Deserialize, GetSize, Serialize};
use common_messages_sv2::*;
use libfuzzer_sys::fuzz_target;

#[derive(Arbitrary, Debug)]
enum FuzzInput {
    SetupConnection(Vec<u8>),
    SetupConnectionError(Vec<u8>),
    SetupConnectionSuccess(Vec<u8>),
    Reconnect(Vec<u8>),
    ChannelEndpointChanged(Vec<u8>),
}

fuzz_target!(|input: FuzzInput| {
    match input {
        FuzzInput::SetupConnection(data) => {
            test_roundtrip!(SetupConnection, data)
        }
        FuzzInput::SetupConnectionError(data) => {
            test_roundtrip!(SetupConnectionError, data)
        }
        FuzzInput::SetupConnectionSuccess(data) => {
            test_roundtrip!(SetupConnectionSuccess, data)
        }
        FuzzInput::Reconnect(data) => {
            test_roundtrip!(Reconnect, data)
        }
        FuzzInput::ChannelEndpointChanged(data) => {
            test_roundtrip!(ChannelEndpointChanged, data)
        }
    }
});
