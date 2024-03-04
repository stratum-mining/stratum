use rpc::mini_rpc_client::RpcError;
use tokio::task::JoinError;
use tracing::{error, warn};

#[derive(Debug)]
pub enum JdsMempoolError {
    EmptyMempool,
    NoClient,
    Rpc(RpcError),
    PoisonLock(String),
    TokioJoin(JoinError),
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
        JdsMempoolError::TokioJoin(_) => {
            error!("{:?}", err);
            error!("Poison lock error)");
        }
    }
}
