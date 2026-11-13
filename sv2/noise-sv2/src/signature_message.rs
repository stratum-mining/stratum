// # Signature-Based Message Handling
//
// Defines the [`SignatureNoiseMessage`] struct, which represents a signed message used in the
// Noise protocol to authenticate and verify the identity of a party during the handshake.
//
// This module provides utilities for creating, signing, and verifying Noise protocol messages
// using Schnorr signatures over the [`secp256k1`] elliptic curve. It encapsulates signed messages
// along with versioning and validity timestamps. The following capabilities are supported:
//
// - Conversion between the 74-byte wire layout and structured [`SignatureNoiseMessage`] instances.
// - Message signing using Schnorr signatures and the [`secp256k1`] curve.
// - Verification of signed messages, ensuring they fall within valid time periods and are signed by
//   an authorized public key.
//
// ## Usage
//
// The [`SignatureNoiseMessage`] is the certificate a [`crate::Responder`] presents. Whoever holds
// the authority key, the responder itself or a separate issuer, uses the `sign` method to produce
// the message: a Schnorr signature over the version, the validity period and the responder's
// static public key. The [`crate::Initiator`] uses the `verify` method to check the signed
// message from the responder, comparing it against the provided public key and authority key,
// while ensuring the message falls within the specified validity period.

use core::convert::TryInto;
use core::fmt;

use secp256k1::{hashes::sha256, schnorr::Signature, Keypair, Message, Secp256k1, XOnlyPublicKey};

use crate::{CERTIFICATE_TIME_LEEWAY, CERTIFICATE_VERSION, SIGNATURE_NOISE_MESSAGE_SIZE};

// Size in bytes of the signed header: `version`, `valid_from` and `not_valid_after`.
const HEADER_SIZE: usize = 10;

/// `SignatureNoiseMessage` represents a signed message used in the Noise NX protocol
/// for authentication during the handshake process. It encapsulates the necessary
/// details for signature verification, including protocol versioning, validity periods,
/// and a Schnorr signature over the message.
///
/// This structure ensures that messages are authenticated and valid only within
/// a specified time window, using Schnorr signatures over the `secp256k1` elliptic curve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignatureNoiseMessage {
    /// Certificate format version; only [`crate::CERTIFICATE_VERSION`] verifies.
    pub version: u16,
    /// Start of the validity period for the message, expressed as a Unix timestamp.
    pub valid_from: u32,
    /// End of the validity period for the message, expressed as a Unix timestamp.
    pub not_valid_after: u32,
    /// 64-byte Schnorr signature that authenticates the message.
    pub signature: [u8; 64],
}

impl fmt::Display for SignatureNoiseMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // write header
        write!(
            f,
            "SignatureNoiseMessage {{ version: {}, valid_from: {}, not_valid_after: {}, signature: ",
            self.version, self.valid_from, self.not_valid_after
        )?;

        // write signature as hex manually
        for byte in &self.signature {
            write!(f, "{:02x}", byte)?;
        }

        write!(f, " }}")
    }
}
impl From<SignatureNoiseMessage> for [u8; SIGNATURE_NOISE_MESSAGE_SIZE] {
    /// Encodes a [`SignatureNoiseMessage`] in the 74-byte layout the handshake carries: the
    /// little-endian `version`, `valid_from` and `not_valid_after`, then the signature. The inverse
    /// of the conversion from bytes.
    fn from(value: SignatureNoiseMessage) -> Self {
        let mut encoded = [0; SIGNATURE_NOISE_MESSAGE_SIZE];
        encoded[..HEADER_SIZE].copy_from_slice(&value.header());
        encoded[HEADER_SIZE..].copy_from_slice(&value.signature);
        encoded
    }
}

impl From<[u8; SIGNATURE_NOISE_MESSAGE_SIZE]> for SignatureNoiseMessage {
    /// Decodes the 74-byte layout the handshake carries: the little-endian `version`, `valid_from`
    /// and `not_valid_after`, then the signature.
    fn from(value: [u8; SIGNATURE_NOISE_MESSAGE_SIZE]) -> Self {
        let version = u16::from_le_bytes(value[0..2].try_into().unwrap());
        let valid_from = u32::from_le_bytes(value[2..6].try_into().unwrap());
        let not_valid_after = u32::from_le_bytes(value[6..HEADER_SIZE].try_into().unwrap());
        let signature = value[HEADER_SIZE..].try_into().unwrap();
        Self {
            version,
            valid_from,
            not_valid_after,
            signature,
        }
    }
}

