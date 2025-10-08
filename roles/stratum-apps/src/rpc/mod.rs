//! RPC utilities for Job Declaration Server
//!
//! This module provides HTTP-based RPC server implementation for JD Server functionality.
//! It includes utilities for handling RPC requests and responses.
//!
//! Originally from the `rpc_sv2` crate.
//!
//! This module is only available when the `rpc` feature is enabled.

pub mod mini_rpc_client;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Hash([u8; 32]);

#[allow(dead_code)]
#[derive(Clone, Deserialize)]
pub struct Amount(f64);

#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlockHash(Hash);

pub use hyper::Uri;
