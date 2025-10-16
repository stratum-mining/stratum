//! # Stratum Apps - SV2 Application Utilities
//!
//! This crate consolidates the essential utilities needed for building Stratum V2 applications.
//! It combines the functionality from the original separate utility crates into a single,
//! well-organized library with feature-based compilation.
//!
//! ## Features
//!
//! ### Core Features
//! - `network` - High-level networking utilities (enabled by default)
//! - `config` - Configuration management helpers (enabled by default)
//! - `rpc` - RPC utilities with custom types for JSON-RPC communication (optional)
//!
//! ### Role-Specific Feature Bundles
//! - `pool` - Everything needed for pool applications
//! - `jd_client` - Everything needed for JD client applications
//! - `jd_server` - Everything needed for JD server applications (includes RPC)
//! - `translator` - Everything needed for translator applications (includes SV1)
//! - `mining_device` - Everything needed for mining device applications
//!
//! ## Modules
//!
//! - [`network_helpers`] - High-level networking utilities for SV2 connections
//! - [`config_helpers`] - Configuration management and parsing utilities
//! - [`rpc`] - RPC utilities with custom serializable types (`Hash`, `BlockHash`, `Amount`)

/// Re-export all the modules from `stratum_core`
pub use stratum_core;

/// High-level networking utilities for SV2 connections
///
/// Provides connection management, encrypted streams, and protocol handling.
/// Originally from the `network_helpers_sv2` crate.
#[cfg(feature = "network")]
pub mod network_helpers;

/// Configuration management helpers
///
/// Utilities for parsing configuration files, handling coinbase outputs,
/// and setting up logging. Originally from the `config_helpers_sv2` crate.
#[cfg(feature = "config")]
pub mod config_helpers;

/// RPC utilities for Job Declaration Server
///
/// HTTP-based RPC server implementation for JD Server functionality.
/// Originally from the `rpc_sv2` crate.
#[cfg(feature = "rpc")]
pub mod rpc;

/// Key utilities for cryptographic operations
///
/// Provides Secp256k1 key management, serialization/deserialization, and signature services.
/// Supports both standard and no_std environments.
pub mod key_utils;

/// Error handling utilities
///
/// Provides the `handle_result!` macro and error handling logic for Stratum applications.
pub mod error_handling;
