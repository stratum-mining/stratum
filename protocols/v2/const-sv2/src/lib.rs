//! This crate provides all constants used in the SV2 protocol.
//! These constants are essential for message framing, encryption, and
//! protocol-specific identifiers across various SV2 components, including
//! Mining, Job Declaration, and Template Distribution protocols.
//!
//! It also includes definitions for key encryption settings and message types,
//! ensuring consistency and accuracy when working with SV2's binary protocol.
//! These constants are used throughout the system to ensure that SV2
//! messages are formatted and interpreted correctly.
//!
//! ### Discriminants for Stratum V2 (sub)protocols
//! Discriminants are unique identifiers used to distinguish between different
//! Stratum V2 (sub)protocols. Each protocol within the SV2 ecosystem has a
//! specific discriminant value that indicates its type, enabling the correct
//! interpretation and handling of messages. These discriminants ensure that
//! messages are processed by the appropriate protocol handlers,
//! thereby facilitating seamless communication across different components of
//! the SV2 architecture. More info can be found [on Chapter 03 of the Stratum V2 specs](https://github.com/stratum-mining/sv2-spec/blob/main/03-Protocol-Overview.md#3-protocol-overview).

//!
//! ### Message Types
//! Message types in the SV2 protocol define the specific operations and data
//! exchanges between participants. Each type corresponds to a distinct action
//! or information transfer, facilitating communication in various contexts such
//! as mining operations, job declarations, and template distribution. Properly
//! identifying and handling these message types is crucial for maintaining
//! protocol compliance and ensuring seamless interactions within the SV2
//! ecosystem. The message types are categorized into common types and those
//! specific to each subprotocol, providing clarity on their intended use and
//! interaction patterns.
//!
//! ### Channel Bits
//! The `channel bits` indicate whether a message is associated with a specific
//! channel. If the most significant bit of the `extension_type` (referred to as
//! `channel_msg`) is set, the message is related to a channel and includes a
//! `channel_id`. In this case, the first 4 bytes of the payload represent the
//! `channel_id` the message is destined for.

#![cfg_attr(feature = "no_std", no_std)]

/// Identifier for the extension_type field in the SV2 frame, indicating no
/// extensions.
pub const EXTENSION_TYPE_NO_EXTENSION: u16 = 0;

/// Size of the SV2 frame header in bytes.
pub const SV2_FRAME_HEADER_SIZE: usize = 6;

// It's not used anywhere.
// Refactoring: deprecate it.
pub const SV2_FRAME_HEADER_LEN_OFFSET: usize = 3;

// It's not used anywhere.
// Refactoring: deprecate it.
pub const SV2_FRAME_HEADER_LEN_END: usize = 3;

/// Maximum size of an SV2 frame chunk in bytes.
pub const SV2_FRAME_CHUNK_SIZE: usize = 65535;

/// Size of the MAC for supported AEAD encryption algorithm (ChaChaPoly).
pub const AEAD_MAC_LEN: usize = 16;

/// Size of the encrypted SV2 frame header, including the MAC.
// Refactoring: declared in sv2-ffi, and imported in framing_sv2/src/header.rs
// header.rs is then imported into codec_sv2 just for this constant
pub const ENCRYPTED_SV2_FRAME_HEADER_SIZE: usize = SV2_FRAME_HEADER_SIZE + AEAD_MAC_LEN;

/// Size of the Noise protocol frame header in bytes.
// Refactoring: declared in sv2-ffi, and imported in framing_sv2/src/header.rs
// header.rs is then imported into codec_sv2 just for this constant
pub const NOISE_FRAME_HEADER_SIZE: usize = 2;

// Refactoring: declared in sv2-ffi, and imported in framing_sv2/src/header.rs
// header.rs is then imported into codec_sv2 just for this constant, and the
// const there is not even used
pub const NOISE_FRAME_HEADER_LEN_OFFSET: usize = 0;

// It's not used anywhere.
// Refactoring: deprecate it.
pub const NOISE_FRAME_MAX_SIZE: usize = u16::MAX as usize;

/// Size in bytes of the encoded elliptic curve point using ElligatorSwift
/// encoding. This encoding produces a 64-byte representation of the
/// X-coordinate of a secp256k1 curve point.
pub const ELLSWIFT_ENCODING_SIZE: usize = 64;

// Refactoring: the alias could be created where it's imported, or we could just
// use one name everywhere
pub const RESPONDER_EXPECTED_HANDSHAKE_MESSAGE_SIZE: usize = ELLSWIFT_ENCODING_SIZE;

