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
// a Schnorr signature over the version, the validity period and the responder's static public
// key. The [`crate::Initiator`] uses the `verify` method to check the validity of the signed
// message from the responder, comparing it against the provided public key and optional authority
// key, while ensuring the message falls within the specified validity period.

use core::convert::TryInto;
use core::fmt;

use secp256k1::{hashes::sha256, schnorr::Signature, Keypair, Message, Secp256k1, XOnlyPublicKey};

/// `SignatureNoiseMessage` represents a signed message used in the Noise NX protocol
/// for authentication during the handshake process. It encapsulates the necessary
/// details for signature verification, including protocol versioning, validity periods,
/// and a Schnorr signature over the message.
///
/// This structure ensures that messages are authenticated and valid only within
/// a specified time window, using Schnorr signatures over the `secp256k1` elliptic curve.
#[derive(Debug, PartialEq, Eq)]
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
impl From<SignatureNoiseMessage> for [u8; 74] {
    /// Encodes a [`SignatureNoiseMessage`] in the 74-byte layout the handshake carries: the
    /// little-endian `version`, `valid_from` and `not_valid_after`, then the signature. The inverse
    /// of the conversion from bytes.
    fn from(value: SignatureNoiseMessage) -> Self {
        let (header, signature) = value.split();
        let mut encoded = [0; 74];
        encoded[0..10].copy_from_slice(&header);
        encoded[10..74].copy_from_slice(&signature);
        encoded
    }
}

impl From<[u8; 74]> for SignatureNoiseMessage {
    /// Decodes the 74-byte layout the handshake carries: the little-endian `version`, `valid_from`
    /// and `not_valid_after`, then the signature.
    fn from(value: [u8; 74]) -> Self {
        let version = u16::from_le_bytes(value[0..2].try_into().unwrap());
        let valid_from = u32::from_le_bytes(value[2..6].try_into().unwrap());
        let not_valid_after = u32::from_le_bytes(value[6..10].try_into().unwrap());
        let signature = value[10..74].try_into().unwrap();
        Self {
            version,
            valid_from,
            not_valid_after,
            signature,
        }
    }
}

