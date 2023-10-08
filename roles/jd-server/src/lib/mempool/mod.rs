pub mod hex_iterator;
pub mod rpc_client;
use binary_sv2::ShortTxId;
use bitcoin::blockdata::transaction::Transaction;
use stratum_common::bitcoin;
use bitcoin::hashes::HashEngine as HashEngineTrait;
use bitcoin::hashes::sha256::HashEngine as HashEngineStruct;
use bitcoin::hashes::sha256::Hash as HashStruct;
use bitcoin::hashes::Hash as HashTrait;
use bitcoin::hashes::sha256::Midstate;
use rpc_client::{Auth, RpcApi, RpcClient};
use serde::{Deserialize, Serialize};
use siphasher::sip::SipHasher24;
use std::{hash::Hasher, collections::hash_map::DefaultHasher};

#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Hash([u8; 32]);

#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Txid(Hash);

#[derive(Clone, Deserialize)]
pub struct Amount(usize);

#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlockHash(Hash);

struct TransacrtionWithHash {
    id: Txid,
    tx: Transaction,
}

fn get_profitability(tx_fee: (Transaction, Amount)) -> usize {
    let tx: Transaction = tx_fee.0;
    let size: usize = Transaction::size(&tx);
    let fee = tx_fee.1 .0;
    fee / size
}

//TODO make the function below work with get_profitability
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
pub struct JDsMempool {
    mempool: Vec<TransacrtionWithHash>,
    client: RpcClient,
}

impl JDsMempool {
    //TODO write this function that takes a short hash transaction id (the sip hash with 6 bytes
    //length) and a mempool in input and returns as output Some(Transaction) if the transaction is
    //present in the mempool and None otherwise.

    //fn verify_short_id<'a>(&self, tx_short_id: ShortTxId<'a>, tx_short_hash_nonce: u64) -> Option<&Transaction> {
    //    // hash the short hash nonce
    //    //let mut hasher = DefaultHasher::new();
    //    //let nonce_hash = HashEngineStruct::from(&tx_short_hash_nonce.to_le_bytes());
    //    let nonce_hash: HashStruct = HashTrait::hash(&tx_short_hash_nonce.to_le_bytes());
    //    // take first two integers from the hash
    //    let k0 = u64::from_le_bytes(nonce_hash[0..8]);
    //    let k1 = u64::from_le_bytes(nonce_hash[8..16]);
    //    let hasher = SipHasher24::new_with_keys(k0, k1);
    //    for transaction_with_hash in self.mempool.iter() {
    //        let tx_hashed = hasher.hash(&transaction_with_hash.id);
    //        let tx_hashed_bytes: Vec<u8> = transaction_with_hash.id.0.0.to_le_bytes().to_vec().drain(0..2).collect();
    //        let short_txid_mempool: ShortTxId = tx_hashed_bytes.try_into().unwrap();
    //        if short_txid_mempool == tx_short_id {
    //            return Some(&transaction_with_hash.tx);
    //        } else {
    //           continue;
    //        }
    //    }
    //    // ShortTxId doesn't match, need to ask JD client for this transaction
    //    None
    //}

    fn new(url: String, username: String, password: String) -> Self {
        let auth = Auth::UserPass(username, password);
        let rpc = RpcClient::new(&url, auth).unwrap();
        let empty_mempool: Vec<TransacrtionWithHash> = Vec::new();
        JDsMempool {
            mempool: empty_mempool,
            client: rpc,
        }
    }

    fn order_mempool_by_profitability(mut self) -> JDsMempool {
        self.mempool
            .sort_by(|a, b| b.tx.weight().cmp(&a.tx.weight()));
        self
    }

    fn update_mempool(self) -> Result<JDsMempool, JdsMempoolError> {
        let mut mempool_ordered: Vec<TransacrtionWithHash> = Vec::new();
        let client = self.client;
        let mempool = client.get_raw_mempool_verbose().unwrap();
        for txid in mempool.keys() {
            let transaction = client.get_raw_transaction(txid, None).unwrap();
            mempool_ordered.push(TransacrtionWithHash {
                id: txid.clone(),
                tx: transaction,
            });
        }

        if mempool_ordered.is_empty() {
            Err(JdsMempoolError::EmptyMempool)
        } else {
            Ok(JDsMempool {
                mempool: mempool_ordered,
                client,
            }
            .order_mempool_by_profitability())
        }
    }
}

pub enum JdsMempoolError {
    EmptyMempool,
}
