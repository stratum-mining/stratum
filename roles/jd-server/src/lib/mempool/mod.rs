//! ## Mempool Management for the Job Declarator Server (JDS)
//!
//! This module defines the internal mempool of the JDS, responsible for keeping track of known
//! transactions and interacting with the Bitcoin node via RPC.
//!
//! Its core responsibilities are:
//! - Keeping a local copy of txids and (optionally) their full transaction data
//! - Pulling known transactions from the Bitcoin node on demand (via `getrawtransaction`)
//! - Accepting and tracking raw transactions received from clients
//! - Forwarding valid blocks to the Bitcoin node via `submitblock`
//!
//! Internally, `JDsMempool` uses a `HashMap<Txid, Option<(Transaction, u32)>>`:
//! - `None`: transaction only known by ID, data is missing
//! - `Some`: full transaction is known, `u32` is a reference counter for eviction
//!
//! Most methods are `Arc<Mutex<_>>`-wrapped and should be reviewed for locking efficiency.

pub mod error;
use super::job_declarator::AddTrasactionsToMempoolInner;
use crate::mempool::error::JdsMempoolError;
use async_channel::Receiver;
use hashbrown::HashMap;
use rpc_sv2::{mini_rpc_client, mini_rpc_client::RpcError};
use std::{str::FromStr, sync::Arc};
use stratum_common::roles_logic_sv2::{
    bitcoin::{blockdata::transaction::Transaction, hash_types::Txid},
    utils::Mutex,
};

/// Wrapper around a known transaction and its hash.
#[derive(Clone, Debug)]
pub struct TransactionWithHash {
    pub id: Txid,
    pub tx: Option<(Transaction, u32)>, // Full data and ref count
}

/// Internal representation of the JDS mempool.
#[derive(Clone, Debug)]
pub struct JDsMempool {
    /// Local map of known txids and their associated data (if available).
    pub mempool: HashMap<Txid, Option<(Transaction, u32)>>,
    /// Auth for RPC connection to the node.
    auth: mini_rpc_client::Auth,
    /// URI of the Bitcoin node.
    url: rpc_sv2::Uri,
    /// Receiver for new block solutions coming from JDC.
    new_block_receiver: Receiver<String>,
}

impl JDsMempool {
    /// Returns a MiniRpcClient if the URL looks valid.
    pub fn get_client(&self) -> Option<mini_rpc_client::MiniRpcClient> {
        let url = self.url.to_string();
        if url.contains("http") {
            let client = mini_rpc_client::MiniRpcClient::new(self.url.clone(), self.auth.clone());
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

    /// Instantiates a new empty mempool for JDS.
    pub fn new(
        url: rpc_sv2::Uri,
        username: String,
        password: String,
        new_block_receiver: Receiver<String>,
    ) -> Self {
        let auth = mini_rpc_client::Auth::new(username, password);
        let empty_mempool: HashMap<Txid, Option<(Transaction, u32)>> = HashMap::new();
        JDsMempool {
            mempool: empty_mempool,
            auth,
            url,
            new_block_receiver,
        }
    }

    /// Simple RPC ping to verify connection to Bitcoin node.
    pub async fn health(self_: Arc<Mutex<Self>>) -> Result<(), JdsMempoolError> {
        let client = self_
            .safe_lock(|a| a.get_client())?
            .ok_or(JdsMempoolError::NoClient)?;
        client.health().await.map_err(JdsMempoolError::Rpc)
    }

    /// Inserts transactions into the mempool:
    /// - known txids are fetched from the Bitcoin node
    /// - unknown txs are directly inserted
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
                        .entry(transaction.compute_txid())
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
                    .entry(transaction.compute_txid())
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

    /// Periodically synchronizes the mempool with the Bitcoin node.
    /// This only inserts thin entries (`None` as value), not full transactions.
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

    /// Listens for block submissions (hex-encoded) and propagates them to the Bitcoin node.
    pub async fn on_submit(self_: Arc<Mutex<Self>>) -> Result<(), JdsMempoolError> {
        let new_block_receiver: Receiver<String> =
            self_.safe_lock(|x| x.new_block_receiver.clone())?;
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
}
