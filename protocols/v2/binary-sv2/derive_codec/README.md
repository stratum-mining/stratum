# derive_codec_sv2

[![crates.io](https://img.shields.io/crates/v/derive-codec-sv2.svg)](https://crates.io/crates/derive-codec-sv2)
[![docs.rs](https://docs.rs/derive-codec-sv2/badge.svg)](https://docs.rs/derive-codec-sv2)
[![rustc+](https://img.shields.io/badge/rustc-1.75.0%2B-lightgrey.svg)](https://blog.rust-lang.org/2023/12/28/Rust-1.75.0.html)
[![license](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/stratum-mining/stratum/blob/main/LICENSE.md)
[![codecov](https://codecov.io/gh/stratum-mining/stratum/branch/main/graph/badge.svg?flag=binary_codec_sv2-coverage)](https://codecov.io/gh/stratum-mining/stratum)

`derive-codec-sv2` is a no-std Rust crate offering procedural macros for automating serialization and deserialization of structs used within the Sv2 (Stratum V2) protocol. This crate provides `Encodable` and `Decodable` macros to streamline binary data handling, especially useful for protocol-level implementations where efficient encoding and decoding are essential.

## Key Capabilities

- **Automatic Encoding and Decoding**: Derives methods for converting structs to and from binary format, reducing boilerplate code for data    structures used in Sv2.
- **Attribute-Based Configuration**: Supports `#[already_sized]` attribute for marking fixed-size structs, enabling optimizations in binary handling.
- **Flexible Field Parsing**: Allows parsing of fields with lifetimes, generics, and static references, enhancing compatibility with various protocol requirements.
- **Custom Size Calculation**: Provides field-specific size calculation through the derived `GetSize` trait, helpful for dynamic protocol message framing.

## Usage

To include this crate in your project, run:

```sh
cargo add derive-codec-sv2
