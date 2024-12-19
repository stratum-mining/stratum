// # Error Handling
//
// Defines error types and utilities for handling errors in the `noise_sv2` module.

use alloc::vec::Vec;

use aes_gcm::Error as AesGcm;

/// Noise protocol error handling.
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// The handshake has not been completed when a finalization step is executed.
    HandshakeNotFinalized,

    /// Error on an empty cipher list is provided where one is required.
    CipherListMustBeNonEmpty,

    /// Error on unsupported ciphers.
    UnsupportedCiphers(Vec<u8>),

    /// Provided cipher list is invalid or malformed.
    InvalidCipherList(Vec<u8>),

    /// Chosen cipher is invalid or unsupported.
    InvalidCipherChosed(Vec<u8>),

    /// Wraps AES-GCM errors during encryption/decryption.
    AesGcm(AesGcm),

    /// Cipher is in an invalid state during encryption/decryption operations.
    InvalidCipherState,

    /// Provided certificate is invalid or cannot be verified.
    InvalidCertificate([u8; 74]),

    /// A raw public key is invalid or cannot be parsed.
    InvalidRawPublicKey,

    /// A raw private key is invalid or cannot be parsed.
    InvalidRawPrivateKey,

    /// An incoming handshake message is expected but not received.
    ExpectedIncomingHandshakeMessage,

    /// A message has an incorrect or unexpected length.
    InvalidMessageLength,
}

impl From<AesGcm> for Error {
    fn from(value: AesGcm) -> Self {
        Self::AesGcm(value)
    }
}
