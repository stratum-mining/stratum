# job_declaration_sv2

[![crates.io](https://img.shields.io/crates/v/job_declaration_sv2.svg)](https://crates.io/crates/job_declaration_sv2)
[![docs.rs](https://docs.rs/job_declaration_sv2/badge.svg)](https://docs.rs/job_declaration_sv2)
[![rustc+](https://img.shields.io/badge/rustc-1.75.0%2B-lightgrey.svg)](https://blog.rust-lang.org/2023/12/28/Rust-1.75.0.html)
[![license](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/stratum-mining/stratum/blob/main/LICENSE.md)
[![codecov](https://codecov.io/gh/stratum-mining/stratum/branch/main/graph/badge.svg)](https://app.codecov.io/gh/stratum-mining/stratum/tree/main/protocols%2Fv2%2Fjob_declaration_sv2)

The following crate contains the messages defined in the Job Declaration Protocol of Stratum V2.
This protocol runs between the Job Declarator Server(JDS) and Job Declarator Client(JDC). and can be
provided as a trusted 3rd party service for mining farms.

For further information about the messages, please refer to [Stratum V2 documentation - Job Distribution](https://stratumprotocol.org/specification/06-Job-Declaration-Protocol/).

## Build Options

This crate can be built with the following features:
- `std`: Enables support for standard library features.
- `with_serde`: Enables support for serialization and deserialization using Serde.

**Note that this feature flag is only used for the Message Generator, and deprecated for any other
kind of usage. It will likely be fully deprecated in the future.**

## Usage

To include this crate in your project, run:

```bash
$ cargo add job_declaration_sv2
```
