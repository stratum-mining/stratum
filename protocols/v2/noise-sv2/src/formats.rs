use alloc::string::String;
use core::{convert::TryFrom, fmt};
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

use crate::{
    auth::{SignatureNoiseMessage, SignedPart, SignedPartHeader},
    error::{Error, Result},
    StaticPublicKey, StaticSecretKey,
};

/// Generates implementation for the encoded type, Display trait and the file format and
macro_rules! impl_basic_type {
    ($encoded_struct_type:tt, $format_struct_type:ident, $inner_encoded_struct_type:ty,
     $format_struct_inner_rename:expr, $( $tr:tt ), *) => {
        /// Helper that ensures serialization of the `$inner_encoded_struct_type` into a prefered
        /// encoding
        #[derive(Serialize, Deserialize, Debug, $( $tr ), *)]
        #[serde(into = "String", try_from = "String")]
        pub struct $encoded_struct_type {
            inner: $inner_encoded_struct_type,
        }
        impl $encoded_struct_type {
            pub fn new(inner: $inner_encoded_struct_type) -> Self {
                Self { inner }
            }

            pub fn into_inner(self) -> $inner_encoded_struct_type {
                self.inner
            }
        }
        impl fmt::Display for $encoded_struct_type {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", String::from(self.clone()))
            }
        }
        #[allow(clippy::derive_partial_eq_without_eq)]
        #[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
        pub struct $format_struct_type {
            #[serde(rename = $format_struct_inner_rename)]
            inner: $encoded_struct_type,
        }
        impl $format_struct_type {
            pub fn new(inner: $inner_encoded_struct_type) -> Self {
                Self {
                    inner: $encoded_struct_type::new(inner),
                }
            }
            pub fn into_inner(self) -> $inner_encoded_struct_type {
                self.inner.into_inner()
            }
        }
        impl TryFrom<String> for $format_struct_type {
            type Error = Error;

            fn try_from(value: String) -> Result<Self> {
                Ok(serde_json::from_str(value.as_str())?)
            }
        }
        /// Helper serializer into string
        impl TryFrom<$format_struct_type> for String {
            type Error = Error;
            fn try_from(value: $format_struct_type) -> Result<String> {
                Ok(serde_json::to_string_pretty(&value)?)
            }
        }
    };
}

/// Generates implementation of conversions from/to Base58 encoding that we use for representing
/// Ed25519 keys, signatures etc.
macro_rules! generate_ed25519_structs {
    ($encoded_struct_type:tt, $format_struct_type:ident, $inner_encoded_struct_type:ty,
     $format_struct_inner_rename:expr, $( $tr:tt ), *) => {
        impl_basic_type!(
            $encoded_struct_type,
            $format_struct_type,
            $inner_encoded_struct_type,
            $format_struct_inner_rename,
            $($tr), *
        );

        impl TryFrom<String> for $encoded_struct_type {
            type Error = Error;

            fn try_from(value: String) -> Result<Self> {
                // Decode with checksum, don't verify version
                let bytes = bs58::decode(value).with_check(None).into_vec()?;
                Ok(Self::new(<$inner_encoded_struct_type>::from_bytes(&bytes)?))
            }
        }

        impl From<$encoded_struct_type> for String {
            fn from(value: $encoded_struct_type) -> Self {
                bs58::encode(&value.into_inner().to_bytes()[..]).with_check().into_string()
            }
        }
    };
}

macro_rules! generate_noise_keypair_structs {
    ($encoded_struct_type:tt, $format_struct_type: ident, $inner_encoded_struct_type:ty,
     $format_struct_inner_rename:expr) => {
        impl_basic_type!(
            $encoded_struct_type,
            $format_struct_type,
            $inner_encoded_struct_type,
            $format_struct_inner_rename,
            PartialEq,
            Eq,
            Clone
        );

        impl TryFrom<String> for $encoded_struct_type {
            type Error = Error;

            fn try_from(value: String) -> Result<Self> {
                let bytes = bs58::decode(value).with_check(None).into_vec()?;
                Ok(Self::new(bytes))
            }
        }

        impl From<$encoded_struct_type> for String {
            fn from(value: $encoded_struct_type) -> Self {
                bs58::encode(&value.into_inner()).with_check().into_string()
            }
        }
    };
}

generate_ed25519_structs!(
    EncodedEd25519PublicKey,
    Ed25519PublicKeyFormat,
    ed25519_dalek::PublicKey,
    "ed25519_public_key",
    PartialEq,
    Eq,
    Clone
);

