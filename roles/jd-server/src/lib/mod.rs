pub mod job_declarator;
pub mod mempool;
pub mod error;
pub mod status;

use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use roles_logic_sv2::{
    errors::Error, parsers::PoolMessages as JdsMessages, utils::CoinbaseOutput as CoinbaseOutput_,
};
use std::convert::{TryFrom, TryInto};
use stratum_common::bitcoin::{Script, TxOut};
use codec_sv2::{StandardEitherFrame, StandardSv2Frame};
use serde::Deserialize;

pub type Message = JdsMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;

pub fn get_coinbase_output(config: &Configuration) -> Result<Vec<TxOut>, Error> {
    let mut result = Vec::new();
    for coinbase_output_pool in &config.coinbase_outputs {
        let coinbase_output: CoinbaseOutput_ = coinbase_output_pool.try_into()?;
        let output_script: Script = coinbase_output.try_into()?;
        result.push(TxOut {
            value: 0,
            script_pubkey: output_script,
        });
    }
    match result.is_empty() {
        true => Err(Error::EmptyCoinbaseOutputs),
        _ => Ok(result),
    }
}

impl TryFrom<&CoinbaseOutput> for CoinbaseOutput_ {
    type Error = Error;

    fn try_from(pool_output: &CoinbaseOutput) -> Result<Self, Self::Error> {
        match pool_output.output_script_type.as_str() {
            "P2PK" | "P2PKH" | "P2WPKH" | "P2SH" | "P2WSH" | "P2TR" => Ok(CoinbaseOutput_ {
                output_script_type: pool_output.clone().output_script_type,
                output_script_value: pool_output.clone().output_script_value,
            }),
            _ => Err(Error::UnknownOutputScriptType),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct CoinbaseOutput {
    output_script_type: String,
    output_script_value: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Configuration {
    pub listen_jd_address: String,
    pub authority_public_key: Secp256k1PublicKey,
    pub authority_secret_key: Secp256k1SecretKey,
    pub cert_validity_sec: u64,
    pub coinbase_outputs: Vec<CoinbaseOutput>,
    pub core_rpc_url: String,
    pub core_rpc_port: u16,
    pub core_rpc_user: String,
    pub core_rpc_pass: String,
}