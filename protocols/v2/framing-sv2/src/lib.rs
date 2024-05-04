//! The SV2 protocol is binary, with fixed message framing.
//! Each message begins with the extension type, message type, and message length (six bytes in total), followed by a variable length message.
//!
//! This crate provides primitives for framing of SV2 binary messages.
//!
//! The message framing is outlined below ([according to SV2 specs](https://stratumprotocol.org/specification/03-Protocol-Overview/#32-framing)):
//!
//! | Protocol Type  | Byte Length | Description |
//! |----------------|-------------|-------------|
//! | `extension_type` | `U16` | Unique identifier of the extension describing this protocol message. <br><br> Most significant bit (i.e.bit `15`, `0`-indexed, aka `channel_msg`) indicates a message which is specific to a channel, whereas if the most significant bit is unset, the message is to be interpreted by the immediate receiving device. <br><br> Note that the `channel_msg` bit is ignored in the extension lookup, i.e.an `extension_type` of `0x8ABC` is for the same "extension" as `0x0ABC`. <br><br> If the `channel_msg` bit is set, the first four bytes of the payload field is a `U32` representing the `channel_id` this message is destined for (these bytes are repeated in the message framing descriptions below). <br><br> Note that for the Job Declaration and Template Distribution Protocols the `channel_msg` bit is always unset. |
//! | `msg_type` | `U8` | Unique identifier of the extension describing this protocol message. |
//! | `msg_length` | `U24` | Length of the protocol message, not including this header. |
//! | `payload` | `BYTES` | Message-specific payload of length `msg_length`. If the MSB in `extension_type` (the `channel_msg` bit) is set the first four bytes are defined as a `U32` `"channel_id"`, though this definition is repeated in the message definitions below and these 4 bytes are included in `msg_length`. |
//!
//! # Features
//! This crate can be built with the following features:
//! - `with_serde`: builds `binary_sv2` and `buffer_sv2` crates with `serde`-based encoding and decoding.
//! - `with_buffer_pool`: uses `buffer_sv2` to provide a more efficient allocation method for `non_std` environments. Please refer to `buffer_sv2` crate docs for more context.
//!
//! The `with_serde` feature flag is only used for the Message Generator, and deprecated for any other kind of usage. It will likely be fully deprecated in the future.

#![no_std]
extern crate alloc;

/// SV2 framing types
pub mod framing2;

/// SV2 framing errors
pub mod error;

/// SV2 framing header
pub mod header;
pub use error::Error;
