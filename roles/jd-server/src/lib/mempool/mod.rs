pub mod hex_iterator;
pub mod rpc_client;
use binary_sv2::ShortTxId;
use bitcoin::blockdata::transaction::Transaction;
use hashbrown::HashMap;
use roles_logic_sv2::utils::Mutex;
use rpc_client::{Auth, GetMempoolEntryResult, RpcApi, RpcClient};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use stratum_common::{
    bitcoin,
    bitcoin::{consensus::encode::deserialize, hash_types::Txid, hashes::hex::FromHex},
};

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
    fn get_client(&self) -> RpcClient {
        let url = self.url.as_str();
        RpcClient::new(url, self.auth.clone()).unwrap()
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
        let client = self_.safe_lock(|x| x.get_client()).unwrap();
        let new_mempool: Result<Vec<TransacrtionWithHash>, JdsMempoolError> =
            tokio::task::spawn(async move {
                let mempool: HashMap<String, GetMempoolEntryResult> =
                    client.get_raw_mempool_verbose().unwrap();
                for id in mempool.keys() {
                    let tx: Transaction = client.get_raw_transaction(id, None).unwrap();
                    let id = Vec::from_hex(id).expect("Invalid hex string");
                    let id: Txid = deserialize(&id).expect("Failed to deserialize txid");
                    mempool_ordered.push(TransacrtionWithHash { id, tx });
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

    pub fn verify_short_id(
        &self,
        tx_short_id: &ShortTxId<'_>,
        nonce: u64,
    ) -> Option<(bitcoin::Txid, bitcoin::Transaction)> {
        let mempool: Vec<TransacrtionWithHash> = self.clone().mempool;
        for tx_with_hash in mempool {
            let btc_txid = tx_with_hash.id;
            if &roles_logic_sv2::utils::get_short_hash(btc_txid, nonce) == tx_short_id {
                return Some((btc_txid, tx_with_hash.tx));
            } else {
                continue;
            }
        }
        None
    }
}

#[derive(Debug)]
pub enum JdsMempoolError {
    EmptyMempool,
}