// This is the same as AEAD_MAC_LEN.
// Refactoring: deprecate it.
pub const MAC: usize = 16;

/// Size in bytes of the encrypted ElligatorSwift encoded data, which includes
/// the original ElligatorSwift encoded data and a MAC for integrity
/// verification.
pub const ENCRYPTED_ELLSWIFT_ENCODING_SIZE: usize = ELLSWIFT_ENCODING_SIZE + MAC;

/// Size in bytes of the SIGNATURE_NOISE_MESSAGE, which contains information and
/// a signature for the handshake initiator, formatted according to the Noise
/// Protocol specifications.
pub const SIGNATURE_NOISE_MESSAGE_SIZE: usize = 74;

/// Size in bytes of the encrypted signature noise message, which includes the
/// SIGNATURE_NOISE_MESSAGE and a MAC for integrity verification.
pub const ENCRYPTED_SIGNATURE_NOISE_MESSAGE_SIZE: usize = SIGNATURE_NOISE_MESSAGE_SIZE + MAC;

/// Size in bytes of the handshake message expected by the initiator,
/// encompassing:
/// - ElligatorSwift encoded public key
/// - Encrypted ElligatorSwift encoding
/// - Encrypted SIGNATURE_NOISE_MESSAGE
pub const INITIATOR_EXPECTED_HANDSHAKE_MESSAGE_SIZE: usize = ELLSWIFT_ENCODING_SIZE
    + ENCRYPTED_ELLSWIFT_ENCODING_SIZE
    + ENCRYPTED_SIGNATURE_NOISE_MESSAGE_SIZE;

/// If protocolName is less than or equal to 32 bytes in length, use
/// protocolName with zero bytes appended to make 32 bytes. Otherwise, apply
/// HASH to it. For name = "Noise_NX_Secp256k1+EllSwift_ChaChaPoly_SHA256", we
/// need the hash. More info can be found [at this link](https://github.com/stratum-mining/sv2-spec/blob/main/04-Protocol-Security.md#451-handshake-act-1-nx-handshake-part-1---e).
pub const NOISE_HASHED_PROTOCOL_NAME_CHACHA: [u8; 32] = [
    46, 180, 120, 129, 32, 142, 158, 238, 31, 102, 159, 103, 198, 110, 231, 14, 169, 234, 136, 9,
    13, 80, 63, 232, 48, 220, 75, 200, 62, 41, 191, 16,
];

// len = 1
// 47,53,45,41 = AESG
// We're dropping support for AESG.
// Refactoring: deprecate it.
pub const NOISE_SUPPORTED_CIPHERS_MESSAGE: [u8; 5] = [1, 0x47, 0x53, 0x45, 0x41];

// Discriminants for distinct Stratum V2 (sub)protocols. More info at https://github.com/stratum-
// mining/sv2-spec/blob/main/03-Protocol-Overview.md#3-protocol-overview
pub const SV2_MINING_PROTOCOL_DISCRIMINANT: u8 = 0;
pub const SV2_JOB_DECLARATION_PROTOCOL_DISCRIMINANT: u8 = 1;
// Refactoring: rename this into SV2_TEMPLATE_DISTRIBUTION_PROTOCOL_DISCRIMINANT
pub const SV2_TEMPLATE_DISTR_PROTOCOL_DISCRIMINANT: u8 = 2;

// Common message types used across all Stratum V2 (sub)protocols.
pub const MESSAGE_TYPE_SETUP_CONNECTION: u8 = 0x0;
pub const MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS: u8 = 0x1;
pub const MESSAGE_TYPE_SETUP_CONNECTION_ERROR: u8 = 0x2;
pub const MESSAGE_TYPE_CHANNEL_ENDPOINT_CHANGED: u8 = 0x3;

// Mining Protocol message types.
pub const MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL: u8 = 0x10;
pub const MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS: u8 = 0x11;
pub const MESSAGE_TYPE_OPEN_MINING_CHANNEL_ERROR: u8 = 0x12;
pub const MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL: u8 = 0x13;
// Refactoring: fix typo with MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCESS
pub const MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCES: u8 = 0x14;
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

// Refactoring: we need to move this to 0x04 and shift SETGROUPCHANNEL to 0x25
// (we are not specs compliant now)
pub const MESSAGE_TYPE_RECONNECT: u8 = 0x25;
pub const MESSAGE_TYPE_SET_GROUP_CHANNEL: u8 = 0x26;

