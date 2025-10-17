#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use bs58::{decode, decode::Error as Bs58DecodeError};
use core::{convert::TryFrom, fmt::Display, str::FromStr};
use secp256k1::{
    schnorr::Signature, Keypair, Message as SecpMessage, Secp256k1, SecretKey, SignOnly,
    VerifyOnly, XOnlyPublicKey,
};
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub enum Error {
    Bs58Decode(Bs58DecodeError),
    Secp256k1(secp256k1::Error),
    KeyVersion(u16),
    KeyLength,
    Custom(String),
}

impl Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Bs58Decode(error) => write!(f, "Base58 code error: {error}"),
            Self::Secp256k1(error) => write!(f, "Secp256k1 error: {error}"),
            Self::KeyVersion(obtained) => {
                write!(f, "Unknown public key version. version found: {obtained}")
            }
            Self::KeyLength => write!(f, "Bad key length"),
            Self::Custom(error) => write!(f, "Custom error: {error}"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {}
#[cfg(not(feature = "std"))]
#[rustversion::since(1.81)]
impl core::error::Error for Error {}

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

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct Secp256k1SecretKey(pub SecretKey);

impl TryFrom<String> for Secp256k1SecretKey {
    type Error = Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl FromStr for Secp256k1SecretKey {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let decoded = decode(value).with_check(None).into_vec()?;
        let secret = SecretKey::from_slice(&decoded)?;
        Ok(Secp256k1SecretKey(secret))
    }
}

impl From<Secp256k1SecretKey> for String {
    fn from(secret: Secp256k1SecretKey) -> Self {
        secret.to_string()
    }
}

impl Display for Secp256k1SecretKey {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        let bytes = self.0.secret_bytes();
        f.write_str(&bs58::encode(bytes).with_check().into_string())
    }
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct Secp256k1PublicKey(pub XOnlyPublicKey);

impl TryFrom<String> for Secp256k1PublicKey {
    type Error = Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl FromStr for Secp256k1PublicKey {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let decoded = decode(value).with_check(None).into_vec()?;
        if decoded.len() < 34 {
            return Err(Error::KeyLength);
        }
        let key_version =
            u16::from_le_bytes(decoded[..2].try_into().expect("Invalid array length"));
        if key_version != 1 {
            return Err(Error::KeyVersion(key_version));
        }
        let public = XOnlyPublicKey::from_slice(&decoded[2..]).map_err(Error::Secp256k1)?;
        Ok(Secp256k1PublicKey(public))
    }
}

impl From<Secp256k1PublicKey> for String {
    fn from(public: Secp256k1PublicKey) -> Self {
        public.to_string()
    }
}

impl Display for Secp256k1PublicKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut output = [0_u8; 34];
        output[0] = 1;
        let bytes = self.0.serialize();
        output[2..].copy_from_slice(&bytes);
        f.write_str(&bs58::encode(&output).with_check().into_string())
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

impl From<Secp256k1SecretKey> for Secp256k1PublicKey {
    fn from(value: Secp256k1SecretKey) -> Self {
        let context = secp256k1::Secp256k1::new();
        let (x_coordinate, _) = value.0.public_key(&context).x_only_public_key();
        Self(x_coordinate)
    }
}

pub struct SignatureService {
    secp_sign: Secp256k1<SignOnly>,
    secp_verify: Secp256k1<VerifyOnly>,
}

impl SignatureService {
    pub fn new() -> Self {
        SignatureService {
            secp_sign: Secp256k1::signing_only(),
            secp_verify: Secp256k1::verification_only(),
        }
    }

    #[cfg(feature = "std")]
    pub fn sign(&self, message: Vec<u8>, private_key: SecretKey) -> Signature {
        self.sign_with_rng(message, private_key, &mut rand::thread_rng())
    }

    #[inline]
    pub fn sign_with_rng<R: rand::Rng + rand::CryptoRng>(
        &self,
        message: Vec<u8>,
        private_key: SecretKey,
        rng: &mut R,
    ) -> Signature {
        let secret_key = private_key;
        let kp = Keypair::from_secret_key(&self.secp_sign, &secret_key);

        self.secp_sign.sign_schnorr_with_rng(
            &SecpMessage::from_digest_slice(&message).unwrap(),
            &kp,
            rng,
        )
    }

    pub fn verify(
        &self,
        message: Vec<u8>,
        signature: secp256k1::schnorr::Signature,
        public_key: XOnlyPublicKey,
    ) -> Result<(), secp256k1::Error> {
        let x_only_public_key = public_key;

        // Verify signature
        self.secp_verify.verify_schnorr(
            &signature,
            &secp256k1::Message::from_digest_slice(&message)?,
            &x_only_public_key,
        )
    }
}

impl Default for SignatureService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn key_conversions() {
        let secret_key = "zmBEmPhqo3A92FkiLVvyCz6htc3e53ph3ZbD4ASqGaLjwnFLi";
        let public_key = "9bDuixKmZqAJnrmP746n8zU1wyAQRrus7th9dxnkPg6RzQvCnan";
        let bad_public_key1 = "9bDuixKmZqAJnrmP746n8zU1wyAQRrus7th9dxnkPg6RzQvCnam"; // invalid checksum (swapped char)
        let bad_public_key2 = "2myPhc5vkPzuC5FXNK5tee79WmP7uoLh55SxezoF8iqwF3E3rnPY"; // invalid version (version 12)
        let bad_public_key3 = "2wmHTKZkLg2QzXyEXGMBXzKP7JXDUt8yy9SA5hoQwERc92qR6c"; // invalid length (1 B missing)

        let error = bad_public_key1
            .parse::<Secp256k1PublicKey>()
            .expect_err("Bad bud public key failed to raise error");
        assert!(
            matches!(error, Error::Bs58Decode(_)),
            "expected failed checksum error, got {}",
            error
        );
        let error = bad_public_key2
            .parse::<Secp256k1PublicKey>()
            .expect_err("Bad bud public key failed to raise error");
        assert!(
            matches!(error, Error::KeyVersion(_)),
            "expected invalid key version error, got {}",
            error
        );
        let error = bad_public_key3
            .parse::<Secp256k1PublicKey>()
            .expect_err("Bad bud public key failed to raise error");
        assert!(
            matches!(error, Error::KeyLength),
            "expected invalid key length error, got {}",
            error
        );

        let parsed_key = secret_key
            .parse::<Secp256k1SecretKey>()
            .expect("Invalid test key");

        let calculated_public_key = Secp256k1PublicKey::from(parsed_key);
        assert_eq!(calculated_public_key.to_string(), public_key);

        let parsed_public_key = public_key
            .parse::<Secp256k1PublicKey>()
            .expect("Invalid test pubkey");
        assert_eq!(calculated_public_key.0, parsed_public_key.0);
    }
}
