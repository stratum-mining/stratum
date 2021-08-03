//! Central repository for all the sv2 constants
#![no_std]

pub const EXTENSION_TYPE_NO_EXTENSION: u16 = 0;

pub const SV2_FRAME_HEADER_SIZE: usize = 6;
pub const SV2_FRAME_HEADER_LEN_OFFSET: usize = 3;
pub const SV2_FRAME_HEADER_LEN_END: usize = 3;

pub const NOISE_FRAME_HEADER_SIZE: usize = 2;
pub const NOISE_FRAME_HEADER_LEN_OFFSET: usize = 0;
pub const NOISE_FRAME_HEADER_LEN_END: usize = 2;
pub const NOISE_FRAME_MAX_SIZE: usize = u16::MAX as usize;

pub const NOISE_PARAMS: &str = "Noise_NX_25519_ChaChaPoly_BLAKE2s";
pub const SNOW_PSKLEN: usize = 32;
pub const SNOW_TAGLEN: usize = 16;

pub const SV2_MINING_PROTOCOL_DISCRIMINANT: u8 = 0;
pub const SV2_JOB_NEG_PROTOCOL_DISCRIMINANT: u8 = 1;
pub const SV2_TEMPLATE_DISTR_PROTOCOL_DISCRIMINANT: u8 = 2;
pub const SV2_JOB_DISTR_PROTOCOL_DISCRIMINANT: u8 = 3;

// COMMON MESSAGES TYPES
pub const MESSAGE_TYPE_SETUP_CONNECTION: u8 = 0x0;
pub const MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS: u8 = 0x1;
pub const MESSAGE_TYPE_SETUP_CONNECTION_ERROR: u8 = 0x2;
pub const MESSAGE_TYPE_CHANNEL_ENDPOINT_CHANGED: u8 = 0x3;
// TEMPLATE DISTRIBUTION PROTOCOL MESSAGES TYPES
pub const MESSAGE_TYPE_COINBASE_OUTPUT_DATA_SIZE: u8 = 0x70;
pub const MESSAGE_TYPE_NEW_TEMPLATE: u8 = 0x71;
pub const MESSAGE_TYPE_SET_NEW_PREV_HASH: u8 = 0x72;
pub const MESSAGE_TYPE_REQUEST_TRANSACTION_DATA: u8 = 0x73;
pub const MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_SUCCESS: u8 = 0x74;
pub const MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_ERROR: u8 = 0x75;
pub const MESSAGE_TYPE_SUBMIT_SOLUTION: u8 = 0x76;
// JOB NEGOTIATION PROTOCOL MESSAGES TYPES
pub const MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN: u8 = 0x50;
pub const MESSAGE_TYPE_ALLOCATE_MINING_JOB_SUCCESS: u8 = 0x51;
// pub const MESSAGE_TYPE_ALLOCATE_MINING_JOB_ERROR: u8 = 0x52; // TODO is on the message type
// table but is not defined as message
pub const MESSAGE_TYPE_IDENTIFY_TRANSACTIONS: u8 = 0x53;
pub const MESSAGE_TYPE_IDENTIFY_TRANSACTIONS_SUCCESS: u8 = 0x54;
pub const MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTION: u8 = 0x55;
pub const MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTION_SUCCESS: u8 = 0x56;
// TODO not in messages type table !!!
pub const MESSAGE_TYPE_COMMIT_MINING_JOB: u8 = 0x57;
pub const MESSAGE_TYPE_COMMIT_MINING_JOB_SUCCESS: u8 = 0x58;
pub const MESSAGE_TYPE_COMMIT_MINING_JOB_ERROR: u8 = 0x59;
// MINING PROTOCOL MESSAGES TYPES
pub const MESSAGE_TYPE_CLOSE_CHANNEL: u8 = 0x18;
pub const MESSAGE_TYPE_NEW_EXTENDED_MINING_JOB: u8 = 0x1f;
pub const MESSAGE_TYPE_NEW_MINING_JOB: u8 = 0x1e;
pub const MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL: u8 = 0x13;
pub const MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCES: u8 = 0x14;
// TODO in the spec page 21 is defined OpenMiningChannelError valid for both extended and standard
// messages but in the spec page 40 are defined two different message types for
// OpenStandardMiningChannelError and OpenExtendedMiningChannelError
pub const MESSAGE_TYPE_OPEN_MINING_CHANNEL_ERROR: u8 = 0x12;
pub const MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL: u8 = 0x10;
pub const MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS: u8 = 0x11;
pub const MESSAGE_TYPE_RECONNECT: u8 = 0x25;
pub const MESSAGE_TYPE_SET_CUSTOM_MINING_JOB: u8 = 0x22;
pub const MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_ERROR: u8 = 0x24;
pub const MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_SUCCESS: u8 = 0x23;
pub const MESSAGE_TYPE_SET_EXTRANONCE_PREFIX: u8 = 0x19;
pub const MESSAGE_TYPE_SET_GROUP_CHANNEL: u8 = 0x26;
pub const MESSAGE_TYPE_MINING_SET_NEW_PREV_HASH: u8 = 0x20;
pub const MESSAGE_TYPE_SET_TARGET: u8 = 0x21;
pub const MESSAGE_TYPE_SUBMIT_SHARES_ERROR: u8 = 0x1d;
pub const MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED: u8 = 0x1b;
pub const MESSAGE_TYPE_SUBMIT_SHARES_STANDARD: u8 = 0x1a;
pub const MESSAGE_TYPE_SUBMIT_SHARES_SUCCESS: u8 = 0x1c;
pub const MESSAGE_TYPE_UPDATE_CHANNEL: u8 = 0x16;
pub const MESSAGE_TYPE_UPDATE_CHANNEL_ERROR: u8 = 0x17;

