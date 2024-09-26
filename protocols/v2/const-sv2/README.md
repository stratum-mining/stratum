# const_sv2

[![crates.io](https://img.shields.io/crates/v/const_sv2.svg)](https://crates.io/crates/const_sv2)
[![docs.rs](https://docs.rs/const_sv2/badge.svg)](https://docs.rs/const_sv2)

`const_sv2` is a Rust crate that provides essential constants for the SV2 (Stratum V2) protocol. These constants are crucial for message framing, encryption, and protocol-specific identifiers across various SV2 components, including Mining, Job Declaration, and Template Distribution protocols.

## Main Components

- **Protocol Constants**: Define key protocol discriminants, message types, and sizes for the SV2 binary protocol.
- **Encryption Support**: Includes constants for encryption using ChaChaPoly and ElligatorSwift encoding.
- **Channel Bits**: Defines whether specific messages are associated with a channel, simplifying protocol handling.
- **Modular**: Supports a `no_std` environment, enabling use in embedded systems or environments without a standard library.

## Usage

To use this crate, add it to your `Cargo.toml`:

```toml
[dependencies]
const_sv2 = "2.0.0"
