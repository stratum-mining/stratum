use super::RpcError;
use tokio::task::JoinError;

// TODO this should be includede in JdsError

#[derive(Debug)]
pub enum JdsMempoolError {
    EmptyMempool,
    NoClient,
    Rpc(RpcError),
    PoisonLock(String),
    TokioJoinError(JoinError),
}

pub fn handle_error(error: JdsMempoolError) {
    match error {
        JdsMempoolError::EmptyMempool => println!("Empty mempool!"),
        JdsMempoolError::NoClient => println!("RPC Client not found"),
        JdsMempoolError::Rpc(a) => match a {
            RpcError::JsonRpc(m) => {
                let id = m.id;
                let error = m.error.unwrap();
                let code = error.code;
                let message = error.message;
                println!(
                    "RPC error: id {:?}, code {:?}, message {:?}",
                    id, code, message
                );
            }
            RpcError::Deserialization(e) => println!("Deserialization error: {:?}", e),
            RpcError::Serialization(e) => println!("Serialization error: {:?}", e),
            RpcError::Http(e) => println!("Http error: {:?}", e),
            RpcError::Other(e) => println!("Other error: {:?}", e),
        },
        JdsMempoolError::PoisonLock(e) => println!("Poison lock error: {:?}", e),
        JdsMempoolError::TokioJoinError(e) => println!("Tokio Join error: {:?}", e),
    }
}
