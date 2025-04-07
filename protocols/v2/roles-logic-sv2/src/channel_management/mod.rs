//! # Channel Management
//!
//! A module for managing channels on applications.

pub mod chain_tip;
pub mod extended_channel;
pub mod extended_channel_factory;
pub mod group_channel;
pub mod standard_channel;
pub mod standard_channel_factory;

#[derive(Debug)]
pub enum ShareValidationResult {
    Valid,
    ValidWithAcknowledgement,
    BlockFound,
}

#[derive(Debug)]
pub enum ShareValidationError {
    Invalid,
    Stale,
    InvalidJobId,
    InvalidChannelId,
    DoesNotMeetTarget,
    VersionRollingNotAllowed,
}
