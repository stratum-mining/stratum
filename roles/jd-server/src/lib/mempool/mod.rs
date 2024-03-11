pub mod error;
use crate::mempool::error::JdsMempoolError;
use async_channel::Receiver;
use bitcoin::blockdata::transaction::Transaction;
use hashbrown::HashMap;
use roles_logic_sv2::utils::Mutex;
use rpc::mini_rpc_client;
use std::{convert::TryInto, sync::Arc};
use stratum_common::{bitcoin, bitcoin::hash_types::Txid};

#[derive(Clone, Debug)]
pub struct TransacrtionWithHash {
    id: Txid,
    tx: Transaction,
}

#[derive(Clone, Debug)]
pub struct JDsMempool {
    pub mempool: Vec<TransacrtionWithHash>,
    auth: mini_rpc_client::Auth,
    url: String,
    new_block_receiver: Receiver<String>,
}

impl JDsMempool {
    pub fn get_client(&self) -> Option<mini_rpc_client::MiniRpcClient> {
        let url = self.url.as_str();
        if url.contains("http") {
            let client = mini_rpc_client::MiniRpcClient::new(url.to_string(), self.auth.clone());
            Some(client)
        } else {
            None
        }
    }

    /// This function is used only for debug purposes and should not be used
    /// in production code.
    #[cfg(debug_assertions)]
    pub fn _get_transaction_list(self_: Arc<Mutex<Self>>) -> Vec<Txid> {
        let tx_list = self_.safe_lock(|x| x.mempool.clone()).unwrap();
        let tx_list_: Vec<Txid> = tx_list.iter().map(|n| n.id).collect();
        tx_list_
    }
    pub fn new(
        url: String,
        username: String,
        password: String,
        new_block_receiver: Receiver<String>,
    ) -> Self {
        let auth = mini_rpc_client::Auth::new(username, password);
        let empty_mempool: Vec<TransacrtionWithHash> = Vec::new();
        JDsMempool {
            mempool: empty_mempool,
            auth,
            url,
            new_block_receiver,
        }
    }

    pub async fn update_mempool(self_: Arc<Mutex<Self>>) -> Result<(), JdsMempoolError> {
        let mut mempool_ordered: Vec<TransacrtionWithHash> = Vec::new();
        let client = self_
            .safe_lock(|x| x.get_client())
            .map_err(|e| JdsMempoolError::PoisonLock(e.to_string()))?
            .ok_or(JdsMempoolError::NoClient)?;
        let mempool: Vec<String> = client
            .get_raw_mempool()
            .await
            .map_err(JdsMempoolError::Rpc)?;

        for id in &mempool {
            let tx: Result<Transaction, _> = client.get_raw_transaction(id, None).await;
            if let Ok(tx) = tx {
                let id = tx.txid();
                mempool_ordered.push(TransacrtionWithHash { id, tx });
            }
        }

        if mempool_ordered.is_empty() {
            return Err(JdsMempoolError::EmptyMempool);
        }

        self_
            .safe_lock(|x| {
                x.mempool = mempool_ordered;
            })
            .map_err(|e| JdsMempoolError::PoisonLock(e.to_string()))?;

        Ok(())
    }

    pub async fn on_submit(self_: Arc<Mutex<Self>>) -> Result<(), JdsMempoolError> {
        let new_block_receiver: Receiver<String> = self_
            .safe_lock(|x| x.new_block_receiver.clone())
            .map_err(|e| JdsMempoolError::PoisonLock(e.to_string()))?;
        let client = self_
            .safe_lock(|x| x.get_client())
            .map_err(|e| JdsMempoolError::PoisonLock(e.to_string()))?
            .ok_or(JdsMempoolError::NoClient)?;

        while let Ok(block_hex) = new_block_receiver.recv().await {
            match mini_rpc_client::MiniRpcClient::submit_block(&client, block_hex).await {
                Ok(_) => return Ok(()),
                Err(e) => JdsMempoolError::Rpc(e),
            };
        }
        Ok(())
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
