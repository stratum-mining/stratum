# binary_codec_sv2

[![crates.io](https://img.shields.io/crates/v/binary_codec_sv2.svg)](https://crates.io/crates/binary_codec_sv2)  
[![docs.rs](https://docs.rs/binary_codec_sv2/badge.svg)](https://docs.rs/binary_codec_sv2)  
[![rustc+](https://img.shields.io/badge/rustc-1.75.0%2B-lightgrey.svg)](https://blog.rust-lang.org/2023/12/28/Rust-1.75.0.html)  
[![license](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/stratum-mining/stratum/blob/main/LICENSE.md)
[![codecov](https://codecov.io/gh/stratum-mining/stratum/branch/main/graph/badge.svg?flag=binary_codec_sv2-coverage)](https://codecov.io/gh/stratum-mining/stratum)  

`binary_codec_sv2` is a `no_std` Rust crate that helps serialize and de-serialize binary data into and from Stratum V2 types.

## Key Features

- **Comprehensive Encoding and Decoding**: Provides traits (`Encodable`, `Decodable`) for converting between Rust and SV2 data types/structures.  
- **Support for Complex Data Structures**: Handles primitives, nested structures, and protocol-specific types like `U24`, `U256`,`Str0255` and rest.  
- **Error Handling**: Robust mechanisms for managing encoding/decoding failures, including size mismatches and invalid data.   
- **`no_std` Compatibility**: Fully supports constrained environments without the Rust standard library.  

## Sv2 Type Mapping

The crate supports the following mappings between Rust and SV2 types

| Rust Type   | Sv2 Type       |  
|-------------|----------------|  
| `bool`      | `BOOL`         |  
| `u8`        | `U8`           |  
| `u16`       | `U16`          |  
| `U24`       | `U24`          |  
| `u32`       | `U32`          |  
| `u64`       | `U64`          |  
| `f32`       | `F32`          |  
| `Str0255`   | `STRO_255`     |  
| `Signature` | `SIGNATURE`    |  
| `[u8]`      | `BYTES`        |  
| `Seq0255`   | `SEQ0_255[T]`  |  
| `Seq064K`   | `SEQ0_64K[T]`  | 

## Installation

Add `binary_codec_sv2` to your project by running:

```sh
cargo add binary_codec_sv2
``` 
