//! ## JDS Mempool Errors
//!
//! This module defines the error types and handling utilities related to the mempool logic in the
//! Job Declarator Server (JDS).
//!
//! These errors are mostly used when interacting with:
//! - the internal mempool data structure
//! - the RPC client that communicates with the Bitcoin node
//! - the synchronization/update routines
//!
//! It also includes a centralized error logging helper (`handle_error`) to standardize warnings
//! and diagnostics across components.

use rpc_sv2::mini_rpc_client::RpcError;
use std::{convert::From, sync::PoisonError};
use tracing::{error, warn};

/// Errors that may occur during JDS mempool operations.
#[derive(Debug)]
pub enum JdsMempoolError {
    /// The mempool was found to be empty (likely due to testnet/signet conditions).
    EmptyMempool,
    /// Failed to construct a valid RPC client (e.g. invalid URL, malformed credentials).
    NoClient,
    /// An RPC call to the Bitcoin node failed.
    Rpc(RpcError),
    /// A poisoned lock was encountered while accessing the mempool
    PoisonLock(String),
}

impl From<RpcError> for JdsMempoolError {
    fn from(value: RpcError) -> Self {
        JdsMempoolError::Rpc(value)
    }
}

impl<T> From<PoisonError<T>> for JdsMempoolError {
    fn from(value: PoisonError<T>) -> Self {
        JdsMempoolError::PoisonLock(value.to_string())
    }
}

/// Logs a structured diagnostic message for a given mempool error.
///
/// This function is used throughout the codebase to provide more meaningful context
/// in logs when mempool-related operations fail.
pub fn handle_error(err: &JdsMempoolError) {
    match err {
        JdsMempoolError::EmptyMempool => {
            warn!("{:?}", err);
            warn!("Template Provider is running, but its MEMPOOL is empty (possible reasons: you're testing in testnet, signet, or regtest)");
        }
        JdsMempoolError::NoClient => {
            error!("{:?}", err);
            error!("Unable to establish RPC connection with Template Provider (possible reasons: not fully synced, down)");
        }
        JdsMempoolError::Rpc(_) => {
            error!("{:?}", err);
            error!("Unable to establish RPC connection with Template Provider (possible reasons: not fully synced, down)");
        }
        JdsMempoolError::PoisonLock(_) => {
            error!("{:?}", err);
            error!("Poison lock error)");
        }
    }
}
