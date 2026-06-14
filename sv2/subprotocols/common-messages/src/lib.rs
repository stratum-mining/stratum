//! # Stratum V2 Common Messages Crate.
//!
//! This crate defines a set of shared messages used across all Stratum V2 subprotocols.
//!
//! ## Build Options
//! This crate can be built with the following features:
//! - `std`: Enables support for standard library features.
//!
//! For further information about the messages, please refer to [Stratum V2
//! documentation - Common Messages](https://stratumprotocol.org/specification/03-Protocol-Overview/#36-common-protocol-messages).

#![no_std]

extern crate alloc;
mod channel_endpoint_changed;
mod reconnect;
mod setup_connection;

pub use channel_endpoint_changed::ChannelEndpointChanged;
pub use reconnect::Reconnect;
pub use setup_connection::{
    has_declare_tx_data, has_requires_std_job, has_version_rolling, has_work_selection, Protocol,
    SetupConnection, SetupConnectionError, SetupConnectionSuccess,
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
pub const CHANNEL_BIT_RECONNECT: bool = false;

// Commonly used SetupConnectionError error_code values.
pub const ERROR_CODE_SETUP_CONNECTION_UNSUPPORTED_FEATURE_FLAGS: &str = "unsupported-feature-flags";
pub const ERROR_CODE_SETUP_CONNECTION_UNSUPPORTED_PROTOCOL: &str = "unsupported-protocol";
pub const ERROR_CODE_SETUP_CONNECTION_MISSING_DECLARE_TX_DATA_FLAG: &str =
    "missing-declare-tx-data-flag";
