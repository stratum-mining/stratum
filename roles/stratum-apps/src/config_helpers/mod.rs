//! Configuration management helpers for SV2 applications
//!
//! This module provides utilities for:
//! - Parsing configuration files (TOML, etc.)
//! - Handling coinbase output specifications
//! - Setting up logging and tracing
//!
//! Originally from the `config_helpers_sv2` crate.

mod coinbase_output;
pub use coinbase_output::{CoinbaseRewardScript, Error as CoinbaseOutputError};

pub mod logging;

mod toml;
pub use toml::duration_from_toml;