// Job Declaration Protocol message types.
pub const MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN: u8 = 0x50;
pub const MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN_SUCCESS: u8 = 0x51;
pub const MESSAGE_TYPE_IDENTIFY_TRANSACTIONS: u8 = 0x53;
pub const MESSAGE_TYPE_IDENTIFY_TRANSACTIONS_SUCCESS: u8 = 0x54;
pub const MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS: u8 = 0x55;
pub const MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS_SUCCESS: u8 = 0x56;
pub const MESSAGE_TYPE_DECLARE_MINING_JOB: u8 = 0x57;
pub const MESSAGE_TYPE_DECLARE_MINING_JOB_SUCCESS: u8 = 0x58;
pub const MESSAGE_TYPE_DECLARE_MINING_JOB_ERROR: u8 = 0x59;
pub const MESSAGE_TYPE_SUBMIT_SOLUTION_JD: u8 = 0x60;

// Template Distribution Protocol message types.
pub const MESSAGE_TYPE_COINBASE_OUTPUT_DATA_SIZE: u8 = 0x70;
pub const MESSAGE_TYPE_NEW_TEMPLATE: u8 = 0x71;
pub const MESSAGE_TYPE_SET_NEW_PREV_HASH: u8 = 0x72;
pub const MESSAGE_TYPE_REQUEST_TRANSACTION_DATA: u8 = 0x73;
pub const MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_SUCCESS: u8 = 0x74;
pub const MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_ERROR: u8 = 0x75;
pub const MESSAGE_TYPE_SUBMIT_SOLUTION: u8 = 0x76;

// The `channel bits` indicate whether a message is associated with a specific
// channel. If the most significant bit of the `extension_type` (referred to as
// `channel_msg`) is set, the message is related to a channel and includes a
// `channel_id`. In this case, the first 4 bytes of the payload represent the
// `channel_id` the message is destined for. For the Job Declaration and
// Template Distribution protocols, the `channel_msg` bit is always unset.

pub const CHANNEL_BIT_SETUP_CONNECTION: bool = false;
pub const CHANNEL_BIT_SETUP_CONNECTION_SUCCESS: bool = false;
pub const CHANNEL_BIT_SETUP_CONNECTION_ERROR: bool = false;
pub const CHANNEL_BIT_CHANNEL_ENDPOINT_CHANGED: bool = true;

// For the Template Distribution protocol, the channel bit is always unset.
pub const CHANNEL_BIT_COINBASE_OUTPUT_DATA_SIZE: bool = false;
pub const CHANNEL_BIT_NEW_TEMPLATE: bool = false;
pub const CHANNEL_BIT_SET_NEW_PREV_HASH: bool = false;
pub const CHANNEL_BIT_REQUEST_TRANSACTION_DATA: bool = false;
pub const CHANNEL_BIT_REQUEST_TRANSACTION_DATA_SUCCESS: bool = false;
pub const CHANNEL_BIT_REQUEST_TRANSACTION_DATA_ERROR: bool = false;
pub const CHANNEL_BIT_SUBMIT_SOLUTION: bool = false;

// In the Job Declaration protocol, the `channel_msg` bit is always unset,
// except for `SUBMIT_SOLUTION_JD`, which requires a specific channel reference.
pub const CHANNEL_BIT_ALLOCATE_MINING_JOB_TOKEN: bool = false;
pub const CHANNEL_BIT_ALLOCATE_MINING_JOB_TOKEN_SUCCESS: bool = false;
pub const CHANNEL_BIT_DECLARE_MINING_JOB: bool = false;
pub const CHANNEL_BIT_DECLARE_MINING_JOB_SUCCESS: bool = false;
pub const CHANNEL_BIT_DECLARE_MINING_JOB_ERROR: bool = false;
pub const CHANNEL_BIT_IDENTIFY_TRANSACTIONS: bool = false;
pub const CHANNEL_BIT_IDENTIFY_TRANSACTIONS_SUCCESS: bool = false;
pub const CHANNEL_BIT_PROVIDE_MISSING_TRANSACTIONS: bool = false;
pub const CHANNEL_BIT_PROVIDE_MISSING_TRANSACTIONS_SUCCESS: bool = false;
pub const CHANNEL_BIT_SUBMIT_SOLUTION_JD: bool = true;

// Channel bits in the Mining protocol vary depending on the message.
pub const CHANNEL_BIT_CLOSE_CHANNEL: bool = true;
pub const CHANNEL_BIT_NEW_EXTENDED_MINING_JOB: bool = true;
pub const CHANNEL_BIT_NEW_MINING_JOB: bool = true;
pub const CHANNEL_BIT_OPEN_EXTENDED_MINING_CHANNEL: bool = false;
pub const CHANNEL_BIT_OPEN_EXTENDED_MINING_CHANNEL_SUCCES: bool = false;
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
