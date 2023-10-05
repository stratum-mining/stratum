use binary_sv2::ShortTxId;
use stratum_common::bitcoin as bitcoin;
use bitcoin::{
    blockdata::transaction::Transaction,
    hashes::hex::{self, HexIterator},
    secp256k1,
};
//use bitcoin::hashes::Hash;
use bitcoin::consensus::Decodable;
use hashbrown::hash_map::HashMap;
use jsonrpc::{error::Error as JsonRpcError, Client as JosnRpcClient};
use serde::{Deserialize, Serialize, Serializer};
use std::{convert::TryInto, io};

#[derive(PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Hash([u8; 32]);

#[derive(PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Txid(Hash);

#[derive(PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlockHash(Hash);

//impl Serialize for bitcoin::Txid {
//    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
//    where
//        S: Serializer,
//    {
//        serializer.serialize_str(&self.to_string())
//    }
//}

//pub struct TxidWrapper {
//    txid: bitcoin::Txid,
//}
//
//impl Serialize for TxidWrapper {
//    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
//    where
//        S: Serializer,
//    {
//        serializer.serialize_str(&self.0.to_string())
//    }
//}
//impl From<bitcoin::hash_types::Txid> for Txid {
//    fn from(value: bitcoin::hash_types::Txid) -> Self {
//        Txid(value.into_inner())//to_vec().try_into().unwrap_or_else(|v: Vec<u8>| panic!("Expected a Vec of length {} but it was {}", 4, v.len()))
//    }
//}

struct TransacrtionWithHash {
    id: Txid,
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
pub struct JDsMempool {
    mempool: Vec<TransacrtionWithHash>,
}

impl JDsMempool {
    fn new() -> Self {
        JDsMempool {
            mempool: Vec::new(),
        }
    }
    //TODO write this function that takes a short hash transaction id (the sip hash with 6 bytes
    //length) and a mempool in input and returns as output Some(Transaction) if the transaction is
    //present in the mempool and None otherwise.
    fn verify_short_id<'a>(&self, tx_short_id: ShortTxId<'a>) -> Option<&Transaction> {
        //for transaction_with_hash in self.mempool.iter() {
        //    if transaction_with_hash.id == tx_short_id {
        //        return Some(&transaction_with_hash.tx);
        //    } else {
        //        continue;
        //    }
        //}
        //None
        todo!()
    }

    fn order_mempool_by_profitability(mut self) -> JDsMempool {
        self.mempool
            .sort_by(|a, b| b.tx.get_weight().cmp(&a.tx.get_weight()));
        self
    }

    //fn add_transaction_data(mut self, tx_short_id: ShortTxId<'decoder>, transaction: Transaction) -> JDsMempool<'decoder> {
    //    self.mempool.insert(tx_short_id, transaction).unwrap();
    //    self
    //}
    fn get_mempool() -> Result<JDsMempool, JdsMempoolError> {
        let mut mempool_ordered = JDsMempool::new();
        let url = "http://127.0.0.1:18443".to_string();
        let username = "username".to_string();
        let password = "password".to_string();
        let auth = Auth::UserPass(username, password);
        let rpc = Client::new(&url, auth).unwrap();
        let mempool = rpc.get_raw_mempool_verbose().unwrap();
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
        //    2.2 uncomment GetRawTransaction message below (maked with a TODO) and make this
        //      compile
        // 3. work on the TODO above, about the function verify_shor_id (this task is already
        //    assigned to 4ss0.
        // 4. rebase (assigned to 4ss0)
        //
        // SORRY: the code should has beed divided in different files and modules. Sorry for this.
        //
        //
        //for key in mempool.keys() {
        //    let transaction: bitcoin::Transaction = rpc.gettr
        //    let transaction = mempool.get(key).clone();
        //    mempool_ordered.mempool.push(TransacrtionWithHash {id: key.into(), tx: transaction.unwrap()});
        //};
        ////if mempool.is_empty() {
        ////    return Err(JdsMempoolError::EmptyMempool)
        ////} else {
        ////    for key in mempool.keys() {
        ////        let transaction = mempool.get(key).clone();
        ////        mempool_ordered.mempool.push(TransacrtionWithHash { id: key, tx: transaction })
        ////    }
        ////};
        //mempool_ordered.order_mempool_by_profitability();
        ////let mempool_from_node: HashMap<bitcoin::Txid, GetMempoolEntryResult> = todo!();
        ////CONVERT MEMPOOL FROM HASHMAP TO VECTOR
        todo!()
    }
}

enum JdsMempoolError {
    EmptyMempool,
}

pub enum Auth {
    None,
    UserPass(String, String),
    //CookieFile(PathBuf),
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
pub struct Client {
    client: JosnRpcClient, //jsonrpc::client::Client,
}

impl Client {
    /// Creates a client to a bitcoind JSON-RPC server.
    ///
    /// Can only return [Err] when using cookie authentication.
    pub fn new(url: &str, auth: Auth) -> Result<Self, BitcoincoreRpcError> {
        let (user, pass) = auth.get_user_pass();
        jsonrpc::client::Client::simple_http(url, user, pass)
            .map(|client| Client { client })
            .map_err(|e| BitcoincoreRpcError::JsonRpc(e.into()))
    }
}

pub trait RpcApi: Sized {
    /// Call a `cmd` rpc with given `args` list
    fn call<T: for<'a> serde::de::Deserialize<'a>>(
        &self,
        cmd: &str,
        args: &[serde_json::Value],
    ) -> Result<T, BitcoincoreRpcError>;

