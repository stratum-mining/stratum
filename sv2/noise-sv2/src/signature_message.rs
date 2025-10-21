// # Signature-Based Message Handling
//
// Defines the [`SignatureNoiseMessage`] struct, which represents a signed message used in the
// Noise protocol to authenticate and verify the identity of a party during the handshake.
//
// This module provides utilities for creating, signing, and verifying Noise protocol messages
// using Schnorr signatures over the [`secp256k1`] elliptic curve. It encapsulates signed messages
// along with versioning and validity timestamps. The following capabilities are supported:
//
// - Conversion of raw byte arrays into structured [`SignatureNoiseMessage`] instances.
// - Message signing using Schnorr signatures and the [`secp256k1`] curve.
// - Verification of signed messages, ensuring they fall within valid time periods and are signed by
//   an authorized public key.
//
// ## Usage
//
// The [`SignatureNoiseMessage`] is used by both the [`crate::Responder`] and [`crate::Initiator`]
// roles. The [`crate::Responder`] uses the `sign` method to generate a Schnorr signature over the
// initial message sent by the initiator. The [`crate::Initiator`] uses the `verify` method to
// check the validity of the signed message from the responder, comparing it against the provided
// public key and optional authority key, while ensuring the message falls within the specified
// validity period.

use core::convert::TryInto;

use secp256k1::{hashes::sha256, schnorr::Signature, Keypair, Message, Secp256k1, XOnlyPublicKey};

/// `SignatureNoiseMessage` represents a signed message used in the Noise NX protocol
/// for authentication during the handshake process. It encapsulates the necessary
/// details for signature verification, including protocol versioning, validity periods,
/// and a Schnorr signature over the message.
///
/// This structure ensures that messages are authenticated and valid only within
/// a specified time window, using Schnorr signatures over the `secp256k1` elliptic curve.
pub struct SignatureNoiseMessage {
    // Version of the protocol being used.
    pub version: u16,
    // Start of the validity period for the message, expressed as a Unix timestamp.
    pub valid_from: u32,
    // End of the validity period for the message, expressed as a Unix timestamp.
    pub not_valid_after: u32,
    // 64-byte Schnorr signature that authenticates the message.
    pub signature: [u8; 64],
}

impl From<[u8; 74]> for SignatureNoiseMessage {
    // Converts a 74-byte array into a [`SignatureNoiseMessage`].
    //
    // Allows a raw 74-byte array to be converted into a [`SignatureNoiseMessage`], extracting the
    // version, validity periods, and signature from the provided data. Panics if the byte array
    // cannot be correctly converted into the struct fields.
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
    // Verifies the [`SignatureNoiseMessage`] against the provided public key and an optional
    // authority public key. The verification checks that the message is currently valid
    // (i.e., within the `valid_from` and `not_valid_after` time window) and that the signature
    // is correctly signed by the authority.
    //
    // If an authority public key is not provided, the function assumes that the signature
    // is already valid without further verification.
    #[allow(dead_code)]
    #[cfg(feature = "std")]
    pub fn verify(self, pk: &XOnlyPublicKey, authority_pk: &Option<XOnlyPublicKey>) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as u32;
        self.verify_with_now(pk, authority_pk, now)
    }

    /// Verifies the validity and authenticity of the `SignatureNoiseMessage` at a given timestamp.
    ///
    /// See [`Self::verify`] for more details.
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
        if let Some(authority_pk) = authority_pk {
            if self.valid_from <= now && self.not_valid_after >= now {
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

    // Signs a [`SignatureNoiseMessage`] using the provided keypair (`kp`).
    //
    // Creates a Schnorr signature for the message, combining the version, validity period, and
    // the static public key of the server (`static_pk`). The resulting signature is then written
    // into the provided message buffer (`msg`).
    #[allow(dead_code)]
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
