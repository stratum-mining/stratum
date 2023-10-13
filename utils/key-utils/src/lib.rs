use bs58::{decode, decode::Error as Bs58DecodeError};
use core::convert::TryFrom;
use secp256k1::{SecretKey, XOnlyPublicKey};
use serde::{Deserialize, Serialize};
use std::fmt::Display;

#[derive(Debug)]
pub enum Error {
    Bs58Decode(Bs58DecodeError),
    Secp256k1(secp256k1::Error),
    Custom,
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Key Utils Error")
    }
}

impl From<Bs58DecodeError> for Error {
    fn from(e: Bs58DecodeError) -> Self {
        Error::Bs58Decode(e)
    }
}

impl From<secp256k1::Error> for Error {
    fn from(e: secp256k1::Error) -> Self {
        Error::Secp256k1(e)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct Secp256k1SecretKey(pub SecretKey);

impl TryFrom<String> for Secp256k1SecretKey {
    type Error = Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let decoded = decode(value).into_vec()?;
        let secret = SecretKey::from_slice(&decoded)?;
        Ok(Secp256k1SecretKey(secret))
    }
}

impl From<Secp256k1SecretKey> for String {
    fn from(secret: Secp256k1SecretKey) -> Self {
        let bytes = secret.0.secret_bytes();
        bs58::encode(bytes).into_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct Secp256k1PublicKey(pub XOnlyPublicKey);

impl TryFrom<String> for Secp256k1PublicKey {
    type Error = Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let decoded = decode(value).into_vec()?;
        let public = XOnlyPublicKey::from_slice(&decoded).expect("Invalid public key");
        Ok(Secp256k1PublicKey(public))
    }
}

impl From<Secp256k1PublicKey> for String {
    fn from(public: Secp256k1PublicKey) -> Self {
        let bytes = public.0.serialize();
        bs58::encode(bytes).into_string()
    }
}

impl Secp256k1PublicKey {
    pub fn into_bytes(self) -> [u8; 32] {
        self.0.serialize()
    }
}
impl Secp256k1SecretKey {
    pub fn into_bytes(self) -> [u8; 32] {
        self.0.secret_bytes()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[derive(Serialize, Deserialize)]
    struct Test {
        public_key: Secp256k1PublicKey,
        secret_key: Secp256k1SecretKey,
    }

    #[test]
    fn deserialize_serialize_toml() {
        let pub_ = "3VANfft6ei6jQq1At7d8nmiZzVhBFS4CiQujdgim1ign";
        let secr = "7qbpUjScc865jyX2kiB4NVJANoC7GA7TAJupdzXWkc62";
        let string = r#"
            public_key = "3VANfft6ei6jQq1At7d8nmiZzVhBFS4CiQujdgim1ign"
            secret_key = "7qbpUjScc865jyX2kiB4NVJANoC7GA7TAJupdzXWkc62"
        "#;
        let test: Test = toml::from_str(&string).unwrap();
        let ser_p: String = test.public_key.try_into().unwrap();
        let ser_s: String = test.secret_key.try_into().unwrap();
        assert_eq!(ser_p, pub_);
        assert_eq!(ser_s, secr);
    }
}
