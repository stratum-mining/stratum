pub mod hex_iterator;
pub mod rpc_client;
use binary_sv2::ShortTxId;
use bitcoin::blockdata::transaction::Transaction;
use hashbrown::HashMap;
use roles_logic_sv2::utils::Mutex;
use rpc_client::{Auth, GetMempoolEntryResult, RpcApi, RpcClient};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use stratum_common::bitcoin;
use stratum_common::bitcoin::hash_types::Txid;
use stratum_common::bitcoin::consensus::encode::deserialize;
use stratum_common::bitcoin::hashes::hex::FromHex;

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

// TODO if we want to order transactions in memepool by profitability (i.e. fees/weight) we must
// use this function
//fn get_profitability(tx_fee: (Transaction, Amount)) -> usize {
//    let tx: Transaction = tx_fee.0;
//    let size: usize = Transaction::size(&tx);
//    let fee = tx_fee.1 .0;
//    fee / size
//}

// TODO make the function below work with get_profitability
//fn order_mempool_by_fee_over_size(mut vector: Vec<(Transaction, Amount)>) -> Vec<Transaction> {
//    vector.sort_by(|a, b| b.get_profitability().cmp(&a.get_profitability()));
//    vector
//}

// TODO! (the following is relative to the crate bitcoincore_rpc)
// in the message GetRawTransactionVerbose we get a an hashmap<Txid,
// GetMempoolEntryResult>. I realized later that in the struc GetMempoolEntryResult there
// isn't the transaction itself, but rather the data of relative to ancestors, descendnts,
// replacability, weight and so on. I don't know if there is a rpc request to get all the
// whole transactions in the mempool, no just the IDs. I had a brief look at it, and seems
// that it was't present such a message. In this case, we must ask to the node the
// full transaction data for every transaction id and include the message GetRawTransaction
// (that is already present below as commented). Furthermore, we don't need all the data of
// GetRawMempoolVerbose, the message GetRawMempool is enough. So, summarizing, we must
// 1. check if an rpc request that retrieves the mempool, with all the transactions data
//    and not just the Txid + genealogy of txid
// 2. if the message in 1. is not present, we must
//    2.1 change the message from GetRawMempoolVerbose to GetRawMempool
//    2.2 (DONE) uncomment GetRawTransaction message below (maked with a TODO) and make this
//      compile
// 3. work on the TODO above, about the function verify_shor_id (this task is already
//    assigned to 4ss0.
// 4. (DONE) rebase (assigned to 4ss0)
// 5. the method order_mempool_by_profitability is misleading, because actially it order by
//    size. Correct and use the function above order_mempool_by_fee_over_size
//
// SORRY: the code should has beed divided in different files and modules. Sorry for this.
//
//

// NOTE the transaction in the mempool are
// NOTE oredered as fee/weight in descending order
// NOTE
#[derive(Clone, Debug)]
pub struct JDsMempool {
    pub mempool: Vec<TransacrtionWithHash>,
    auth: Auth,
    url: String,
}

impl JDsMempool {
    //TODO write this function that takes a short hash transaction id (the sip hash with 6 bytes
    //length) and a mempool in input and returns as output Some(Transaction) if the transaction is
    //present in the mempool and None otherwise.

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

    //fn order_mempool_by_profitability(mut self) -> JDsMempool {
    //    self.mempool
    //        .sort_by(|a, b| b.tx.weight().cmp(&a.tx.weight()));
    //    self
    //}

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
        tx_short_id: ShortTxId<'_>,
        nonce: u64,
    ) -> Option<(bitcoin::Txid, bitcoin::Transaction)> {
        let mempool: Vec<TransacrtionWithHash> = self.clone().mempool;
        for tx_with_hash in mempool {
            let btc_txid = tx_with_hash.id;
            if roles_logic_sv2::utils::get_short_hash(btc_txid, nonce) == tx_short_id {
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
