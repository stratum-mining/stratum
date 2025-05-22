mod errors;
mod serde_types;

use core::convert::TryFrom;

use miniscript::bitcoin::{Script, ScriptBuf};

pub use errors::Error;

/// Coinbase output transaction.
///
/// Typically used for parsing coinbase outputs defined in SRI role configuration files.
#[derive(Debug, serde::Deserialize, Clone)]
#[serde(try_from = "serde_types::SerdeCoinbaseOutput")]
pub struct CoinbaseOutput {
    script_pubkey: ScriptBuf,
}

impl CoinbaseOutput {
    /// Creates a new [`CoinbaseOutput`] from a script type and value.
    pub fn new(output_script_type: String, output_script_value: String) -> Result<Self, Error> {
        Self::try_from(serde_types::SerdeCoinbaseOutput {
            output_script_type,
            output_script_value,
        })
    }

    /// The `scriptPubKey` associated with the coinbase output
    pub fn script_pubkey(&self) -> &Script {
        &self.script_pubkey
    }
}
