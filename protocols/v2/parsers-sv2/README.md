# `parsers_sv2`

[![crates.io](https://img.shields.io/crates/v/parsers_sv2.svg)](https://crates.io/crates/parsers_sv2)
[![docs.rs](https://docs.rs/parsers_sv2/badge.svg)](https://docs.rs/parsers_sv2)
[![rustc+](https://img.shields.io/badge/rustc-1.75.0%2B-lightgrey.svg)](https://blog.rust-lang.org/2023/12/28/Rust-1.75.0.html)
[![license](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/stratum-mining/stratum/blob/main/LICENSE.md)
[![codecov](https://codecov.io/gh/stratum-mining/stratum/branch/main/graph/badge.svg?flag=parsers_sv2-coverage)](https://codecov.io/gh/stratum-mining/stratum)

`parsers_sv2` provides logic to convert raw Stratum V2 (Sv2) message data into Rust types, as well as logic to handle conversions among Sv2 Rust types. The crate is `no_std` compatible by default.

Most of the logic on this crate is tightly coupled with the [`binary_sv2`](https://docs.rs/binary_sv2/latest/binary_sv2/) crate.
