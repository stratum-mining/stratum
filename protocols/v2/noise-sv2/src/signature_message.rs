// # Signature-Based Messages Handling
//
// Defines the [`SignatureNoiseMessage`] struct, which represents a signed message used in the
// Noise protocol to authenticate and verify the identity of a party during the handshake. It
// includes functionality for signing and verifying messages using Schnorr signatures with the
// [`secp256k1`] elliptic curve.
//
// ## Usage
//
// The [`SignatureNoiseMessage`] is used in both the [`crate::Responder`] and [`crate::Initiator`]
// during the Noise handshake to ensure that messages are authentic and have not been tampered
// with.

use secp256k1::{hashes::sha256, schnorr::Signature, Keypair, Message, Secp256k1, XOnlyPublicKey};
use std::{convert::TryInto, time::SystemTime};

// [`SignatureNoiseMessage`] is a structure that encapsulates a cryptographic signature message
// used in the Noise NX protocol. This message includes versioning and timestamp information,
// ensuring that the signature is valid only within a specific time window.
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
    pub fn verify(self, pk: &XOnlyPublicKey, authority_pk: &Option<XOnlyPublicKey>) -> bool {
        if let Some(authority_pk) = authority_pk {
            let now = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs() as u32;
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
    pub fn sign(msg: &mut [u8; 74], static_pk: &XOnlyPublicKey, kp: &Keypair) {
        let secp = Secp256k1::signing_only();
        let m = [&msg[0..10], &static_pk.serialize()].concat();
        let m = Message::from_hashed_data::<sha256::Hash>(&m);
        let signature = secp.sign_schnorr(&m, kp);
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