// COMMON MESSAGES CHANNEL BIT
pub const CHANNEL_BIT_SETUP_CONNECTION: bool = false;
pub const CHANNEL_BIT_SETUP_CONNECTION_SUCCESS: bool = false;
pub const CHANNEL_BIT_SETUP_CONNECTION_ERROR: bool = false;
pub const CHANNEL_BIT_CHANNEL_ENDPOINT_CHANGED: bool = true;
// TEMPLATE DISTRIBUTION PROTOCOL MESSAGES CHANNEL BIT
pub const CHANNEL_BIT_COINBASE_OUTPUT_DATA_SIZE: bool = false;
pub const CHANNEL_BIT_NEW_TEMPLATE: bool = false;
pub const CHANNEL_BIT_SET_NEW_PREV_HASH: bool = false;
pub const CHANNEL_BIT_REQUEST_TRANSACTION_DATA: bool = false;
pub const CHANNEL_BIT_REQUEST_TRANSACTION_DATA_SUCCESS: bool = false;
pub const CHANNEL_BIT_REQUEST_TRANSACTION_DATA_ERROR: bool = false;
pub const CHANNEL_BIT_SUBMIT_SOLUTION: bool = false;
// JOB NEGOTIATION PROTOCOL MESSAGES CHANNEL BIT
pub const CHANNEL_BIT_ALLOCATE_MINING_JOB_TOKEN: bool = false;
pub const CHANNEL_BIT_ALLOCATE_MINING_JOB_SUCCESS: bool = false;
pub const CHANNEL_BIT_ALLOCATE_MINING_JOB_ERROR: bool = false; // TODO is on the message type
                                                               // table but is not defined as message
pub const CHANNEL_BIT_IDENTIFY_TRANSACTIONS: bool = false;
pub const CHANNEL_BIT_IDENTIFY_TRANSACTIONS_SUCCESS: bool = false;
pub const CHANNEL_BIT_PROVIDE_MISSING_TRANSACTION: bool = false;
pub const CHANNEL_BIT_PROVIDE_MISSING_TRANSACTION_SUCCESS: bool = false;
// TODO not in messages type table !!!
pub const CHANNEL_BIT_COMMIT_MINING_JOB: bool = false;
pub const CHANNEL_BIT_COMMIT_MINING_JOB_SUCCESS: bool = false;
pub const CHANNEL_BIT_COMMIT_MINING_JOB_ERROR: bool = false;
// MINING PROTOCOL MESSAGES CHANNEL BIT
pub const CHANNEL_BIT_CLOSE_CHANNEL: bool = true;
pub const CHANNEL_BIT_NEW_EXTENDED_MINING_JOB: bool = true;
pub const CHANNEL_BIT_NEW_MINING_JOB: bool = true;
pub const CHANNEL_BIT_OPEN_EXTENDED_MINING_CHANNEL: bool = false;
pub const CHANNEL_BIT_OPEN_EXTENDED_MINING_CHANNEL_SUCCES: bool = false;
// TODO in the spec page 21 is defined OpenMiningChannelError valid for both extended and standard
// messages but in the spec page 40 are defined two different message types for
// OpenStandardMiningChannelError and OpenExtendedMiningChannelError
pub const CHANNEL_BIT_OPEN_MINING_CHANNEL_ERROR: bool = false;
pub const CHANNEL_BIT_OPEN_STANDARD_MINING_CHANNEL: bool = false;
pub const CHANNEL_BIT_OPEN_STANDARD_MINING_CHANNEL_SUCCESS: bool = false;
pub const CHANNEL_BIT_RECONNECT: bool = false;
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
pub const CHANNEL_BIT_UPDATE_CHANNEL: bool = true;
pub const CHANNEL_BIT_UPDATE_CHANNEL_ERROR: bool = true;
