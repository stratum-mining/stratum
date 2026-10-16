#![no_main]

mod common;
mod generators;
mod spec_assertions;
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
            test_roundtrip!(SetupConnection, data, generators::gen_setup_connection,
                spec_assertions::assert_setup_connection);
        }
        FuzzInput::SetupConnectionError(data) => {
            test_roundtrip!(SetupConnectionError, data, generators::gen_setup_connection_error,
                spec_assertions::assert_setup_connection_error);
        }
        FuzzInput::SetupConnectionSuccess(data) => {
            test_roundtrip!(SetupConnectionSuccess, data, generators::gen_setup_connection_success);
        }
        FuzzInput::Reconnect(data) => {
            test_roundtrip!(Reconnect, data, generators::gen_reconnect);
        }
        FuzzInput::ChannelEndpointChanged(data) => {
            test_roundtrip!(ChannelEndpointChanged, data, generators::gen_channel_endpoint_changed);
        }
    }
});
