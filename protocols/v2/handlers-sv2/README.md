
# handlers_sv2

[![crates.io](https://img.shields.io/crates/v/handlers_sv2.svg)](https://crates.io/crates/handlers_sv2)
[![docs.rs](https://docs.rs/handlers_sv2/badge.svg)](https://docs.rs/handlers_sv2)
[![rustc+](https://img.shields.io/badge/rustc-1.75.0%2B-lightgrey.svg)](https://blog.rust-lang.org/2023/12/28/Rust-1.75.0.html)
[![license](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/stratum-mining/stratum/blob/main/LICENSE.md)
[![codecov](https://codecov.io/gh/stratum-mining/stratum/branch/main/graph/badge.svg?flag=handlers_sv2-coverage)](https://codecov.io/gh/stratum-mining/stratum)

The `handlers_sv2` crate defines traits for handling Sv2 messages, with separate variants for servers and clients. Implementors can choose which message types to support—such as `Mining`, `TemplateDistribution`, `Common`, or `JobDeclaration` based on their role in the system. Both synchronous and asynchronous versions are provided, making the crate adaptable to different execution environments.

## Usage
To include this crate in your project, run:

```bash
cargo add handlers_sv2
```