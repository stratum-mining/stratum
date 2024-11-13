# const_sv2

[![crates.io](https://img.shields.io/crates/v/const_sv2.svg)](https://crates.io/crates/const_sv2)
[![docs.rs](https://docs.rs/const_sv2/badge.svg)](https://docs.rs/const_sv2)
[![rustc+](https://img.shields.io/badge/rustc-1.75.0%2B-lightgrey.svg)](https://blog.rust-lang.org/2023/12/28/Rust-1.75.0.html)
[![license](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/stratum-mining/stratum/blob/main/LICENSE.md)

`const_sv2` is a Rust `no_std` crate that provides essential constants for the Sv2 (Stratum V2) protocol. These constants are crucial for message framing, encryption, and protocol-specific identifiers across various Sv2 components, including Mining, Job Declaration, and Template Distribution protocols.

## Key Capabilities

- **Protocol Constants**: Define key protocol discriminants, message types, and sizes for the Sv2 binary protocol.
- **Encryption Support**: Includes constants for encryption using `ChaChaPoly` and `ElligatorSwift` encoding.
- **Channel Bits**: Defines whether specific messages are associated with a channel, simplifying protocol handling.
- **Modular**: Supports a `no_std` environment, enabling use in embedded systems or environments without a standard library.

## Usage

To include this crate in your project, run:

```sh
cargo add const_sv2