impl SignatureNoiseMessage {
    /// Verifies the [`SignatureNoiseMessage`] against the provided public key and an optional
    /// authority public key at the current system time. See [`Self::verify_with_now`].
    #[cfg(feature = "std")]
    pub fn verify(self, pk: &XOnlyPublicKey, authority_pk: &Option<XOnlyPublicKey>) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as u32;
        self.verify_with_now(pk, authority_pk, now)
    }

    /// Verifies the validity and authenticity of the `SignatureNoiseMessage` at a given timestamp
    /// with [`crate::CERTIFICATE_TIME_LEEWAY`] seconds of tolerance.
    ///
    /// With an authority public key, the message must carry the supported certificate version,
    /// `now` must fall inside its validity window, and its signature over the responder's static
    /// key `pk` must verify against the authority key.
    ///
    /// # Security
    ///
    /// Without an authority public key, only the certificate version is checked and the message is
    /// otherwise accepted unverified, so `true` does not mean the responder was authenticated.
    ///
    /// The current system time should be provided to avoid relying on `std` and allow `no_std`
    /// environments to use another source of time.
    #[inline]
    pub fn verify_with_now(
        self,
        pk: &XOnlyPublicKey,
        authority_pk: &Option<XOnlyPublicKey>,
        now: u32,
    ) -> bool {
        use crate::CERTIFICATE_TIME_LEEWAY as TIME_LEEWAY;

        if self.version != crate::CERTIFICATE_VERSION {
            return false;
        }

        if let Some(authority_pk) = authority_pk {
            // Use saturating ops to cap edges (valid_from ≥ 0, not_valid_after ≤ u32::MAX),
            // preventing wrap-around and subtle validation bugs with untrusted timestamps.
            if self.valid_from.saturating_sub(TIME_LEEWAY) <= now
                && self.not_valid_after.saturating_add(TIME_LEEWAY) >= now
            {
                let secp = Secp256k1::verification_only();
                let (m, s) = self.split();
                // m = SHA-256(version || valid_from || not_valid_after || server_static_key)
                let m = [&m[0..10], &pk.serialize()].concat();
                let m = Message::from_hashed_data::<sha256::Hash>(&m);
                let s = match Signature::from_slice(&s) {
                    Ok(s) => s,
                    _ => return false,
                };
                secp.verify_schnorr(&s, &m, authority_pk).is_ok()
            } else {
                false
            }
        } else {
            true
        }
    }

    /// Signs a [`SignatureNoiseMessage`] using the provided keypair (`kp`).
    ///
    /// `msg` holds the encoded message, as produced by the conversion into `[u8; 74]`; its first 10
    /// bytes (version and validity period) are signed together with the static public key of the
    /// server (`static_pk`), and the resulting signature is written into its remaining 64 bytes.
    #[cfg(feature = "std")]
    pub fn sign(msg: &mut [u8; 74], static_pk: &XOnlyPublicKey, kp: &Keypair) {
        Self::sign_with_rng(msg, static_pk, kp, &mut rand::thread_rng());
    }

    /// Signs a [`SignatureNoiseMessage`] using the provided keypair (`kp`) and a custom random
    /// number generator.
    ///
    /// See [`Self::sign`] for more details.
    ///
    /// The random number generator is used in the signature generation. It should be provided in
    /// order to not implicitely rely on `std` and allow `no_std` environments to provide a
    /// hardware random number generator for example.
    #[inline]
    pub fn sign_with_rng<R: rand::Rng + rand::CryptoRng>(
        msg: &mut [u8; 74],
        static_pk: &XOnlyPublicKey,
        kp: &Keypair,
        rng: &mut R,
    ) {
        let secp = Secp256k1::signing_only();
        let m = [&msg[0..10], &static_pk.serialize()].concat();
        let m = Message::from_hashed_data::<sha256::Hash>(&m);
        let signature = secp.sign_schnorr_with_rng(&m, kp, rng);
        for (i, b) in signature.as_ref().iter().enumerate() {
            msg[10 + i] = *b;
        }
    }

    // Splits the [`SignatureNoiseMessage`] into its component parts: the message hash and the
    // signature.
    //
    // Separates the message into the first 10 bytes (containing the version and validity period)
    // and the 64-byte Schnorr signature, returning them in a tuple. Used internally during the
    // verification process.
    fn split(self) -> ([u8; 10], [u8; 64]) {
        let mut m = [0; 10];
        m[0] = self.version.to_le_bytes()[0];
        m[1] = self.version.to_le_bytes()[1];
        m[2] = self.valid_from.to_le_bytes()[0];
        m[3] = self.valid_from.to_le_bytes()[1];
        m[4] = self.valid_from.to_le_bytes()[2];
        m[5] = self.valid_from.to_le_bytes()[3];
        m[6] = self.not_valid_after.to_le_bytes()[0];
        m[7] = self.not_valid_after.to_le_bytes()[1];
        m[8] = self.not_valid_after.to_le_bytes()[2];
        m[9] = self.not_valid_after.to_le_bytes()[3];
        (m, self.signature)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn byte_encoding_round_trips() {
        let mut encoded = [0u8; 74];
        for (i, b) in encoded.iter_mut().enumerate() {
            *b = i as u8;
        }
        let message = SignatureNoiseMessage::from(encoded);
        assert_eq!(message.version, u16::from_le_bytes([0, 1]));
        assert_eq!(message.valid_from, u32::from_le_bytes([2, 3, 4, 5]));
        assert_eq!(message.not_valid_after, u32::from_le_bytes([6, 7, 8, 9]));
        assert_eq!(<[u8; 74]>::from(message), encoded);
    }

    #[test]
    #[cfg(feature = "std")]
    #[cfg_attr(miri, ignore)]
    fn verify_rejects_unsupported_certificate_version() {
        use rand::{rngs::StdRng, SeedableRng};

        let secp = Secp256k1::new();
        let mut rng = StdRng::seed_from_u64(42);
        let authority = Keypair::new(&secp, &mut rng);
        let responder_static = Keypair::new(&secp, &mut rng);

        let mut encoded = [0u8; 74];
        encoded[0..2].copy_from_slice(&(crate::CERTIFICATE_VERSION + 1).to_le_bytes());
        encoded[2..6].copy_from_slice(&100u32.to_le_bytes());
        encoded[6..10].copy_from_slice(&200u32.to_le_bytes());
        SignatureNoiseMessage::sign_with_rng(
            &mut encoded,
            &responder_static.x_only_public_key().0,
            &authority,
            &mut rng,
        );

        let certificate = SignatureNoiseMessage::from(encoded);
        let authority_pk = Some(authority.x_only_public_key().0);
        assert!(!certificate.verify_with_now(
            &responder_static.x_only_public_key().0,
            &authority_pk,
            150,
        ));
    }
}
