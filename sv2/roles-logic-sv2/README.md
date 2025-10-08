# `roles_logic_sv2`

[![crates.io](https://img.shields.io/crates/v/roles_logic_sv2.svg)](https://crates.io/crates/roles_logic_sv2)
[![docs.rs](https://docs.rs/roles_logic_sv2/badge.svg)](https://docs.rs/roles_logic_sv2)
[![rustc+](https://img.shields.io/badge/rustc-1.75.0%2B-lightgrey.svg)](https://blog.rust-lang.org/2023/12/28/Rust-1.75.0.html)
[![license](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/stratum-mining/stratum/blob/main/LICENSE.md)
[![codecov](https://codecov.io/gh/stratum-mining/stratum/branch/main/graph/badge.svg?flag=roles_logic_sv2-coverage)](https://codecov.io/gh/stratum-mining/stratum)

`roles_logic_sv2` provides the core logic and utilities for implementing roles in the Stratum V2 (Sv2) protocol, such as miners, pools, and proxies. It abstracts message handling, channel management, job creation, and routing logic, enabling efficient and secure communication across upstream and downstream connections.

## Main Components

- **Channel Logic**: Manages the lifecycle and settings of communication channels (standard, extended, and group ones) between roles.
- **Handlers**: Provides traits for handling logic of Sv2 protocol messages.
- **Job Management**: Facilitates the creation, validation, and dispatching of mining jobs.
- **Parsers**: Handles serialization and deserialization of Sv2 messages via [`binary_sv2`](https://docs.rs/binary_sv2/latest/binary_sv2/index.html).
- **Routing Logic**: Implements message routing and downstream/upstream selector utilities. Useful for advanced proxy implementations with multiplexing of Standard Channels across different upstreams.
- **Utilities**: Provides helpers for safe mutex locking, mining-specific calculations, and more.

## Usage

To include this crate in your project, run:

```bash
cargo add roles_logic_sv2
```

This crate can be built with the following feature flags:

- `prop_test`: Enables property-based testing features for template distribution logic, leveraging dependencies' testing capabilities such as `template_distribution_sv2` crate.
- `disable_nopanic`: Disables the nopanic logic in scenarios where code coverage tools might conflict with it.