generate_ed25519_structs!(
    EncodedEd25519SecretKey,
    Ed25519SecretKeyFormat,
    ed25519_dalek::SecretKey,
    "ed25519_secret_key",
);

/// Required by serde's Serialize trait, `ed25519_dalek::SecretKey` doesn't support
/// clone
impl Clone for EncodedEd25519SecretKey {
    fn clone(&self) -> Self {
        // Cloning the secret key should never fail and is considered bug as the original private
        // key is correct
        Self::new(
            ed25519_dalek::SecretKey::from_bytes(self.inner.as_bytes())
                .expect("BUG: cannot clone secret key"),
        )
    }
}

/// Required only to comply with the required interface of impl_ed25519_encoding_conversion macro
/// that generates
impl PartialEq for EncodedEd25519SecretKey {
    fn eq(&self, other: &Self) -> bool {
        self.inner.as_bytes() == other.inner.as_bytes()
    }
}

generate_ed25519_structs!(
    EncodedEd25519Signature,
    Ed25519SignatureFormat,
    ed25519_dalek::Signature,
    "ed25519_signature",
    PartialEq,
    Eq,
    Clone
);

generate_noise_keypair_structs!(
    EncodedStaticPublicKey,
    StaticPublicKeyFormat,
    StaticPublicKey,
    "noise_public_key"
);

generate_noise_keypair_structs!(
    EncodedStaticSecretKey,
    StaticSecretKeyFormat,
    StaticSecretKey,
    "noise_secret_key"
);

/// Certificate is intended to be serialized and deserialized from/into a file and loaded on the
/// stratum server.
/// Second use of the certificate is to build it from `SignatureNoiseMessage` and check its
/// validity
#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct Certificate {
    signed_part_header: SignedPartHeader,
    pub public_key: StaticPublicKeyFormat,
    authority_public_key: Ed25519PublicKeyFormat,
    signature: Ed25519SignatureFormat,
}

impl Certificate {
    pub fn new(signed_part: SignedPart, signature: ed25519_dalek::Signature) -> Self {
        Self {
            signed_part_header: signed_part.header,
            public_key: StaticPublicKeyFormat::new(signed_part.pubkey),
            authority_public_key: Ed25519PublicKeyFormat::new(signed_part.authority_public_key),
            signature: Ed25519SignatureFormat::new(signature),
        }
    }

    pub fn validate(&self) -> Result<()> {
        let signed_part = SignedPart::new(
            self.signed_part_header.clone(),
            self.public_key.clone().into_inner(),
            self.authority_public_key.clone().into_inner(),
        );
        signed_part.verify(&self.signature.clone().into_inner())?;
        signed_part.verify_expiration(SystemTime::now())
    }

    pub fn from_noise_message(
        signature_noise_message: SignatureNoiseMessage,
        pubkey: StaticPublicKey,
        authority_public_key: ed25519_dalek::PublicKey,
    ) -> Self {
        Self::new(
            SignedPart::new(signature_noise_message.header, pubkey, authority_public_key),
            signature_noise_message.signature,
        )
    }

    pub fn build_noise_message(&self) -> SignatureNoiseMessage {
        SignatureNoiseMessage {
            header: self.signed_part_header.clone(),
            signature: self.signature.clone().into_inner(),
        }
    }
}

impl TryFrom<String> for Certificate {
    type Error = Error;

    fn try_from(value: String) -> Result<Self> {
        Ok(serde_json::from_str(value.as_str())?)
    }
}

impl TryFrom<Certificate> for String {
    type Error = Error;
    fn try_from(value: Certificate) -> Result<String> {
        Ok(serde_json::to_string_pretty(&value)?)
    }
}

#[cfg(test)]
pub mod test {
    use super::*;
    use crate::auth::test::build_test_signed_part_and_auth;

    #[test]
    fn certificate_validate() {
        let (signed_part, _authority_keypair, _static_keypair, signature) =
            build_test_signed_part_and_auth();
        let certificate = Certificate::new(signed_part, signature);

        certificate.validate().expect("BUG: Certificate not valid!");
    }

    #[test]
    fn certificate_serialization() {
        let (signed_part, _authority_keypair, _static_keypair, signature) =
            build_test_signed_part_and_auth();
        let certificate = Certificate::new(signed_part, signature);

        let serialized_cert =
            serde_json::to_string(&certificate).expect("BUG: cannot serialize certificate");
        let deserialized_cert = serde_json::from_str(serialized_cert.as_str())
            .expect("BUG: cannot deserialized certificate");

        assert_eq!(certificate, deserialized_cert, "Certificates don't match!");
    }
}
