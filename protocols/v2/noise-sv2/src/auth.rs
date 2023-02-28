use bytes::{BufMut, BytesMut};
use core::{convert::TryFrom, time::Duration};
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

use crate::{Error, Result, StaticPublicKey};

use ed25519_dalek::Signer;

pub use crate::formats::*;

use std::io::Write;

/// Header of the `SignedPart` that will also be part of the `Certificate`
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct SignedPartHeader {
    version: u16,
    // Validity start time (unix timestamp)
    valid_from: u32,
    // Signature is invalid after this point in time (unix timestamp)
    not_valid_after: u32,
}

impl SignedPartHeader {
    const VERSION: u16 = 0;

    pub fn serialize_to_writer<T: Write>(&self, writer: &mut T) -> Result<()> {
        let version = self.version.to_le_bytes();
        let valid_from = self.valid_from.to_le_bytes();
        let not_valid_after = self.not_valid_after.to_le_bytes();
        writer
            .write_all(&[&version[..], &valid_from[..], &not_valid_after[..]].concat()[..])
            .map_err(|_| Error::IoError)?;
        Ok(())
    }

    pub fn from_bytes(b: &[u8]) -> Self {
        debug_assert!(b.len() == 10);
        let version = u16::from_le_bytes([b[0], b[1]]);
        let valid_from = u32::from_le_bytes([b[2], b[3], b[4], b[5]]);
        let not_valid_after = u32::from_le_bytes([b[6], b[7], b[8], b[9]]);
        Self {
            version,
            valid_from,
            not_valid_after,
        }
    }

    pub fn with_duration(valid_for: Duration) -> Result<Self> {
        let valid_from = SystemTime::now();
        let not_valid_after = valid_from + valid_for;
        Ok(Self {
            version: Self::VERSION,
            valid_from: Self::system_time_to_unix_time_u32(&valid_from)?,
            not_valid_after: Self::system_time_to_unix_time_u32(&not_valid_after)?,
        })
    }

    pub fn valid_from(&self) -> SystemTime {
        Self::unix_time_u32_to_system_time(self.valid_from)
            .expect("BUG: cannot provide 'valid_from' time")
    }

    pub fn not_valid_after(&self) -> SystemTime {
        Self::unix_time_u32_to_system_time(self.not_valid_after)
            .expect("BUG: cannot provide 'not_valid_after' time")
    }

    pub fn verify_expiration(&self, now: SystemTime) -> Result<()> {
        let now_timestamp = Self::system_time_to_unix_time_u32(&now)?;
        if now_timestamp < self.valid_from {
            return Err(Error::CertificateInvalid(self.valid_from, now_timestamp));
        }
        if now_timestamp > self.not_valid_after {
            return Err(Error::CertificateExpired(
                self.not_valid_after,
                now_timestamp,
            ));
        }
        Ok(())
    }

    /// Convert system time to UNIX time
    fn system_time_to_unix_time_u32(t: &SystemTime) -> Result<u32> {
        Ok(t.duration_since(SystemTime::UNIX_EPOCH)
            .map(|duration| duration.as_secs() as u32)?)
    }

    /// Convert UNIX time to system time
    fn unix_time_u32_to_system_time(unix_timestamp: u32) -> Result<SystemTime> {
        SystemTime::UNIX_EPOCH
            .checked_add(Duration::from_secs(unix_timestamp.into()))
            .ok_or(Error::BadTimestampFromSystemTime(unix_timestamp))
    }
}

/// Helper struct for performing the actual signature of the relevant parts of the certificate
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct SignedPart {
    pub(crate) header: SignedPartHeader,
    pub(crate) pubkey: StaticPublicKey,
    pub(crate) authority_public_key: ed25519_dalek::PublicKey,
}

impl SignedPart {
    pub fn new(
        header: SignedPartHeader,
        pubkey: StaticPublicKey,
        authority_public_key: ed25519_dalek::PublicKey,
    ) -> Self {
        Self {
            header,
            pubkey,
            authority_public_key,
        }
    }

