//! # Noise-SV2: Noise Protocol Implementation for Stratum V2
//!
//! `noise_sv2` ensures secure communication between Sv2 roles by handling encryption, decryption,
//! and authentication through Noise protocol handshakes and cipher operations.
//!
//! Implementation of the [Sv2 Noise protocol specification](https://github.com/stratum-mining/sv2-spec/blob/main/04-Protocol-Security.md#4-protocol-security).
//!
//! ## Features
//! - Noise Protocol: Establishes secure communication via the Noise protocol handshake between the
//!   [`Initiator`] and [`Responder`] roles.
//! - Diffie-Hellman with [`secp256k1`]: Securely establishes a shared secret between two Sv2 roles,
//!   using the same elliptic curve used in Bitcoin.
//! - AEAD: Ensures confidentiality and integrity of the data.
//! - `AES-GCM` and `ChaCha20-Poly1305`: Provides encryption, with hardware-optimized and
//!   software-optimized options.
//! - Schnorr Signatures: Authenticates messages and verifies the identity of the Sv2 roles.
//! In practice, the primitives exposed by this crate should be used to secure communication
//! channels between Sv2 roles. Securing communication between two Sv2 roles on the same local
//! network (e.g., local mining devices communicating with a local mining proxy) is optional.
//! However, it is mandatory to secure the communication between two Sv2 roles communicating over a
//! remote network (e.g., a local mining proxy communicating with a remote pool sever).
//!
//! The Noise protocol establishes secure communication between two Sv2 roles via a handshake
//! performed at the beginning of the connection. The initiator (e.g., a local mining proxy) and
//! the responder (e.g., a mining pool) establish a shared secret using Elliptic Curve
//! Diffie-Hellman (ECDH) with the [`secp256k1`] elliptic curve (the same elliptic curve used by
//! Bitcoin). Once both Sv2 roles compute the shared secret from the ECDH exchange, the Noise
//! protocol derives symmetric encryption keys for secure communication. These keys are used with
//! AEAD (using either `AES-GCM` or `ChaCha20-Poly1305`) to encrypt and authenticate all
//! communication between the roles. This encryption ensures that sensitive data, such as share
//! submissions, remains confidential and tamper-resistant. Additionally, Schnorr signatures are
//! used to authenticate messages and validate the identities of the Sv2 roles, ensuring that
//! critical messages like job templates and share submissions originate from legitimate sources.

#![cfg_attr(all(not(feature = "std"), not(test)), no_std)]

#[macro_use]
extern crate alloc;

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

// The parity value used in the Schnorr signature process.
//
// Used to define whether a public key corresponds to an even or odd point on the elliptic curve.
// In this case, `Parity::Even` is used.
const PARITY: secp256k1::Parity = secp256k1::Parity::Even;

/// A codec for managing encrypted communication in the Noise protocol.
///
/// Manages the encryption and decryption of messages between two parties, the [`Initiator`] and
/// [`Responder`], using the Noise protocol. A symmetric cipher is used for both encrypting
/// outgoing messages and decrypting incoming messages.
pub struct NoiseCodec {
    // Cipher to encrypt outgoing messages.
    encryptor: GenericCipher,

    // Cipher to decrypt incoming messages.
    decryptor: GenericCipher,
}

impl core::fmt::Debug for NoiseCodec {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
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
