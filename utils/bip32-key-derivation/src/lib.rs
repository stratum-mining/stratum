use std::str::FromStr;
use stratum_common::roles_logic_sv2::bitcoin::{
    bip32::{DerivationPath, Error, Xpub},
    secp256k1::Secp256k1,
};

pub fn derive_child_public_key(xpub: &Xpub, path: &str) -> Result<Xpub, Error> {
    let secp = Secp256k1::new();
    let derivation_path = DerivationPath::from_str(path)?;
    let child_pub_key = xpub.derive_pub(&secp, &derivation_path)?;
    Ok(child_pub_key)
}