    fn serialize_to_buf(&self) -> Result<BytesMut> {
        let mut signed_part_writer = BytesMut::new().writer();
        let version = &self.header.version.to_le_bytes()[..];
        let valid_from = &self.header.valid_from.to_le_bytes()[..];
        let not_valid_after = &self.header.not_valid_after.to_le_bytes()[..];
        let pub_k_len = [32, 0];
        let pub_k = &self.pubkey[..];
        let auth_pub_k = &self.authority_public_key.as_bytes()[..];
        signed_part_writer
            .write_all(
                &[
                    version,
                    valid_from,
                    not_valid_after,
                    &pub_k_len,
                    pub_k,
                    &pub_k_len,
                    auth_pub_k,
                ]
                .concat()[..],
            )
            .map_err(|_| Error::IoError)?;
        Ok(signed_part_writer.into_inner())
    }

    /// Generates the actual `ed25519_dalek::Signature` to embed into the certificate.
    pub fn sign_with(&self, keypair: &ed25519_dalek::Keypair) -> Result<ed25519_dalek::Signature> {
        debug_assert_eq!(
            keypair.public,
            self.authority_public_key,
            "BUG: Signing Authority public key ({}) inside the certificate doesn't match the key \
             we are trying to sign with (its public key is: {})",
            EncodedEd25519PublicKey::new(keypair.public),
            EncodedEd25519PublicKey::new(self.authority_public_key)
        );

        let signed_part_buf = self.serialize_to_buf()?;
        Ok(keypair.sign(&signed_part_buf[..]))
    }

    /// Verifies the specified `signature` against this signed part.
    pub(crate) fn verify(&self, signature: &ed25519_dalek::Signature) -> Result<()> {
        let signed_part_buf = self.serialize_to_buf()?;
        self.authority_public_key
            .verify_strict(&signed_part_buf[..], signature)?;
        Ok(())
    }

    pub(crate) fn verify_expiration(&self, now: SystemTime) -> Result<()> {
        self.header.verify_expiration(now)
    }
}

/// The payload message that will be appended to the handshake message to proof static key
/// authenticity
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct SignatureNoiseMessage {
    pub(crate) header: SignedPartHeader,
    pub(crate) signature: ed25519_dalek::Signature,
}

impl SignatureNoiseMessage {
    pub fn serialize_to_writer<T: Write>(&self, writer: &mut T) -> Result<()> {
        self.header.serialize_to_writer(writer)?;
        writer
            .write_all(&self.signature.to_bytes()[..])
            .map_err(|_| Error::IoError)?;
        Ok(())
    }

    pub fn serialize_to_bytes_mut(&self) -> Result<BytesMut> {
        let mut writer = BytesMut::with_capacity(super::SIGNATURE_MESSAGE_LEN).writer();
        self.serialize_to_writer(&mut writer)?;
        let serialized_signature_noise_message = writer.into_inner();
        Ok(serialized_signature_noise_message)
    }

    pub fn with_duration(pub_k: &[u8], priv_k: &[u8], duration: core::time::Duration) -> Self {
        let to_be_signed_keypair =
            crate::generate_keypair().expect("BUG: cannot generate noise static keypair");
        let authority_keypair = ed25519_dalek::Keypair::from_bytes(&[priv_k, pub_k].concat())
            .expect("BUG: cannot generate noise authority keypair");
        let header = SignedPartHeader::with_duration(duration)
            .expect("BUG: cannot prepare certificate header");
        let signed_part = SignedPart::new(
            header.clone(),
            to_be_signed_keypair.public,
            authority_keypair.public,
        );
        let signature = signed_part
            .sign_with(&authority_keypair)
            .expect("BUG: cannot sign");
        Self { header, signature }
    }
}

// Deserialization implementation
impl TryFrom<&[u8]> for SignatureNoiseMessage {
    type Error = Error;

