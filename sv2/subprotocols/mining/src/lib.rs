//! # Stratum V2 Mining Protocol Messages Crate
//!
//! `mining_sv2` is a Rust crate that implements a set of messages defined in the Mining protocol
//! of Stratum V2.
//!
//! The Mining protocol enables the distribution of work to mining devices and the submission of
//! proof-of-work results.
//!
//! For further information about the messages, please refer to [Stratum V2 documentation - Mining](https://stratumprotocol.org/specification/05-Mining-Protocol/).
//!
//! ## Build Options
//!
//! This crate can be built with the following features:
//!
//! ## Usage
//!
//! To include this crate in your project, run:
//! ```bash
//! $ cargo add mining_sv2
//! ```
//!
//! For further information about the mining protocol, please refer to [Stratum V2 documentation -
//! Mining Protocol](https://stratumprotocol.org/specification/05-Mining-Protocol/).

#![no_std]

extern crate alloc;

mod close_channel;
mod new_mining_job;
mod open_channel;
mod set_custom_mining_job;
mod set_extranonce_prefix;
mod set_group_channel;
mod set_new_prev_hash;
mod set_target;
mod submit_shares;
mod update_channel;

pub use close_channel::CloseChannel;
pub use new_mining_job::{NewExtendedMiningJob, NewMiningJob};
pub use open_channel::{
    OpenExtendedMiningChannel, OpenExtendedMiningChannelSuccess, OpenMiningChannelError,
    OpenStandardMiningChannel, OpenStandardMiningChannelSuccess,
};
pub use set_custom_mining_job::{
    SetCustomMiningJob, SetCustomMiningJobError, SetCustomMiningJobSuccess,
};
pub use set_extranonce_prefix::SetExtranoncePrefix;
pub use set_group_channel::SetGroupChannel;
pub use set_new_prev_hash::SetNewPrevHash;
pub use set_target::SetTarget;
pub use submit_shares::{
    SubmitSharesError, SubmitSharesExtended, SubmitSharesStandard, SubmitSharesSuccess,
};
pub use update_channel::{UpdateChannel, UpdateChannelError};

// Mining Protocol message types.
pub const MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL: u8 = 0x10;
pub const MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS: u8 = 0x11;
pub const MESSAGE_TYPE_OPEN_MINING_CHANNEL_ERROR: u8 = 0x12;
pub const MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL: u8 = 0x13;
pub const MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCESS: u8 = 0x14;
pub const MESSAGE_TYPE_NEW_MINING_JOB: u8 = 0x15;
pub const MESSAGE_TYPE_UPDATE_CHANNEL: u8 = 0x16;
pub const MESSAGE_TYPE_UPDATE_CHANNEL_ERROR: u8 = 0x17;
pub const MESSAGE_TYPE_CLOSE_CHANNEL: u8 = 0x18;
pub const MESSAGE_TYPE_SET_EXTRANONCE_PREFIX: u8 = 0x19;
pub const MESSAGE_TYPE_SUBMIT_SHARES_STANDARD: u8 = 0x1a;
pub const MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED: u8 = 0x1b;
pub const MESSAGE_TYPE_SUBMIT_SHARES_SUCCESS: u8 = 0x1c;
pub const MESSAGE_TYPE_SUBMIT_SHARES_ERROR: u8 = 0x1d;
pub const MESSAGE_TYPE_NEW_EXTENDED_MINING_JOB: u8 = 0x1f;
pub const MESSAGE_TYPE_MINING_SET_NEW_PREV_HASH: u8 = 0x20;
pub const MESSAGE_TYPE_SET_TARGET: u8 = 0x21;
pub const MESSAGE_TYPE_SET_CUSTOM_MINING_JOB: u8 = 0x22;
pub const MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_SUCCESS: u8 = 0x23;
pub const MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_ERROR: u8 = 0x24;
pub const MESSAGE_TYPE_SET_GROUP_CHANNEL: u8 = 0x25;

// Channel bits in the Mining protocol vary depending on the message.
pub const CHANNEL_BIT_CLOSE_CHANNEL: bool = true;
pub const CHANNEL_BIT_NEW_EXTENDED_MINING_JOB: bool = true;
pub const CHANNEL_BIT_NEW_MINING_JOB: bool = true;
pub const CHANNEL_BIT_OPEN_EXTENDED_MINING_CHANNEL: bool = false;
pub const CHANNEL_BIT_OPEN_EXTENDED_MINING_CHANNEL_SUCCESS: bool = false;
pub const CHANNEL_BIT_OPEN_MINING_CHANNEL_ERROR: bool = false;
pub const CHANNEL_BIT_OPEN_STANDARD_MINING_CHANNEL: bool = false;
pub const CHANNEL_BIT_OPEN_STANDARD_MINING_CHANNEL_SUCCESS: bool = false;
pub const CHANNEL_BIT_SET_CUSTOM_MINING_JOB: bool = false;
pub const CHANNEL_BIT_SET_CUSTOM_MINING_JOB_ERROR: bool = false;
pub const CHANNEL_BIT_SET_CUSTOM_MINING_JOB_SUCCESS: bool = false;
pub const CHANNEL_BIT_SET_EXTRANONCE_PREFIX: bool = true;
pub const CHANNEL_BIT_SET_GROUP_CHANNEL: bool = false;
pub const CHANNEL_BIT_MINING_SET_NEW_PREV_HASH: bool = true;
pub const CHANNEL_BIT_SET_TARGET: bool = true;
pub const CHANNEL_BIT_SUBMIT_SHARES_ERROR: bool = true;
pub const CHANNEL_BIT_SUBMIT_SHARES_EXTENDED: bool = true;
pub const CHANNEL_BIT_SUBMIT_SHARES_STANDARD: bool = true;
pub const CHANNEL_BIT_SUBMIT_SHARES_SUCCESS: bool = true;

