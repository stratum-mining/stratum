use rpc_sv2::mini_rpc_client::RpcError;
use std::{convert::From, sync::PoisonError};
use tracing::{error, warn};

#[derive(Debug)]
pub enum JdsMempoolError {
    EmptyMempool,
    NoClient,
    Rpc(RpcError),
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