    /// Get details for the transactions in a memory pool
    fn get_raw_mempool_verbose(
        &self,
    ) -> Result<HashMap<Txid, GetMempoolEntryResult>, BitcoincoreRpcError> {
        self.call("getrawmempool", &[serde_json::to_value(true).unwrap()])
    }

    //TODO! uncomment this and make it work!
    //fn get_raw_transaction(
    //    &self,
    //    txid: &Txid,
    //    block_hash: Option<&BlockHash>,
    //) -> Result<Transaction, JsonRpcError> {
    //    let mut args = [into_json(txid)?, into_json(false)?, opt_into_json(block_hash)?];
    //    let hex: String = self.call("getrawtransaction", handle_defaults(&mut args, &[serde_json::Value::Null])).unwrap();
    //    let mut reader = HexIterator::new(&hex).unwrap();
    //    let object = Decodable::consensus_decode(&mut reader)?;
    //    Ok(object)
    //}
}

/// Shorthand for converting a variable into a serde_json::Value.
fn into_json<T>(val: T) -> Result<serde_json::Value, JsonRpcError>
where
    T: serde::ser::Serialize,
{
    Ok(serde_json::to_value(val)?)
}

/// Shorthand for converting an Option into an Option<serde_json::Value>.
fn opt_into_json<T>(opt: Option<T>) -> Result<serde_json::Value, JsonRpcError>
where
    T: serde::ser::Serialize,
{
    match opt {
        Some(val) => Ok(into_json(val)?),
        None => Ok(serde_json::Value::Null),
    }
}

impl RpcApi for Client {
    /// Call an `cmd` rpc with given `args` list
    fn call<T: for<'a> serde::de::Deserialize<'a>>(
        &self,
        cmd: &str,
        args: &[serde_json::Value],
    ) -> RResult<T> {
        let raw_args: Vec<_> = args
            .iter()
            .map(|a| {
                let json_string = serde_json::to_string(a)?;
                serde_json::value::RawValue::from_string(json_string) // we can't use to_raw_value here due to compat with Rust 1.29
            })
            .map(|a| a.map_err(|e| BitcoincoreRpcError::Json(e)))
            .collect::<RResult<Vec<_>>>()?;
        let req = self.client.build_request(&cmd, &raw_args);
        //if log_enabled!(Debug) {
        //    debug!(target: "bitcoincore_rpc", "JSON-RPC request: {} {}", cmd, serde_json::Value::from(args));
        //}

        let resp = self.client.send_request(req).map_err(JsonRpcError::from);
        //log_response(cmd, &resp);
        Ok(resp?.result()?)
    }
}

pub type RResult<T> = Result<T, BitcoincoreRpcError>;

/// The error type for errors produced in this library.
#[derive(Debug)]
pub enum BitcoincoreRpcError {
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

impl From<jsonrpc::error::Error> for BitcoincoreRpcError {
    fn from(e: jsonrpc::error::Error) -> BitcoincoreRpcError {
        BitcoincoreRpcError::JsonRpc(e)
    }
}

/// Handle default values in the argument list
///
/// Substitute `Value::Null`s with corresponding values from `defaults` table,
/// except when they are trailing, in which case just skip them altogether
/// in returned list.
///
/// Note, that `defaults` corresponds to the last elements of `args`.
///
/// ```norust
/// arg1 arg2 arg3 arg4
///           def1 def2
/// ```
///
/// Elements of `args` without corresponding `defaults` value, won't
/// be substituted, because they are required.
fn handle_defaults<'a, 'b>(
    args: &'a mut [serde_json::Value],
    defaults: &'b [serde_json::Value],
) -> &'a [serde_json::Value] {
    assert!(args.len() >= defaults.len());

    // Pass over the optional arguments in backwards order, filling in defaults after the first
    // non-null optional argument has been observed.
    let mut first_non_null_optional_idx = None;
    for i in 0..defaults.len() {
        let args_i = args.len() - 1 - i;
        let defaults_i = defaults.len() - 1 - i;
        if args[args_i] == serde_json::Value::Null {
            if first_non_null_optional_idx.is_some() {
                if defaults[defaults_i] == serde_json::Value::Null {
                    panic!("Missing `default` for argument idx {}", args_i);
                }
                args[args_i] = defaults[defaults_i].clone();
            }
        } else if first_non_null_optional_idx.is_none() {
            first_non_null_optional_idx = Some(args_i);
        }
    }

    let required_num = args.len() - defaults.len();

    if let Some(i) = first_non_null_optional_idx {
        &args[..i + 1]
    } else {
        &args[..required_num]
    }
}
