use binary_sv2::ShortTxId;
use bitcoin::{self, blockdata::transaction::Transaction, hashes::hex, secp256k1};
use jsonrpc::{error::Error as JsonRpcError, Client as JosnRpcClient};
use serde::Deserialize;
use std::{collections::HashMap, io};

struct TransacrtionWithHash<'decoder> {
    hash: ShortTxId<'decoder>,
    tx: Transaction,
}

//pub struct Txid(sha256d::Hash);
#[derive(Clone, Deserialize)]
pub struct Amount(u64);

#[derive(Deserialize)]
pub struct GetMempoolEntryResultFees {
    /// Transaction fee in BTC
    //#[serde(with = "bitcoin::amount::serde::as_btc")]
    pub base: Amount,
    /// Transaction fee with fee deltas used for mining priority in BTC
    //#[serde(with = "bitcoin::amount::serde::as_btc")]
    pub modified: Amount,
    /// Modified fees (see above) of in-mempool ancestors (including this one) in BTC
    //#[serde(with = "bitcoin::amount::serde::as_btc")]
    pub ancestor: Amount,
    /// Modified fees (see above) of in-mempool descendants (including this one) in BTC
    //#[serde(with = "bitcoin::amount::serde::as_btc")]
    pub descendant: Amount,
}

#[derive(Deserialize)]
pub struct GetMempoolEntryResult {
    /// Virtual transaction size as defined in BIP 141. This is different from actual serialized
    /// size for witness transactions as witness data is discounted.
    #[serde(alias = "size")]
    pub vsize: u64,
    /// Transaction weight as defined in BIP 141. Added in Core v0.19.0.
    pub weight: Option<u64>,
    /// Local time transaction entered pool in seconds since 1 Jan 1970 GMT
    pub time: u64,
    /// Block height when transaction entered pool
    pub height: u64,
    /// Number of in-mempool descendant transactions (including this one)
    #[serde(rename = "descendantcount")]
    pub descendant_count: u64,
    /// Virtual transaction size of in-mempool descendants (including this one)
    #[serde(rename = "descendantsize")]
    pub descendant_size: u64,
    /// Number of in-mempool ancestor transactions (including this one)
    #[serde(rename = "ancestorcount")]
    pub ancestor_count: u64,
    /// Virtual transaction size of in-mempool ancestors (including this one)
    #[serde(rename = "ancestorsize")]
    pub ancestor_size: u64,
    /// Hash of serialized transaction, including witness data
    /// before was pub wtxid: bitcoin::Txid,
    pub wtxid: Vec<u8>,
    //Fee information
    pub fees: GetMempoolEntryResultFees,
    /// Unconfirmed transactions used as inputs for this transaction
    /// before was pub depends: Vec<bitcoin::Txid>,
    pub depends: Vec<Vec<u8>>,
    /// Unconfirmed transactions spending outputs from this transaction
    /// before was pub spent_by: Vec<bitcoin::Txid>,
    #[serde(rename = "spentby")]
    pub spent_by: Vec<Vec<u8>>,
    /// Whether this transaction could be replaced due to BIP125 (replace-by-fee)
    #[serde(rename = "bip125-replaceable")]
    pub bip125_replaceable: bool,
    /// Whether this transaction is currently unbroadcast (initial broadcast not yet acknowledged by any peers)
    /// Added in Bitcoin Core v0.21
    pub unbroadcast: Option<bool>,
}

// the transaction in the mempool are
// oredered as fee/weight in descending order
pub struct JDsMempool<'decoder> {
    mempool: Vec<TransacrtionWithHash<'decoder>>,
}

impl<'decoder> JDsMempool<'decoder> {
    fn new() -> Self {
        JDsMempool {
            mempool: Vec::new(),
        }
    }
    fn verify_short_id<'a>(&self, tx_short_id: ShortTxId<'a>) -> Option<&Transaction> {
        for transaction_with_hash in self.mempool.iter() {
            if transaction_with_hash.hash == tx_short_id {
                return Some(&transaction_with_hash.tx);
            } else {
                continue;
            }
        }
        None
    }

    fn order_mempool_by_profitability(mut self) -> JDsMempool<'decoder> {
        self.mempool
            .sort_by(|a, b| b.tx.get_weight().cmp(&a.tx.get_weight()));
        self
    }

    //fn add_transaction_data(mut self, tx_short_id: ShortTxId<'decoder>, transaction: Transaction) -> JDsMempool<'decoder> {
    //    self.mempool.insert(tx_short_id, transaction).unwrap();
    //    self
    //}
    fn get_mempool() -> JDsMempool<'decoder> {
        let url = "http://127.0.0.1:18443".to_string();
        let username = "username".to_string();
        let password = "password".to_string();
        let auth = Auth::UserPass(username, password);
        let rpc = Client::new(&url, auth).unwrap();
        //let mempool = rpc.get_raw_mempool()?;
        //if mempool.is_empty() {
        //    println!("La mempool è vuota.");
        //} else {
        //    println!("Transazioni nella mempool:");
        //    for txid in mempool {
        //        println!("{}", txid);
        //    }
        //}
        let mempool_from_node: HashMap<bitcoin::Txid, GetMempoolEntryResult> = todo!();
        todo!()
    }
}

pub enum Auth {
    None,
    UserPass(String, String),
    //CookieFile(PathBuf),
}

pub struct Client {
    client: JosnRpcClient, //jsonrpc::client::Client,
}
impl Auth {
    /// Convert into the arguments that jsonrpc::Client needs.
    pub fn get_user_pass(self) -> (Option<String>, Option<String>) {
        //use std::io::Read;
        match self {
            Auth::None => (None, None),
            Auth::UserPass(u, p) => (Some(u), Some(p)),
            //Auth::CookieFile(path) => {
            //    let mut file = File::open(path)?;
            //    let mut contents = String::new();
            //    file.read_to_string(&mut contents)?;
            //    let mut split = contents.splitn(2, ":");
            //    Ok((
            //        Some(split.next().ok_or(Error::InvalidCookieFile)?.into()),
            //        Some(split.next().ok_or(Error::InvalidCookieFile)?.into()),
            //    ))
            //}
        }
    }
}

impl Client {
    /// Creates a client to a bitcoind JSON-RPC server.
    ///
    /// Can only return [Err] when using cookie authentication.
    pub fn new(url: &str, auth: Auth) -> Result<Self, BitcopinecoreRpcError> {
        let (user, pass) = auth.get_user_pass();
        jsonrpc::client::Client::simple_http(url, user, pass)
            .map(|client| Client { client })
            .map_err(|e| BitcopinecoreRpcError::JsonRpc(e.into()))
    }
}

/// The error type for errors produced in this library.
#[derive(Debug)]
pub enum BitcopinecoreRpcError {
    JsonRpc(jsonrpc::error::Error),
    Hex(hex::Error),
    Json(serde_json::error::Error),
    BitcoinSerialization(bitcoin::consensus::encode::Error),
    Secp256k1(secp256k1::Error),
    Io(io::Error),
    InvalidAmount(bitcoin::util::amount::ParseAmountError),
    InvalidCookieFile,
    /// The JSON result had an unexpected structure.
    UnexpectedStructure,
    /// The daemon returned an error string.
    ReturnedError(String),
}
