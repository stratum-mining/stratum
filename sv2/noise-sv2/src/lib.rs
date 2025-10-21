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
//! - Schnorr Signatures: Authenticates messages and verifies the identity of the Sv2 roles. In
//!   practice, the primitives exposed by this crate should be used to secure communication channels
//!   between Sv2 roles. Securing communication between two Sv2 roles on the same local network
//!   (e.g., local mining devices communicating with a local mining proxy) is optional. However, it
//!   is mandatory to secure the communication between two Sv2 roles communicating over a remote
//!   network (e.g., a local mining proxy communicating with a remote pool sever).
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

/// Size of the MAC for supported AEAD encryption algorithm (ChaChaPoly).
pub const AEAD_MAC_LEN: usize = 16;

/// Size of the Noise protocol frame header in bytes.
pub const NOISE_FRAME_HEADER_SIZE: usize = 2;

/// Size in bytes of the SIGNATURE_NOISE_MESSAGE, which contains information and
/// a signature for the handshake initiator, formatted according to the Noise
/// Protocol specifications.
pub const SIGNATURE_NOISE_MESSAGE_SIZE: usize = 74;

/// Size in bytes of the encrypted signature noise message, which includes the
/// SIGNATURE_NOISE_MESSAGE and a MAC for integrity verification.
pub const ENCRYPTED_SIGNATURE_NOISE_MESSAGE_SIZE: usize =
    SIGNATURE_NOISE_MESSAGE_SIZE + AEAD_MAC_LEN;

/// Size in bytes of the encoded elliptic curve point using ElligatorSwift
/// encoding. This encoding produces a 64-byte representation of the
/// X-coordinate of a secp256k1 curve point.
pub const ELLSWIFT_ENCODING_SIZE: usize = 64;

/// Size in bytes of the encrypted ElligatorSwift encoded data, which includes
/// the original ElligatorSwift encoded data and a MAC for integrity
/// verification.
pub const ENCRYPTED_ELLSWIFT_ENCODING_SIZE: usize = ELLSWIFT_ENCODING_SIZE + AEAD_MAC_LEN;

/// Size in bytes of the handshake message expected by the initiator,
/// encompassing:
/// - ElligatorSwift encoded public key
/// - Encrypted ElligatorSwift encoding
/// - Encrypted SIGNATURE_NOISE_MESSAGE
pub const INITIATOR_EXPECTED_HANDSHAKE_MESSAGE_SIZE: usize = ELLSWIFT_ENCODING_SIZE
    + ENCRYPTED_ELLSWIFT_ENCODING_SIZE
    + ENCRYPTED_SIGNATURE_NOISE_MESSAGE_SIZE;

/// If protocolName is less than or equal to 32 bytes in length, use
/// protocolName with zero bytes appended to make 32 bytes. Otherwise, apply
/// HASH to it. For name = "Noise_NX_Secp256k1+EllSwift_ChaChaPoly_SHA256", we
/// need the hash. More info can be found [at this link](https://github.com/stratum-mining/sv2-spec/blob/main/04-Protocol-Security.md#451-handshake-act-1-nx-handshake-part-1---e).
pub const NOISE_HASHED_PROTOCOL_NAME_CHACHA: [u8; 32] = [
    46, 180, 120, 129, 32, 142, 158, 238, 31, 102, 159, 103, 198, 110, 231, 14, 169, 234, 136, 9,
    13, 80, 63, 232, 48, 220, 75, 200, 62, 41, 191, 16,
];

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
#[derive(Clone)]
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
