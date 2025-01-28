pub mod error;
use super::job_declarator::AddTrasactionsToMempoolInner;
use crate::mempool::error::JdsMempoolError;
use bitcoin::blockdata::transaction::Transaction;
use hashbrown::HashMap;
use roles_logic_sv2::utils::Mutex;
use rpc_sv2::{mini_rpc_client, mini_rpc_client::RpcError};
use std::{convert::TryInto, str::FromStr, sync::Arc};
use stratum_common::{bitcoin, bitcoin::hash_types::Txid};

#[derive(Clone, Debug)]
pub struct TransactionWithHash {
    pub id: Txid,
    pub tx: Option<(Transaction, u32)>,
}

#[derive(Clone, Debug)]
pub struct JDsMempool {
    pub mempool: HashMap<Txid, Option<(Transaction, u32)>>,
    auth: mini_rpc_client::Auth,
    url: String,
    new_block_sender: tokio::sync::broadcast::Sender<String>,
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
        let tx_list_: Vec<Txid> = tx_list.iter().map(|n| *n.0).collect();
        tx_list_
    }

    pub fn new(
        url: String,
        username: String,
        password: String,
        new_block_sender: tokio::sync::broadcast::Sender<String>,
    ) -> Self {
        let auth = mini_rpc_client::Auth::new(username, password);
        let empty_mempool: HashMap<Txid, Option<(Transaction, u32)>> = HashMap::new();
        JDsMempool {
            mempool: empty_mempool,
            auth,
            url,
            new_block_sender,
        }
    }

    // this functions fill in the mempool the transactions with the given txid and insert the given
    // transactions. The ids are for the transactions that are already known to the node, the
    // unknown transactions are provided directly as a vector
    pub async fn add_tx_data_to_mempool(
        self_: Arc<Mutex<Self>>,
        add_txs_to_mempool_inner: AddTrasactionsToMempoolInner,
    ) -> Result<(), JdsMempoolError> {
        let txids = add_txs_to_mempool_inner.known_transactions;
        let transactions = add_txs_to_mempool_inner.unknown_transactions;
        let client = self_
            .safe_lock(|a| a.get_client())?
            .ok_or(JdsMempoolError::NoClient)?;
        // fill in the mempool the transactions id in the mempool with the full transactions
        // retrieved from the jd client
        for txid in txids {
            if let Some(None) = self_
                .safe_lock(|a| a.mempool.get(&txid).cloned())
                .map_err(|e| JdsMempoolError::PoisonLock(e.to_string()))?
            {
                let transaction = client
                    .get_raw_transaction(&txid.to_string(), None)
                    .await
                    .map_err(JdsMempoolError::Rpc)?;
                let _ = self_.safe_lock(|a| {
                    a.mempool
                        .entry(transaction.txid())
                        .and_modify(|entry| {
                            if let Some((_, count)) = entry {
                                *count += 1;
                            } else {
                                *entry = Some((transaction.clone(), 1));
                            }
                        })
                        .or_insert(Some((transaction, 1)));
                });
            }
        }

        // fill in the mempool the transactions given in input
        for transaction in transactions {
            let _ = self_.safe_lock(|a| {
                a.mempool
                    .entry(transaction.txid())
                    .and_modify(|entry| {
                        if let Some((_, count)) = entry {
                            *count += 1;
                        } else {
                            *entry = Some((transaction.clone(), 1));
                        }
                    })
                    .or_insert(Some((transaction, 1)));
            });
        }
        Ok(())
    }

    pub async fn update_mempool(self_: Arc<Mutex<Self>>) -> Result<(), JdsMempoolError> {
        let client = self_
            .safe_lock(|x| x.get_client())?
            .ok_or(JdsMempoolError::NoClient)?;

        let mempool = client.get_raw_mempool().await?;

        let raw_mempool_txids: Result<Vec<Txid>, _> = mempool
            .into_iter()
            .map(|id| {
                Txid::from_str(&id)
                    .map_err(|err| JdsMempoolError::Rpc(RpcError::Deserialization(err.to_string())))
            })
            .collect();

        let raw_mempool_txids = raw_mempool_txids?;

        // Holding the lock till the light mempool updation is complete.
        let is_mempool_empty = self_.safe_lock(|x| {
            raw_mempool_txids.iter().for_each(|txid| {
                x.mempool.entry(*txid).or_insert(None);
            });
            x.mempool.is_empty()
        })?;

        if is_mempool_empty {
            Err(JdsMempoolError::EmptyMempool)
        } else {
            Ok(())
        }
    }

    pub async fn on_submit(self_: Arc<Mutex<Self>>) -> Result<(), JdsMempoolError> {
        let new_block_sender: tokio::sync::broadcast::Sender<String> =
            self_.safe_lock(|x| x.new_block_sender.clone())?;
        let mut new_block_receiver = new_block_sender.subscribe();
        let client = self_
            .safe_lock(|x| x.get_client())?
            .ok_or(JdsMempoolError::NoClient)?;

        while let Ok(block_hex) = new_block_receiver.recv().await {
            match mini_rpc_client::MiniRpcClient::submit_block(&client, block_hex).await {
                Ok(_) => return Ok(()),
                Err(e) => JdsMempoolError::Rpc(e),
            };
        }
        Ok(())
    }

    pub fn to_short_ids(&self, nonce: u64) -> Option<HashMap<[u8; 6], TransactionWithHash>> {
        let mut ret = HashMap::new();
        for tx in &self.mempool {
            let s_id = roles_logic_sv2::utils::get_short_hash(*tx.0, nonce)
                .to_vec()
                .try_into()
                .unwrap();
            let tx_data = TransactionWithHash {
                id: *tx.0,
                tx: tx.1.clone(),
            };
            if ret.insert(s_id, tx_data.clone()).is_none() {
                continue;
            } else {
                return None;
            }
        }
        Some(ret)
    }
}
