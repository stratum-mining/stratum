//! # Stratum V2 Common Messages Crate.
//!
//! This crate defines a set of shared messages used across all Stratum V2 subprotocols.
//!
//! ## Build Options
//! This crate can be built with the following features:
//! - `std`: Enables support for standard library features.
//! - `quickcheck`: Enables support for property-based testing using QuickCheck.
//!
//!
//! For further information about the messages, please refer to [Stratum V2
//! documentation - Common Messages](https://stratumprotocol.org/specification/03-Protocol-Overview/#36-common-protocol-messages).

#![no_std]

extern crate alloc;
mod channel_endpoint_changed;
mod reconnect;
mod setup_connection;

#[cfg(feature = "prop_test")]
use alloc::vec;
#[cfg(feature = "prop_test")]
use core::convert::TryInto;
#[cfg(feature = "prop_test")]
use quickcheck::{Arbitrary, Gen};

pub use channel_endpoint_changed::ChannelEndpointChanged;
pub use reconnect::Reconnect;
pub use setup_connection::{
    has_requires_std_job, has_version_rolling, has_work_selection, Protocol, SetupConnection,
    SetupConnectionError, SetupConnectionSuccess,
};

// Discriminants for Stratum V2 (sub)protocols
//
// Discriminants are unique identifiers used to distinguish between different
// Stratum V2 (sub)protocols. Each protocol within the SV2 ecosystem has a
// specific discriminant value that indicates its type, enabling the correct
// interpretation and handling of messages. These discriminants ensure that
// messages are processed by the appropriate protocol handlers,
// thereby facilitating seamless communication across different components of
// the SV2 architecture.
//
// More info can be found [on Chapter 03 of the Stratum V2 specs](https://github.com/stratum-mining/sv2-spec/blob/main/03-Protocol-Overview.md#3-protocol-overview).
pub const SV2_MINING_PROTOCOL_DISCRIMINANT: u8 = 0;
pub const SV2_JOB_DECLARATION_PROTOCOL_DISCRIMINANT: u8 = 1;
pub const SV2_TEMPLATE_DISTRIBUTION_PROTOCOL_DISCRIMINANT: u8 = 2;

// Common message types used across all Stratum V2 (sub)protocols.
pub const MESSAGE_TYPE_SETUP_CONNECTION: u8 = 0x0;
pub const MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS: u8 = 0x1;
pub const MESSAGE_TYPE_SETUP_CONNECTION_ERROR: u8 = 0x2;
pub const MESSAGE_TYPE_CHANNEL_ENDPOINT_CHANGED: u8 = 0x3;
pub const MESSAGE_TYPE_RECONNECT: u8 = 0x04;

pub const CHANNEL_BIT_SETUP_CONNECTION: bool = false;
pub const CHANNEL_BIT_SETUP_CONNECTION_SUCCESS: bool = false;
pub const CHANNEL_BIT_SETUP_CONNECTION_ERROR: bool = false;
pub const CHANNEL_BIT_CHANNEL_ENDPOINT_CHANGED: bool = true;

#[cfg(feature = "prop_test")]
impl ChannelEndpointChanged {
    pub fn from_gen(g: &mut Gen) -> Self {
        ChannelEndpointChanged {
            channel_id: u32::arbitrary(g),
        }
    }
}

#[cfg(feature = "prop_test")]
impl SetupConnection<'static> {
    pub fn from_gen(g: &mut Gen) -> Self {
        let protocol = setup_connection::Protocol::MiningProtocol;

        let mut endpoint_host_gen = Gen::new(255);
        let mut endpoint_host: vec::Vec<u8> = vec::Vec::new();
        endpoint_host.resize_with(255, || u8::arbitrary(&mut endpoint_host_gen));
        let endpoint_host: binary_sv2::Str0255 = endpoint_host.try_into().unwrap();

        let mut vendor_gen = Gen::new(255);
        let mut vendor: vec::Vec<u8> = vec::Vec::new();
        vendor.resize_with(255, || u8::arbitrary(&mut vendor_gen));
        let vendor: binary_sv2::Str0255 = vendor.try_into().unwrap();

        let mut hardware_version_gen = Gen::new(255);
        let mut hardware_version: vec::Vec<u8> = vec::Vec::new();
        hardware_version.resize_with(255, || u8::arbitrary(&mut hardware_version_gen));
        let hardware_version: binary_sv2::Str0255 = hardware_version.try_into().unwrap();

        let mut firmware_gen = Gen::new(255);
        let mut firmware: vec::Vec<u8> = vec::Vec::new();
        firmware.resize_with(255, || u8::arbitrary(&mut firmware_gen));
        let firmware: binary_sv2::Str0255 = firmware.try_into().unwrap();

        let mut device_id_gen = Gen::new(255);
        let mut device_id: vec::Vec<u8> = vec::Vec::new();
        device_id.resize_with(255, || u8::arbitrary(&mut device_id_gen));
        let device_id: binary_sv2::Str0255 = device_id.try_into().unwrap();

        SetupConnection {
            protocol,
            min_version: u16::arbitrary(g),
            max_version: u16::arbitrary(g),
            flags: u32::arbitrary(g),
            endpoint_host,
            endpoint_port: u16::arbitrary(g),
            vendor,
            hardware_version,
            firmware,
            device_id,
        }
    }
}

#[cfg(feature = "prop_test")]
impl SetupConnectionError<'static> {
    pub fn from_gen(g: &mut Gen) -> Self {
        let mut error_code_gen = Gen::new(255);
        let mut error_code: vec::Vec<u8> = vec::Vec::new();
        error_code.resize_with(255, || u8::arbitrary(&mut error_code_gen));
        let error_code: binary_sv2::Str0255 = error_code.try_into().unwrap();

        SetupConnectionError {
            flags: u32::arbitrary(g),
            error_code,
        }
    }
}

#[cfg(feature = "prop_test")]
impl SetupConnectionSuccess {
    pub fn from_gen(g: &mut Gen) -> Self {
        SetupConnectionSuccess {
            used_version: u16::arbitrary(g),
            flags: u32::arbitrary(g),
        }
    }
}
