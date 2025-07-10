use core::convert::TryFrom;
use miniscript::bitcoin::{
    secp256k1::{All, Secp256k1},
    PublicKey, ScriptBuf, ScriptHash, WScriptHash, XOnlyPublicKey,
};

use super::Error;

#[derive(serde::Deserialize)]
pub(super) struct LegacyCoinbaseOutput {
    /// Specifies type of the script used in the output.
    ///
    /// Supported values include:
    /// - `"P2PK"`: Pay-to-Public-Key
    /// - `"P2PKH"`: Pay-to-Public-Key-Hash
    /// - `"P2SH"`: Pay-to-Script-Hash
    /// - `"P2WPKH"`: Pay-to-Witness-Public-Key-Hash:w

    /// - `"P2WSH"`: Pay-to-Witness-Script-Hash
    /// - `"P2TR"`: Pay-to-Taproot
    pub(super) output_script_type: String,

    /// Value associated with the script, typically a public key or script hash.
    ///
    /// This field's interpretation depends on the `output_script_type`:
    /// - For `"P2PK"`: The raw public key.
    /// - For `"P2PKH"`: A public key hash.
    /// - For `"P2WPKH"`: A witness public key hash.
    /// - For `"P2SH"`: A script hash.
    /// - For `"P2WSH"`: A witness script hash.
    /// - For `"P2TR"`: An x-only public key.
    pub(super) output_script_value: String,
}

impl TryFrom<LegacyCoinbaseOutput> for super::CoinbaseRewardScript {
    type Error = super::Error;
    fn try_from(value: LegacyCoinbaseOutput) -> Result<Self, Self::Error> {
        let script_pubkey = match value.output_script_type.as_str() {
            "TEST" => {
                let pub_key_hash = value
                    .output_script_value
                    .parse::<PublicKey>()
                    .map_err(|_| Error::InvalidOutputScript)?
                    .pubkey_hash();
                ScriptBuf::new_p2pkh(&pub_key_hash)
            }
            "P2PK" => {
                let pub_key = value
                    .output_script_value
                    .parse::<PublicKey>()
                    .map_err(|_| Error::InvalidOutputScript)?;
                ScriptBuf::new_p2pk(&pub_key)
            }
            "P2PKH" => {
                let pub_key_hash = value
                    .output_script_value
                    .parse::<PublicKey>()
                    .map_err(|_| Error::InvalidOutputScript)?
                    .pubkey_hash();
                ScriptBuf::new_p2pkh(&pub_key_hash)
            }
            "P2WPKH" => {
                let w_pub_key_hash = value
                    .output_script_value
                    .parse::<PublicKey>()
                    .map_err(|_| Error::InvalidOutputScript)?
                    .wpubkey_hash()
                    .unwrap();
                ScriptBuf::new_p2wpkh(&w_pub_key_hash)
            }
            "P2SH" => {
                let script_hashed = value
                    .output_script_value
                    .parse::<ScriptHash>()
                    .map_err(|_| Error::InvalidOutputScript)?;
                ScriptBuf::new_p2sh(&script_hashed)
            }
            "P2WSH" => {
                let w_script_hashed = value
                    .output_script_value
                    .parse::<WScriptHash>()
                    .map_err(|_| Error::InvalidOutputScript)?;
                ScriptBuf::new_p2wsh(&w_script_hashed)
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
                ScriptBuf::new_p2tr::<All>(&Secp256k1::<All>::new(), pub_key, None)
            }
            _ => return Err(Error::UnknownOutputScriptType),
        };
        Ok(Self {
            script_pubkey,
            // legacy encoding gives no way to specify testnet or mainnet
            ok_for_mainnet: true,
        })
    }
}

/// A coinbase output script as it appears in a configuration file.
///
/// Private to avoid exposing the enum constructors.
#[derive(serde::Deserialize)]
#[serde(untagged)] // decode as whichever variant makes sense for the input
enum SerdeCoinbaseOutputInner {
    Legacy(LegacyCoinbaseOutput),
    Descriptor(String),
}

/// A structure representing a coinbase output script as it appears in a
/// configuration file.
///
/// Can only be constructed via serde, and supports no operations except conversion
/// to a [`super::CoinbaseOutput`] via [`TryFrom`].
#[derive(serde::Deserialize)]
#[serde(transparent)]
pub struct SerdeCoinbaseOutput {
    inner: SerdeCoinbaseOutputInner,
}

impl TryFrom<SerdeCoinbaseOutput> for super::CoinbaseRewardScript {
    type Error = super::Error;
    fn try_from(value: SerdeCoinbaseOutput) -> Result<Self, Self::Error> {
        match value.inner {
            SerdeCoinbaseOutputInner::Legacy(legacy) => Self::try_from(legacy),
            SerdeCoinbaseOutputInner::Descriptor(ref s) => Self::from_descriptor(s),
        }
    }
}
