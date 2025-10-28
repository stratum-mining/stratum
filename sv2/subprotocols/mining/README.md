# mining_sv2

[![crates.io](https://img.shields.io/crates/v/mining_sv2.svg)](https://crates.io/crates/mining_sv2)
[![docs.rs](https://docs.rs/mining_sv2/badge.svg)](https://docs.rs/mining_sv2)
[![rustc+](https://img.shields.io/badge/rustc-1.75.0%2B-lightgrey.svg)](https://blog.rust-lang.org/2023/12/28/Rust-1.75.0.html)
[![license](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/stratum-mining/stratum/blob/main/LICENSE.md)
[![codecov](https://codecov.io/gh/stratum-mining/stratum/branch/main/graph/badge.svg)](https://app.codecov.io/gh/stratum-mining/stratum/tree/main/protocols%2Fv2%2Fmining_sv2)

`mining_sv2` is a Rust `#![no_std]` crate that implements a set of  messages defined in the Mining protocol of Stratum V2.
The Mining protocol enables:
- distribution of work to mining devices
- submission of proof of work from mining devices
- notification of custom work to pool (in conjunction with Job Declaration Subprotocol) 

For further information about the messages, please refer to [Stratum V2 documentation - Mining](https://stratumprotocol.org/specification/05-Mining-Protocol/).

## Usage

To include this crate in your project, run:

```bash
$ cargo add mining_sv2
```