impl SignatureNoiseMessage {
    /// Verifies the [`SignatureNoiseMessage`] against the responder's static public key and the
    /// authority public key at the current system time. See [`Self::verify_with_now`].
    #[cfg(feature = "std")]
    pub fn verify(&self, static_pk: &XOnlyPublicKey, authority_pk: &XOnlyPublicKey) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as u32;
        self.verify_with_now(static_pk, authority_pk, now)
    }

    /// Verifies the validity and authenticity of the `SignatureNoiseMessage` at a given timestamp
    /// with [`CERTIFICATE_TIME_LEEWAY`] seconds of tolerance.
    ///
    /// The message must carry the supported certificate version, `now` must fall inside its
    /// validity window, and its signature over the responder's static key `static_pk` must verify
    /// against `authority_pk`.
    ///
    /// The current system time should be provided to avoid relying on `std` and allow `no_std`
    /// environments to use another source of time.
    #[inline]
    pub fn verify_with_now(
        &self,
        static_pk: &XOnlyPublicKey,
        authority_pk: &XOnlyPublicKey,
        now: u32,
    ) -> bool {
        if self.version != CERTIFICATE_VERSION {
            return false;
        }

        // Use saturating ops to cap edges (valid_from ≥ 0, not_valid_after ≤ u32::MAX),
        // preventing wrap-around and subtle validation bugs with untrusted timestamps.
        if self.valid_from.saturating_sub(CERTIFICATE_TIME_LEEWAY) > now
            || self.not_valid_after.saturating_add(CERTIFICATE_TIME_LEEWAY) < now
        {
            return false;
        }

        let secp = Secp256k1::verification_only();
        // m = SHA-256(version || valid_from || not_valid_after || server_static_key)
        let m = [&self.header()[..], &static_pk.serialize()].concat();
        let m = Message::from_hashed_data::<sha256::Hash>(&m);
        let s = match Signature::from_slice(&self.signature) {
            Ok(s) => s,
            _ => return false,
        };
        secp.verify_schnorr(&s, &m, authority_pk).is_ok()
    }

    /// Issues a [`SignatureNoiseMessage`] of the supported certificate version for the server's
    /// static public key (`static_pk`), valid between the two Unix timestamps and signed with the
    /// `authority` keypair.
    #[cfg(feature = "std")]
    pub fn sign(
        valid_from: u32,
        not_valid_after: u32,
        static_pk: &XOnlyPublicKey,
        authority: &Keypair,
    ) -> Self {
        Self::sign_with_rng(
            valid_from,
            not_valid_after,
            static_pk,
            authority,
            &mut rand::thread_rng(),
        )
    }

    /// Issues a [`SignatureNoiseMessage`] using a custom random number generator.
    ///
    /// See [`Self::sign`] for more details.
    ///
    /// The random number generator is used in the signature generation. It should be provided in
    /// order to not implicitely rely on `std` and allow `no_std` environments to provide a
    /// hardware random number generator for example.
    #[inline]
    pub fn sign_with_rng<R: rand::Rng + rand::CryptoRng>(
        valid_from: u32,
        not_valid_after: u32,
        static_pk: &XOnlyPublicKey,
        authority: &Keypair,
        rng: &mut R,
    ) -> Self {
        let mut message = Self {
            version: CERTIFICATE_VERSION,
            valid_from,
            not_valid_after,
            signature: [0; 64],
        };
        let secp = Secp256k1::signing_only();
        // m = SHA-256(version || valid_from || not_valid_after || server_static_key)
        let m = [&message.header()[..], &static_pk.serialize()].concat();
        let m = Message::from_hashed_data::<sha256::Hash>(&m);
        message.signature = *secp.sign_schnorr_with_rng(&m, authority, rng).as_ref();
        message
    }

    // The signed header: the little-endian `version`, `valid_from` and `not_valid_after`.
    fn header(&self) -> [u8; HEADER_SIZE] {
        let mut header = [0; HEADER_SIZE];
        header[0..2].copy_from_slice(&self.version.to_le_bytes());
        header[2..6].copy_from_slice(&self.valid_from.to_le_bytes());
        header[6..HEADER_SIZE].copy_from_slice(&self.not_valid_after.to_le_bytes());
        header
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn byte_encoding_round_trips() {
        let mut encoded = [0u8; SIGNATURE_NOISE_MESSAGE_SIZE];
        for (i, b) in encoded.iter_mut().enumerate() {
            *b = i as u8;
        }
        let message = SignatureNoiseMessage::from(encoded);
        assert_eq!(message.version, u16::from_le_bytes([0, 1]));
        assert_eq!(message.valid_from, u32::from_le_bytes([2, 3, 4, 5]));
        assert_eq!(message.not_valid_after, u32::from_le_bytes([6, 7, 8, 9]));
        assert_eq!(<[u8; SIGNATURE_NOISE_MESSAGE_SIZE]>::from(message), encoded);
    }

    #[test]
    #[cfg(feature = "std")]
    #[cfg_attr(miri, ignore)]
    fn verify_rejects_unsupported_certificate_version() {
        use rand::{rngs::StdRng, SeedableRng};

        let secp = Secp256k1::new();
        let mut rng = StdRng::seed_from_u64(42);
        let authority = Keypair::new(&secp, &mut rng);
        let static_pk = Keypair::new(&secp, &mut rng).x_only_public_key().0;

        let mut certificate =
            SignatureNoiseMessage::sign_with_rng(100, 200, &static_pk, &authority, &mut rng);
        assert!(certificate.verify_with_now(&static_pk, &authority.x_only_public_key().0, 150));

        // The same certificate, validly signed for a version this crate does not support.
        certificate.version = CERTIFICATE_VERSION + 1;
        let m = [&certificate.header()[..], &static_pk.serialize()].concat();
        let m = Message::from_hashed_data::<sha256::Hash>(&m);
        certificate.signature = *secp
            .sign_schnorr_with_rng(&m, &authority, &mut rng)
            .as_ref();
        assert!(!certificate.verify_with_now(&static_pk, &authority.x_only_public_key().0, 150));
    }
}
