
# noise_sv2

[![crates.io](https://img.shields.io/crates/v/noise_sv2.svg)](https://crates.io/crates/noise_sv2)
[![docs.rs](https://docs.rs/noise_sv2/badge.svg)](https://docs.rs/noise_sv2)
[![rustc+](https://img.shields.io/badge/rustc-1.75.0%2B-lightgrey.svg)](https://blog.rust-lang.org/2023/12/28/Rust-1.75.0.html)
[![license](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/stratum-mining/stratum/blob/main/LICENSE.md)
[![codecov](https://codecov.io/gh/stratum-mining/stratum/branch/main/graph/badge.svg?flag=noise_sv2-coverage)](https://codecov.io/gh/stratum-mining/stratum)

`noise_sv2` is primarily intended to secure communication in the Stratum V2 (Sv2) protocol. It handles the necessary Noise handshakes, encrypts outgoing messages, and decrypts incoming responses, ensuring privacy and integrity across the communication link between Sv2 roles. See the [Protocol Security specification](https://github.com/stratum-mining/sv2-spec/blob/main/04-Protocol-Security.md) for more details.

## Key Capabilities
* **Secure Communication**: Provides encryption and authentication for messages exchanged between different Sv2 roles.
* **Cipher Support**: Includes support for both `AES-GCM` and `ChaCha20-Poly1305`.
* **Handshake Roles**: Implements the `Initiator` and `Responder` roles required by the Noise handshake, allowing both sides of a connection to establish secure communication.
* **Cryptographic Helpers**: Facilitates the management of cryptographic state and encryption operations.

## Usage
To include this crate in your project, run:

```bash
cargo add noise_sv2
```

This crate can be built with the following feature flags:

- `std`: Enable usage of rust `std` library, enabled by default.

In order to use this crate in a `#![no_std]` environment, use the `--no-default-features` to remove the `std` feature.

### Examples

This crate provides example on establishing a secure line:

1. **[Noise Handshake Example](https://github.com/stratum-mining/stratum/blob/main/protocols/v2/noise-sv2/examples/handshake.rs)**:
   Establish a secure line of communication between an Initiator and Responder via the Noise
   protocol, allowing for the encryption and decryption of a secret message.
