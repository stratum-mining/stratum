# template_distribution_sv2

[![crates.io](https://img.shields.io/crates/v/template_distribution_sv2.svg)](https://crates.io/crates/template_distribution_sv2)
[![docs.rs](https://docs.rs/template_distribution_sv2/badge.svg)](https://docs.rs/template_distribution_sv2)
[![rustc+](https://img.shields.io/badge/rustc-1.75.0%2B-lightgrey.svg)](https://blog.rust-lang.org/2023/12/28/Rust-1.75.0.html)
[![license](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/stratum-mining/stratum/blob/main/LICENSE.md)
[![codecov](https://codecov.io/gh/stratum-mining/stratum/branch/main/graph/badge.svg)](https://app.codecov.io/gh/stratum-mining/stratum/tree/main/protocols%2Fv2%2Ftemplate_distribution_sv2)

`template_distribution_sv2` is a Rust `#![no_std]` crate that implements a set of messages defined in the
Template Distribution Protocol of Stratum V2. The Template Distribution protocol can be used to
receive updates of the block templates to use in mining.

For further information about the messages, please refer to [Stratum V2 documentation - Job Distribution](https://stratumprotocol.org/specification/07-Template-Distribution-Protocol/).

## Build Options

This crate can be built with the following features:
- `prop_test`: Enables support for property testing.

## Usage

To include this crate in your project, run:

```bash
$ cargo add template_distribution_sv2
```