// Commonly used OpenMiningChannelError error_code values.
pub const ERROR_CODE_OPEN_MINING_CHANNEL_STANDARD_CHANNELS_NOT_SUPPORTED_FOR_CUSTOM_WORK: &str =
    "standard-channels-not-supported-for-custom-work";
pub const ERROR_CODE_OPEN_MINING_CHANNEL_INVALID_USER_IDENTITY: &str = "invalid-user-identity";
pub const ERROR_CODE_OPEN_MINING_CHANNEL_INVALID_NOMINAL_HASHRATE: &str =
    "invalid-nominal-hashrate";
pub const ERROR_CODE_OPEN_MINING_CHANNEL_MIN_EXTRANONCE_SIZE_TOO_LARGE: &str =
    "min-extranonce-size-too-large";
pub const ERROR_CODE_OPEN_MINING_CHANNEL_MAX_TARGET_OUT_OF_RANGE: &str = "max-target-out-of-range";
pub const ERROR_CODE_OPEN_MINING_CHANNEL_UNSUPPORTED_MIN_EXTRANONCE_SIZE: &str =
    "unsupported-min-extranonce-size";
pub const ERROR_CODE_OPEN_MINING_CHANNEL_UNKNOWN_USER: &str = "unknown-user";

// Commonly used UpdateChannelError error_code values.
pub const ERROR_CODE_UPDATE_CHANNEL_INVALID_NOMINAL_HASHRATE: &str = "invalid-nominal-hashrate";
pub const ERROR_CODE_UPDATE_CHANNEL_INVALID_CHANNEL_ID: &str = "invalid-channel-id";

// Commonly used SubmitSharesError error_code values.
pub const ERROR_CODE_SUBMIT_SHARES_INVALID_CHANNEL_ID: &str = "invalid-channel-id";
pub const ERROR_CODE_SUBMIT_SHARES_INVALID_SHARE: &str = "invalid-share";
pub const ERROR_CODE_SUBMIT_SHARES_STALE_SHARE: &str = "stale-share";
pub const ERROR_CODE_SUBMIT_SHARES_INVALID_JOB_ID: &str = "invalid-job-id";
pub const ERROR_CODE_SUBMIT_SHARES_DIFFICULTY_TOO_LOW: &str = "difficulty-too-low";
pub const ERROR_CODE_SUBMIT_SHARES_DUPLICATE_SHARE: &str = "duplicate-share";
pub const ERROR_CODE_SUBMIT_SHARES_BAD_EXTRANONCE_SIZE: &str = "bad-extranonce-size";
pub const ERROR_CODE_VERSION_ROLLING_NOT_ALLOWED: &str = "version-rolling-not-allowed";

// Commonly used SetCustomMiningJobError error_code values.
pub const ERROR_CODE_SET_CUSTOM_MINING_JOB_JD_NOT_SUPPORTED: &str = "jd-not-supported";
pub const ERROR_CODE_SET_CUSTOM_MINING_JOB_INVALID_CHANNEL_ID: &str = "invalid-channel-id";
pub const ERROR_CODE_SET_CUSTOM_MINING_JOB_INVALID_MINING_JOB_TOKEN: &str =
    "invalid-mining-job-token";
pub const ERROR_CODE_SET_CUSTOM_MINING_JOB_JOB_NOT_YET_VALIDATED: &str = "job-not-yet-validated";
pub const ERROR_CODE_SET_CUSTOM_MINING_JOB_STALE_CHAIN_TIP: &str = "stale-chain-tip";
pub const ERROR_CODE_SET_CUSTOM_MINING_JOB_INVALID_MIN_NTIME: &str = "invalid-min-ntime";
pub const ERROR_CODE_SET_CUSTOM_MINING_JOB_INVALID_NBITS: &str = "invalid-nbits";
pub const ERROR_CODE_SET_CUSTOM_MINING_JOB_INVALID_VERSION: &str = "invalid-version";
pub const ERROR_CODE_SET_CUSTOM_MINING_JOB_INVALID_COINBASE_TX: &str = "invalid-coinbase-tx";
pub const ERROR_CODE_SET_CUSTOM_MINING_JOB_INVALID_COINBASE_TX_VERSION: &str =
    "invalid-coinbase-tx-version";
pub const ERROR_CODE_SET_CUSTOM_MINING_JOB_INVALID_COINBASE_PREFIX: &str =
    "invalid-coinbase-prefix";
pub const ERROR_CODE_SET_CUSTOM_MINING_JOB_INVALID_COINBASE_TX_INPUT_N_SEQUENCE: &str =
    "invalid-coinbase-tx-input-n-sequence";
pub const ERROR_CODE_SET_CUSTOM_MINING_JOB_INVALID_COINBASE_TX_OUTPUTS: &str =
    "invalid-coinbase-tx-outputs";
pub const ERROR_CODE_SET_CUSTOM_MINING_JOB_INVALID_COINBASE_TX_LOCKTIME: &str =
    "invalid-coinbase-tx-locktime";
pub const ERROR_CODE_SET_CUSTOM_MINING_JOB_INVALID_MERKLE_PATH: &str = "invalid-merkle-path";
pub const CHANNEL_BIT_UPDATE_CHANNEL: bool = true;
pub const CHANNEL_BIT_UPDATE_CHANNEL_ERROR: bool = true;