    fn try_from(data: &[u8]) -> Result<Self> {
        debug_assert!(data.len() == 74);
        let header = &data[0..10];
        let siganture = &data[10..74];
        let header = SignedPartHeader::from_bytes(header);
        let signature = ed25519_dalek::Signature::from_bytes(siganture)?;
        Ok(SignatureNoiseMessage { header, signature })
    }
}

#[cfg(test)]
pub(crate) mod test {
    use super::{
        super::{generate_keypair, StaticKeypair},
        *,
    };
    use rand::rngs::OsRng;
    const TEST_CERT_VALIDITY: Duration = Duration::from_secs(3600);

    // Helper that builds a `SignedPart` (as a base e.g. for a noise message or a certificate),
    // testing authority `ed25519_dalek::Keypair` (that actually generated the signature) and the
    // `ed25519_dalek::Signature`
    pub(crate) fn build_test_signed_part_and_auth() -> (
        SignedPart,
        ed25519_dalek::Keypair,
        StaticKeypair,
        ed25519_dalek::Signature,
    ) {
        let mut csprng = OsRng {};
        let to_be_signed_keypair =
            generate_keypair().expect("BUG: cannot generate noise static keypair");
        let authority_keypair = ed25519_dalek::Keypair::generate(&mut csprng);
        let header = SignedPartHeader::with_duration(TEST_CERT_VALIDITY)
            .expect("BUG: cannot prepare certificate header");

        let signed_part = SignedPart::new(
            header,
            to_be_signed_keypair.public.clone(),
            authority_keypair.public,
        );
        let signature = signed_part
            .sign_with(&authority_keypair)
            .expect("BUG: cannot sign");
        (
            signed_part,
            authority_keypair,
            to_be_signed_keypair,
            signature,
        )
    }

    #[test]
    fn header_time_validity_is_valid() {
        let header = SignedPartHeader::with_duration(TEST_CERT_VALIDITY)
            .expect("BUG: cannot build certificate header");
        header
            .verify_expiration(SystemTime::now() + Duration::from_secs(10))
            .expect("BUG: certificate should be evaluated as valid!");
    }

    #[test]
    fn header_time_validity_not_yet_valid() {
        let header = SignedPartHeader::with_duration(TEST_CERT_VALIDITY)
            .expect("BUG: cannot build certificate header");
        let result = header.verify_expiration(SystemTime::now() - Duration::from_secs(10));
        assert!(
            result.is_err(),
            "BUG: Certificate not evaluated as not valid yet: {:?}",
            result
        );
    }

    #[test]
    fn header_time_validity_is_expired() {
        let header = SignedPartHeader::with_duration(TEST_CERT_VALIDITY)
            .expect("BUG: cannot build certificate header");
        let result = header
            .verify_expiration(SystemTime::now() + TEST_CERT_VALIDITY + Duration::from_secs(10));
        assert!(
            result.is_err(),
            "BUG: Certificate not evaluated as expired: {:?}",
            result
        );
    }

    #[test]
    fn signature_noise_message_serialization() {
        let (signed_part, authority_keypair, _static_keypair, _signature) =
            build_test_signed_part_and_auth();

        let noise_message = SignatureNoiseMessage {
            header: signed_part.header.clone(),
            signature: signed_part
                .sign_with(&authority_keypair)
                .expect("BUG: cannot sign"),
        };

        let mut serialized_noise_message_writer = BytesMut::new().writer();
        noise_message
            .serialize_to_writer(&mut serialized_noise_message_writer)
            .expect("BUG: cannot serialize signature noise message");

        let serialized_noise_message_buf = serialized_noise_message_writer.into_inner();
        let deserialized_noise_message =
            SignatureNoiseMessage::try_from(&serialized_noise_message_buf[..])
                .expect("BUG: cannot deserialize signature noise message");

        assert_eq!(
            noise_message, deserialized_noise_message,
            "Signature noise messages don't match each other after serialization cycle"
        )
    }
}
