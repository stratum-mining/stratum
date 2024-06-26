use crate::{PoolError, PoolResult};
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use roles_logic_sv2::{utils::CoinbaseOutput as CoinbaseOutput_, Error as RolesLogicSv2Error};
use serde::Deserialize;
use std::convert::{TryFrom, TryInto};
use stratum_common::bitcoin::{Script, TxOut};

pub fn get_coinbase_output(config: &PoolConfig) -> PoolResult<Vec<TxOut>> {
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
        true => Err(PoolError::RolesLogicSv2(
            RolesLogicSv2Error::EmptyCoinbaseOutputs,
        )),
        _ => Ok(result),
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct CoinbaseOutput {
    output_script_type: String,
    output_script_value: String,
}

impl TryFrom<&CoinbaseOutput> for CoinbaseOutput_ {
    type Error = RolesLogicSv2Error;

    fn try_from(pool_output: &CoinbaseOutput) -> Result<CoinbaseOutput_, RolesLogicSv2Error> {
        match pool_output.output_script_type.as_str() {
            "TEST" | "P2PK" | "P2PKH" | "P2WPKH" | "P2SH" | "P2WSH" | "P2TR" => {
                Ok(CoinbaseOutput_ {
                    output_script_type: pool_output.clone().output_script_type,
                    output_script_value: pool_output.clone().output_script_value,
                })
            }
            _ => Err(RolesLogicSv2Error::UnknownOutputScriptType),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct PoolConfig {
    pub listen_address: String,
    pub tp_address: String,
    pub tp_authority_public_key: Option<Secp256k1PublicKey>,
    pub authority_public_key: Secp256k1PublicKey,
    pub authority_secret_key: Secp256k1SecretKey,
    pub cert_validity_sec: u64,
    pub coinbase_outputs: Vec<CoinbaseOutput>,
    pub pool_signature: String,
    #[cfg(feature = "test_only_allow_unencrypted")]
    pub test_only_listen_adress_plain: String,
}
