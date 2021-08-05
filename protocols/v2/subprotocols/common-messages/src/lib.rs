#![no_std]

//! Common messages for [stratum v2][Sv2]
//! The following protocol messages are common across all of the sv2 (sub)protocols.
extern crate alloc;
mod channel_endpoint_changed;
mod setup_connection;

#[cfg(feature = "prop_test")]
use alloc::vec;
#[cfg(feature = "prop_test")]
use core::convert::TryInto;
#[cfg(feature = "prop_test")]
use quickcheck::{Arbitrary, Gen};

pub use channel_endpoint_changed::ChannelEndpointChanged;
#[cfg(not(feature = "with_serde"))]
pub use setup_connection::{CSetupConnection, CSetupConnectionError};
pub use setup_connection::{
    Protocol, SetupConnection, SetupConnectionError, SetupConnectionSuccess,
};

#[cfg(not(feature = "with_serde"))]
#[no_mangle]
pub extern "C" fn _c_export_channel_endpoint_changed(_a: ChannelEndpointChanged) {}

#[cfg(not(feature = "with_serde"))]
#[no_mangle]
pub extern "C" fn _c_export_setup_conn_succ(_a: SetupConnectionSuccess) {}

#[cfg(feature = "prop_test")]
#[derive(Clone, Debug)]
pub struct CompletelyRandomChannelEndpointChanged(pub ChannelEndpointChanged);

#[cfg(feature = "prop_test")]
impl Arbitrary for CompletelyRandomChannelEndpointChanged {
    fn arbitrary(g: &mut Gen) -> Self {
        CompletelyRandomChannelEndpointChanged(ChannelEndpointChanged {
            channel_id: u32::arbitrary(g).try_into().unwrap(),
        })
    }
}

#[cfg(feature = "prop_test")]
#[derive(Clone, Debug)]
pub struct CompletelyRandomSetupConnectionSuccess(pub SetupConnectionSuccess);

#[cfg(feature = "prop_test")]
impl Arbitrary for CompletelyRandomSetupConnectionSuccess {
    fn arbitrary(g: &mut Gen) -> Self {
        CompletelyRandomSetupConnectionSuccess(SetupConnectionSuccess {
            used_version: u16::arbitrary(g).try_into().unwrap(),
            flags: u32::arbitrary(g).try_into().unwrap(),
        })
    }
}
