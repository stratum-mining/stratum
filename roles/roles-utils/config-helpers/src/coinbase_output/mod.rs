mod errors;

use core::convert::TryFrom;

use miniscript::bitcoin::{
    secp256k1::{All, Secp256k1},
    PublicKey, ScriptBuf, ScriptHash, WScriptHash, XOnlyPublicKey,
};

pub use errors::Error;

/// Coinbase output transaction.
///
/// Typically used for parsing coinbase outputs defined in SRI role configuration files.
#[derive(Debug, serde::Deserialize, Clone)]
pub struct CoinbaseOutput {
    /// Specifies type of the script used in the output.
    ///
    /// Supported values include:
    /// - `"P2PK"`: Pay-to-Public-Key
    /// - `"P2PKH"`: Pay-to-Public-Key-Hash
    /// - `"P2SH"`: Pay-to-Script-Hash
    /// - `"P2WPKH"`: Pay-to-Witness-Public-Key-Hash
    /// - `"P2WSH"`: Pay-to-Witness-Script-Hash
    /// - `"P2TR"`: Pay-to-Taproot
    pub output_script_type: String,

    /// Value associated with the script, typically a public key or script hash.
    ///
    /// This field's interpretation depends on the `output_script_type`:
    /// - For `"P2PK"`: The raw public key.
    /// - For `"P2PKH"`: A public key hash.
    /// - For `"P2WPKH"`: A witness public key hash.
    /// - For `"P2SH"`: A script hash.
    /// - For `"P2WSH"`: A witness script hash.
    /// - For `"P2TR"`: An x-only public key.
    pub output_script_value: String,
}

impl CoinbaseOutput {
    /// Creates a new [`CoinbaseOutput`].
    pub fn new(output_script_type: String, output_script_value: String) -> Self {
        Self {
            output_script_type,
            output_script_value,
        }
    }
}

impl TryFrom<CoinbaseOutput> for ScriptBuf {
    type Error = Error;

    fn try_from(value: CoinbaseOutput) -> Result<Self, Self::Error> {
        match value.output_script_type.as_str() {
            "TEST" => {
                let pub_key_hash = value
                    .output_script_value
                    .parse::<PublicKey>()
                    .map_err(|_| Error::InvalidOutputScript)?
                    .pubkey_hash();
                Ok(ScriptBuf::new_p2pkh(&pub_key_hash))
            }
            "P2PK" => {
                let pub_key = value
                    .output_script_value
                    .parse::<PublicKey>()
                    .map_err(|_| Error::InvalidOutputScript)?;
                Ok(ScriptBuf::new_p2pk(&pub_key))
            }
            "P2PKH" => {
                let pub_key_hash = value
                    .output_script_value
                    .parse::<PublicKey>()
                    .map_err(|_| Error::InvalidOutputScript)?
                    .pubkey_hash();
                Ok(ScriptBuf::new_p2pkh(&pub_key_hash))
            }
            "P2WPKH" => {
                let w_pub_key_hash = value
                    .output_script_value
                    .parse::<PublicKey>()
                    .map_err(|_| Error::InvalidOutputScript)?
                    .wpubkey_hash()
                    .unwrap();
                Ok(ScriptBuf::new_p2wpkh(&w_pub_key_hash))
            }
            "P2SH" => {
                let script_hashed = value
                    .output_script_value
                    .parse::<ScriptHash>()
                    .map_err(|_| Error::InvalidOutputScript)?;
                Ok(ScriptBuf::new_p2sh(&script_hashed))
            }
            "P2WSH" => {
                let w_script_hashed = value
                    .output_script_value
                    .parse::<WScriptHash>()
                    .map_err(|_| Error::InvalidOutputScript)?;
                Ok(ScriptBuf::new_p2wsh(&w_script_hashed))
            }
            "P2TR" => {
                // From the bip
                //
                // Conceptually, every Taproot output corresponds to a combination of
                // a single public key condition (the internal key),
                // and zero or more general conditions encoded in scripts organized in a tree.
                let pub_key = value
                    .output_script_value
                    .parse::<XOnlyPublicKey>()
                    .map_err(|_| Error::InvalidOutputScript)?;
                Ok(ScriptBuf::new_p2tr::<All>(
                    &Secp256k1::<All>::new(),
                    pub_key,
                    None,
                ))
            }
            _ => Err(Error::UnknownOutputScriptType),
        }
    }
}
