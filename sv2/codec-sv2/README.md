# `codec_sv2`

[![crates.io](https://img.shields.io/crates/v/codec_sv2.svg)](https://crates.io/crates/codec_sv2)
[![docs.rs](https://docs.rs/codec_sv2/badge.svg)](https://docs.rs/codec_sv2)
[![rustc+](https://img.shields.io/badge/rustc-1.75.0%2B-lightgrey.svg)](https://blog.rust-lang.org/2023/12/28/Rust-1.75.0.html)
[![license](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/stratum-mining/stratum/blob/main/LICENSE.md)
[![codecov](https://codecov.io/gh/stratum-mining/stratum/branch/main/graph/badge.svg?flag=codec_sv2-coverage)](https://codecov.io/gh/stratum-mining/stratum)

`codec_sv2` provides the message encoding and decoding functionality for the Stratum V2 (Sv2)
protocol, handling secure communication between Sv2 roles. This crate abstracts the complexity of
message encoding/decoding with optional Noise protocol support, ensuring both regular and encrypted
messages can be serialized, transmitted, and decoded consistently and reliably.

## Main Components

- **Encoder**: Encodes Sv2 messages with or without Noise protocol support.
- **Decoder**: Decodes Sv2 messages with or without Noise protocol support.
- **Handshake State**: Manages the current Noise protocol handshake state of the codec.


## Usage

To include this crate in your project, run:

```bash
cargo add codec_sv2
```

This crate can be built with the following feature flags:

- `std`: Enable usage of rust `std` library, enabled by default.
- `noise_sv2`: Enables support for Noise protocol encryption and decryption.
- `with_buffer_pool`: Enables buffer pooling for more efficient memory management.

In order to use this crate in a `#![no_std]` environment, use the `--no-default-features` to remove the `std` feature.

### Examples

This crate provides two examples demonstrating how to encode and decode Sv2 frames:

1. **[Unencrypted Example](https://github.com/stratum-mining/stratum/blob/main/protocols/v2/codec-sv2/examples/unencrypted.rs)**:
   Encode and decode standard Sv2 frames, detailing the message serialization and transmission
   process for unencrypted communications.

2. **[Encrypted Example](https://github.com/stratum-mining/stratum/blob/main/protocols/v2/codec-sv2/examples/encrypted.rs)**:
   Encode and decode Sv2 frames with Noise protocol encryption, detailing the entire encryption
   handshake and transport phase and serialization and transmission process for encrypted
   communications.
