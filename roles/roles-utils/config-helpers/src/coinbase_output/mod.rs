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
    ok_for_mainnet: bool,
}

impl CoinbaseOutput {
    /// Creates a new [`CoinbaseOutput`] from a script type and value.
    pub fn from_type_and_value(
        output_script_type: String,
        output_script_value: String,
    ) -> Result<Self, Error> {
        Self::try_from(serde_types::LegacyCoinbaseOutput {
            output_script_type,
            output_script_value,
        })
    }

    /// Whether this coinbase output is okay for use on mainnet.
    ///
    /// This is a "best effort" check and currently only returns false if the user
    /// provides an addr() descriptor in which they specified a testnet or regtest
    /// address.
    pub fn ok_for_mainnet(&self) -> bool {
        self.ok_for_mainnet
    }

    /// The `scriptPubKey` associated with the coinbase output
    pub fn script_pubkey(&self) -> &Script {
        &self.script_pubkey
    }
}
