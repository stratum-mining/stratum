#![no_std]

//! Common messages for [stratum v2][Sv2]
//! The following protocol messages are common across all of the sv2 (sub)protocols.
mod channel_endpoint_changed;
mod setup_connection;

pub use channel_endpoint_changed::ChannelEndpointChanged;
pub use setup_connection::SetupConnection;
