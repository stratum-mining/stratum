
# noise_sv2

[![crates.io](https://img.shields.io/crates/v/const_sv2.svg)](https://crates.io/crates/const_sv2)
[![docs.rs](https://docs.rs/const_sv2/badge.svg)](https://docs.rs/const_sv2)
[![rustc+](https://img.shields.io/badge/rustc-1.75.0%2B-lightgrey.svg)](https://blog.rust-lang.org/2023/12/28/Rust-1.75.0.html)
[![license](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/stratum-mining/stratum/blob/main/LICENSE.md)
[![docs.rs](https://codecov.io/gh/stratum-mining/stratum/branch/main/graph/badge.svg)](https://app.codecov.io/gh/stratum-mining/stratum/tree/main/protocols%2Fv2%2Fnoise-sv2)

`noise_sv2` is primarily intended to secure communication in the Stratum V2 protocol. It handles the necessary Noise handshakes, encrypts outgoing messages, and decrypts incoming responses, ensuring privacy and integrity across the communication link between clients (downstream) and servers (upstream).

## Key Capabilities
* **Secure Communication**: Provides robust encryption and authentication for messages exchanged between different roles (downstream and upstream) in a mining pool architecture.
* **Cipher Support**: Includes support for both AES-GCM and ChaCha20-Poly1305, two well-known and widely used encryption algorithms.
* **Handshake Roles**: Implements the `Initiator` and `Responder` roles required by the Noise handshake, allowing both sides of a connection to establish secure communication.
* **Cryptographic Helpers**: Comes with utility types to facilitate the management of cryptographic state and encryption operations, simplifying usage for developers.

## Usage
To include this crate in your project, you can run the following command in your terminal:

```bash
cargo add noise_sv2
