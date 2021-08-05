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
pub struct CompletelyRandomSetupConnection(pub SetupConnection<'static>);

#[cfg(feature = "prop_test")]
impl Arbitrary for CompletelyRandomSetupConnection {
    fn arbitrary(g: &mut Gen) -> Self {
        let protocol = setup_connection::Protocol::MiningProtocol;
        // let protocol = setup_connection::Protocol::JobDistributionProtocol;
        // let protocol = setup_connection::Protocol::TemplateDistributionProtocol;
        // let protocol = setup_connection::Protocol::JobNegotiationProtocol;
        let mut endpoint_host = Gen::new(255);
        let endpoint_host: binary_sv2::Str0255 = vec::Vec::<u8>::arbitrary(&mut endpoint_host)
            .try_into()
            .unwrap();
        let mut vendor = Gen::new(255);
        let vendor: binary_sv2::Str0255 =
            vec::Vec::<u8>::arbitrary(&mut vendor).try_into().unwrap();
        let mut hardware_version = Gen::new(255);
        let hardware_version: binary_sv2::Str0255 =
            vec::Vec::<u8>::arbitrary(&mut hardware_version)
                .try_into()
                .unwrap();
        let mut firmware = Gen::new(255);
        let firmware: binary_sv2::Str0255 =
            vec::Vec::<u8>::arbitrary(&mut firmware).try_into().unwrap();
        let mut device_id = Gen::new(255);
        let device_id: binary_sv2::Str0255 = vec::Vec::<u8>::arbitrary(&mut device_id)
            .try_into()
            .unwrap();
        CompletelyRandomSetupConnection(SetupConnection {
            protocol,
            min_version: u16::arbitrary(g).try_into().unwrap(),
            max_version: u16::arbitrary(g).try_into().unwrap(),
            flags: u32::arbitrary(g).try_into().unwrap(),
            endpoint_host,
            endpoint_port: u16::arbitrary(g).try_into().unwrap(),
            vendor,
            hardware_version,
            firmware,
            device_id,
        })
    }
}

#[cfg(feature = "prop_test")]
#[derive(Clone, Debug)]
pub struct CompletelyRandomSetupConnectionError(pub SetupConnectionError<'static>);

#[cfg(feature = "prop_test")]
impl Arbitrary for CompletelyRandomSetupConnectionError {
    fn arbitrary(g: &mut Gen) -> Self {
        let mut error_code_generator = Gen::new(255);
        let error_code: binary_sv2::Str0255 = vec::Vec::<u8>::arbitrary(&mut error_code_generator)
            .try_into()
            .unwrap();
        CompletelyRandomSetupConnectionError(SetupConnectionError {
            flags: u32::arbitrary(g).try_into().unwrap(),
            error_code,
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
