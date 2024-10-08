//! # Noise-SV2: Noise Protocol Implementation for Stratum V2
//!
//! Implementation of the [Sv2 Noise protocol specification](https://github.com/stratum-mining/sv2-spec/blob/main/04-Protocol-Security.md#4-protocol-security).
//!
//! `noise_sv2` ensures secure communication between downstream and upstream roles by handling
//! encryption, decryption, and authentication through Noise protocol handshakes and cipher
//! operations.
//!
//! ## Features
//! * Secure communication between downstream and upstream roles using the Noise protocol.
//! * Support for two ciphers: AES-GCM and ChaCha20-Poly1305.
//! * Implements the [`Initiator`] and [`Responder`] roles for the Noise handshake.
//! * Includes helper types for managing cryptographic state and cipher operations.
//!
//! ## Usage
//!
//! The crate is designed to be used as part of the Stratum V2 mining protocol to secure
//! connections between downstream and upstream roles. This includes performing Noise handshakes,
//! encrypting messages, and decrypting responses.

use aes_gcm::aead::Buffer;
pub use aes_gcm::aead::Error as AeadError;
use cipher_state::GenericCipher;
mod aed_cipher;
mod cipher_state;
mod error;
mod handshake;
mod initiator;
mod responder;
mod signature_message;
#[cfg(test)]
mod test;

pub use const_sv2::{NOISE_HASHED_PROTOCOL_NAME_CHACHA, NOISE_SUPPORTED_CIPHERS_MESSAGE};

/// The parity value used in the Schnorr signature process.
///
/// Used to define whether a public key corresponds to an even or odd point on the elliptic curve.
/// In this case, `Parity::Even` is used.
const PARITY: secp256k1::Parity = secp256k1::Parity::Even;

/// A codec for managing encrypted communication in the Noise protocol.
///
/// Manages the encryption and decryption of messages between two parties, the [`Initiator`] and
/// [`Responder`], using the Noise protocol. A symmetric cipher is used for both encrypting
/// outgoing messages and decrypting incoming messages.
pub struct NoiseCodec {
    /// Cipher to encrypt outgoing messages.
    encryptor: GenericCipher,

    /// Cipher to decrypt incoming messages.
    decryptor: GenericCipher,
}

impl std::fmt::Debug for NoiseCodec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NoiseCodec").finish()
    }
}

impl NoiseCodec {
    /// Encrypts a message (`msg`) in place using the stored cipher.
    pub fn encrypt<T: Buffer>(&mut self, msg: &mut T) -> Result<(), aes_gcm::Error> {
        self.encryptor.encrypt(msg)
    }

    /// Decrypts a message (`msg`) in place using the stored cipher.
    pub fn decrypt<T: Buffer>(&mut self, msg: &mut T) -> Result<(), aes_gcm::Error> {
        self.decryptor.decrypt(msg)
    }
}

pub use error::Error;
pub use initiator::Initiator;
pub use responder::Responder;
