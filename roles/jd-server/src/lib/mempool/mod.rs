pub mod hex_iterator;
pub mod rpc_client;
use bitcoin::blockdata::transaction::Transaction;
use hashbrown::HashMap;
use roles_logic_sv2::utils::Mutex;
use rpc_client::{Auth, RpcApi, RpcClient};
use serde::{Deserialize, Serialize};
use std::{convert::TryInto, sync::Arc};
use stratum_common::{bitcoin, bitcoin::hash_types::Txid};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Hash([u8; 32]);

#[derive(Clone, Deserialize)]
pub struct Amount(f64);

#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlockHash(Hash);

#[derive(Clone, Debug)]
pub struct TransacrtionWithHash {
    id: Txid,
    tx: Transaction,
}

#[derive(Clone, Debug)]
pub struct JDsMempool {
    pub mempool: Vec<TransacrtionWithHash>,
    auth: Auth,
    url: String,
}

impl JDsMempool {
    pub fn get_client(&self) -> Option<RpcClient> {
        let url = self.url.as_str();
        if url.contains("http") {
            Some(RpcClient::new(url, self.auth.clone()).unwrap())
        } else {
            None
        }
    }
    pub fn new(url: String, username: String, password: String) -> Self {
        let auth = Auth::UserPass(username, password);
        let empty_mempool: Vec<TransacrtionWithHash> = Vec::new();
        JDsMempool {
            mempool: empty_mempool,
            auth,
            url,
        }
    }

    pub async fn update_mempool(self_: Arc<Mutex<Self>>) -> Result<(), JdsMempoolError> {
        let mut mempool_ordered: Vec<TransacrtionWithHash> = Vec::new();
        let client = self_
            .safe_lock(|x| x.get_client())
            .unwrap()
            .ok_or(JdsMempoolError::NoClient)?;
        let new_mempool: Result<Vec<TransacrtionWithHash>, JdsMempoolError> =
            tokio::task::spawn(async move {
                let mempool: Vec<String> = client.get_raw_mempool_verbose().unwrap();
                for id in &mempool {
                    let tx: Result<Transaction, _> = client.get_raw_transaction(id, None);
                    if let Ok(tx) = tx {
                        let id = tx.txid();
                        mempool_ordered.push(TransacrtionWithHash { id, tx });
                    }
                }
                if mempool_ordered.is_empty() {
                    Err(JdsMempoolError::EmptyMempool)
                } else {
                    Ok(mempool_ordered)
                }
            })
            .await
            .unwrap();

        match new_mempool {
            Ok(new_mempool_) => {
                let _ = self_.safe_lock(|x| {
                    x.mempool = new_mempool_;
                });
                Ok(())
            }
            Err(a) => Err(a),
        }
    }

    pub fn to_short_ids(&self, nonce: u64) -> Option<HashMap<[u8; 6], Transaction>> {
        let mut ret = HashMap::new();
        for tx in &self.mempool {
            let s_id = roles_logic_sv2::utils::get_short_hash(tx.id, nonce)
                .to_vec()
                .try_into()
                .unwrap();
            if ret.insert(s_id, tx.tx.clone()).is_none() {
                continue;
            } else {
                return None;
            }
        }
        Some(ret)
    }
}

#[derive(Debug)]
pub enum JdsMempoolError {
    EmptyMempool,
    NoClient,
}
