pub mod mini_rpc_client;
//pub mod rpc_client;
//pub mod hex_iterator;
use async_channel::Receiver;
use bitcoin::blockdata::transaction::Transaction;
use hashbrown::HashMap;
use roles_logic_sv2::utils::Mutex;
//use rpc_client::{Auth, RpcApi, RpcClient};
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
    auth: mini_rpc_client::Auth,
    url: String,
    receiver: Receiver<String>,
}

impl JDsMempool {
    pub fn get_client(&self) -> Option<mini_rpc_client::MiniRpcClient> {
        let url = self.url.as_str();
        if url.contains("http") {
            let client = mini_rpc_client::MiniRpcClient::new(
                url.to_string(),
                self.auth.clone(),
                self.receiver.clone(),
            );
            //Some(RpcClient::new(url, self.auth.clone()).unwrap())
            Some(client)
        } else {
            None
        }
    }
    pub fn new(
        url: String,
        username: String,
        password: String,
        receiver: Receiver<String>,
    ) -> Self {
        let auth = mini_rpc_client::Auth::new(username, password);
        let empty_mempool: Vec<TransacrtionWithHash> = Vec::new();
        JDsMempool {
            mempool: empty_mempool,
            auth,
            url,
            receiver,
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
                let mempool: Vec<String> = client.get_raw_mempool_verbose().await.unwrap();
                for id in &mempool {
                    let tx: Result<Transaction, _> = client.get_raw_transaction(id, None).await;
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

    //
    //pub async fn on_submit(self_: Arc<Mutex<Self>>) {
    //    let recv = self_.safe_lock(|x| x.recv_submit);
    pub async fn on_submit(self_: Arc<Mutex<Self>>) {
        let receiver = self_.safe_lock(|x| x.get_client().unwrap()).unwrap().get_receiver();
        let client = self_.safe_lock(|x| x.get_client()).unwrap().unwrap();

        while let Ok(block_hex) = receiver.recv().await {
            mini_rpc_client::MiniRpcClient::submit_block(&client, block_hex).await;
            
             

            //TODO: implement logic for success or error
            //let (last_declare, mut tx_list, _) = match self_.safe_lock(|x| x.declared_mining_job.take()).unwrap() {
            //    Some((last_declare, tx_list, _x)) => (last_declare, tx_list, _x),
            //    None => {
            //        warn!("Received solution but no job available");
            //        return Ok(SendTo::None(None));
            //    }
            //};
            //let coinbase_pre = last_declare.coinbase_prefix.to_vec();
            //let extranonce = message.extranonce.to_vec();
            //let coinbase_suf = last_declare.coinbase_suffix.to_vec();
            //let mut path: Vec<Vec<u8>> = vec![];
            //for tx in &tx_list {
            //    let id = tx.txid();
            //    let id = id.as_ref().to_vec();
            //    path.push(id);
            //}
            //let merkle_root =
            //    merkle_root_from_path(&coinbase_pre[..], &coinbase_suf[..], &extranonce[..], &path)
            //        .expect("Invalid coinbase");
            //let merkle_root = Hash::from_inner(merkle_root.try_into().unwrap());

            //let prev_blockhash = u256_to_block_hash(message.prev_hash.into_static());
            //let header = stratum_common::bitcoin::blockdata::block::BlockHeader {
            //    version: last_declare.version as i32,
            //    prev_blockhash,
            //    merkle_root,
            //    time: message.ntime,
            //    bits: message.nbits,
            //    nonce: message.nonce,
            //};

            //let coinbase = [coinbase_pre, extranonce, coinbase_suf].concat();
            //let coinbase = Transaction::deserialize(&coinbase[..]).unwrap();
            //tx_list.insert(0, coinbase);

            //let mut block = Block {
            //    header,
            //    txdata: tx_list.clone(),
            //};

            //block.header.merkle_root = block.compute_merkle_root().unwrap();

            //let serialized_block = serialize(&block);
            //let hexdata = hex::encode(serialized_block);

            //// TODO This line blok everything!!
            //let client = self_.safe_lock(|y|
            //    y
            //    .mempool
            //    .safe_lock(|x| {
            //        if let Some(client) = x.get_client() {
            //            client//.submit_block(hexdata).await;
            //        } else {
            //            todo!()
            //        }
            //    })
            //    .unwrap()).unwrap();
            //client.submit_block(hexdata).await;
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
