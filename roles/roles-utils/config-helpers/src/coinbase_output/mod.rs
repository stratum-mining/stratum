mod errors;
mod serde_types;

use miniscript::{
    bitcoin::{address::NetworkUnchecked, Address, Network, Script, ScriptBuf},
    DefiniteDescriptorKey, Descriptor,
};

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
    /// Creates a new [`CoinbaseOutput`] from a descriptor string.
    pub fn from_descriptor(mut s: &str) -> Result<Self, Error> {
        // Taproot descriptors cannot be parsed with `expression::Tree::from_str` and
        // need special handling. So we special-case them early and just pass to
        // rust-miniscript. In Miniscript 13 we will not need to do this.
        if s.starts_with("tr") {
            let desc = s.parse::<Descriptor<DefiniteDescriptorKey>>()?;
            return Ok(Self {
                script_pubkey: desc.script_pubkey(),
                // Descriptors don't have a way to specify a network, so we assume
                // they are OK to be used on mainnet.
                ok_for_mainnet: true,
            });
        }

        // Manually verify the checksum. FIXME in Miniscript 13 we will not need
        // to do this, since `expression::Tree::from_str` will do the checksum
        // validation for us. (And yield a much less horrible error type.)
        if let Some((desc_str, checksum_str)) = s.rsplit_once('#') {
            let expected_sum = miniscript::descriptor::checksum::desc_checksum(desc_str)?;
            if checksum_str != expected_sum {
                return Err(miniscript::Error::BadDescriptor(format!(
                    "Invalid checksum '{checksum_str}', expected '{expected_sum}'"
                ))
                .into());
            }
            s = desc_str;
        }

        let tree = miniscript::expression::Tree::from_str(s)?;
        match tree.name {
            "addr" => {
                if tree.args.len() != 1 {
                    return Err(Error::AddrDescriptorNChildren(tree.args.len()));
                }
                if !tree.args[0].args.is_empty() {
                    return Err(Error::AddrDescriptorGrandchild);
                }

                let addr = tree.args[0].name.parse::<Address<NetworkUnchecked>>()?;
                Ok(Self {
                    script_pubkey: addr.assume_checked_ref().script_pubkey(),
                    ok_for_mainnet: addr.is_valid_for_network(Network::Bitcoin),
                })
            }
            _ => {
                let desc = s.parse::<Descriptor<DefiniteDescriptorKey>>()?;
                Ok(Self {
                    script_pubkey: desc.script_pubkey(),
                    // Descriptors don't have a way to specify a network, so we assume
                    // they are OK to be used on mainnet.
                    ok_for_mainnet: true,
                })
            }
        }
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
