use crate::lib::mempool::{hex_iterator::HexIterator, Amount, BlockHash};
use bitcoin::{blockdata::transaction::Transaction, consensus::Decodable};
use jsonrpc::{error::Error as JsonRpcError, Client as JosnRpcClient};
use serde::Deserialize;
use stratum_common::bitcoin;

#[derive(Clone, Debug)]
pub enum Auth {
    //None,
    UserPass(String, String),
    //CookieFile(PathBuf),
}

impl Auth {
    pub fn get_user_pass(self) -> (Option<String>, Option<String>) {
        match self {
            Auth::UserPass(u, p) => (Some(u), Some(p)),
        }
    }
}

pub struct RpcClient {
    client: JosnRpcClient, //jsonrpc::client::Client,
}

impl RpcClient {
    pub fn new(url: &str, auth: Auth) -> Result<Self, BitcoincoreRpcError> {
        let (user, pass) = auth.get_user_pass();
        jsonrpc::client::Client::simple_http(url, user, pass)
            .map(|client| RpcClient { client })
            .map_err(|e| BitcoincoreRpcError::JsonRpc(e.into()))
    }
    pub fn submit_block(
        &self,
        submit_block: String,
    ) -> Result<Option<String>, BitcoincoreRpcError> {
        self.call(
            "submitblock",
            &[serde_json::to_value(submit_block).unwrap()],
        )
    }
}

pub trait RpcApi: Sized {
    /// Call a `cmd` rpc with given `args` list
    fn call<T: for<'a> serde::de::Deserialize<'a>>(
        &self,
        cmd: &str,
        args: &[serde_json::Value],
    ) -> Result<T, BitcoincoreRpcError>;

    fn get_raw_mempool_verbose(&self) -> Result<Vec<String>, BitcoincoreRpcError> {
        self.call("getrawmempool", &[])
    }

    fn get_raw_transaction(
        &self,
        txid: &String,
        block_hash: Option<&BlockHash>,
    ) -> Result<Transaction, JsonRpcError> {
        let mut args = [
            into_json(txid)?,
            into_json(false)?,
            opt_into_json(block_hash)?,
        ];
        let hex: String = self
            .call(
                "getrawtransaction",
                handle_defaults(&mut args, &[serde_json::Value::Null]),
            )
            .map_err(|_| JsonRpcError::EmptyBatch)?;
        let mut reader =
            HexIterator::new(&hex).unwrap_or_else(|_| panic!("Can not decode hex  {}", hex));
        let object = Decodable::consensus_decode(&mut reader).expect("Can not decode transaction");
        Ok(object)
    }
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

impl RpcApi for RpcClient {
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
            .map(|a| a.map_err(BitcoincoreRpcError::Json))
            .collect::<RResult<Vec<_>>>()?;
        let req = self.client.build_request(cmd, &raw_args);

        let resp = self.client.send_request(req).map_err(JsonRpcError::from);
        Ok(resp?.result()?)
    }
}

pub type RResult<T> = Result<T, BitcoincoreRpcError>;

/// The error type for errors produced in this library.
#[derive(Debug)]
pub enum BitcoincoreRpcError {
    JsonRpc(jsonrpc::error::Error),
    //Hex(hex::Error),
    Json(serde_json::error::Error),
    //BitcoinSerialization(bitcoin::consensus::encode::Error),
    //Secp256k1(secp256k1::Error),
    //Io(io::Error),
    //InvalidAmount(bitcoin::util::amount::ParseAmountError),
    //InvalidCookieFile,
    // The JSON result had an unexpected structure.
    //UnexpectedStructure,
    // The daemon returned an error string.
    //ReturnedError(String),
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
fn handle_defaults<'a>(
    args: &'a mut [serde_json::Value],
    defaults: &[serde_json::Value],
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
