# `framing_sv2`

[![crates.io](https://img.shields.io/crates/v/framing_sv2.svg)](https://crates.io/crates/framing_sv2)
[![docs.rs](https://docs.rs/framing_sv2/badge.svg)](https://docs.rs/framing_sv2)
[![rustc+](https://img.shields.io/badge/rustc-1.75.0%2B-lightgrey.svg)](https://blog.rust-lang.org/2023/12/28/Rust-1.75.0.html)
[![license](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/stratum-mining/stratum/blob/main/LICENSE.md)
[![codecov](https://codecov.io/gh/stratum-mining/stratum/branch/main/graph/badge.svg?flag=framing_sv2-coverage)](https://codecov.io/gh/stratum-mining/stratum)

`framing_sv2` provides utilities for framing messages sent between Sv2 roles, handling both Sv2
message and Noise handshake frames.

The Sv2 protocol is binary, with fixed message framing. Each message begins with the extension
type, message type, and message length (six bytes in total), followed by a variable length
message. The message framing is outlined below ([according to Sv2 specs
](https://github.com/stratum-mining/sv2-spec/blob/main/03-Protocol-Overview.md#32-framing)):

| Field Name  | Byte Length | Description |
|----------------|-------------|-------------|
| `extension_type` | `U16` | Unique identifier of the extension associated with this protocol message. |
| `msg_type` | `U8` | Unique identifier of this protocol message. |
| `msg_length` | `U24` | Length of the protocol message, not including this header. |
| `payload` | `BYTES` | Message-specific payload of length `msg_length`. If the MSB in `extension_type` (the `channel_msg` bit) is set the first four bytes are defined as a `U32` `"channel_id"`, though this definition is repeated in the message definitions below and these 4 bytes are included in `msg_length`. |

Some bits of the `extension_type` field can also be repurposed for signaling on how the frame
should be handled across channels.

The least significant bit of `extension_type` (i.e.bit 15, 0-indexed, aka `channel_msg`) indicates
a message which is specific to a channel, whereas if the most significant bit is unset, the message
is to be interpreted by the immediate receiving device.

Note that the `channel_msg` bit is ignored in the extension lookup, i.e.an `extension_type` of
`0x8ABC` is for the same "extension" as `0x0ABC`.

If the `channel_msg` bit is set, the first four bytes of the payload field is a `U32` representing
the `channel_id` this message is destined for (these bytes are repeated in the message framing
descriptions below).

Note that for the Job Declaration and Template Distribution Protocols the `channel_msg` bit is
always unset.

## Main Components

- **Header**: Defines the 6-byte Sv2 message header with information about the message payload,
  including its extension type, if it is associated with a specific mining channel, the type of
  message (e.g. `SetupConnection`, `NewMiningJob`, etc.) and the payload length.
- **Sv2 Framing**: Use for serializing Sv2 messages.
- **Noise Handshake Framing**: Use for serializing Noise protocol handshake messages.

## Usage

To include this crate in your project, run:

```bash
cargo add framing_sv2
```

This crate can be built with the following feature flags:

- `with_buffer_pool`: Enables buffer pooling for more efficient memory management.

### Examples

This crate provides an example demonstrating how to serialize and deserialize Sv2 message frames:

1. **[Sv2 Frame](https://github.com/stratum-mining/stratum/blob/main/protocols/v2/framing-sv2/examples/sv2_frame.rs)**:
   Constructs, serializes, and deserialize a regular Sv2 message frame (`Sv2Frame`).