// # AEAD Cipher
//
// Abstracts the encryption and decryption operations for authenticated encryption with associated
// data (AEAD) ciphers used in the Noise protocol.
//
// The [`AeadCipher`] trait provides the interface used by the Noise protocol's
// [`ChaCha20Poly1305`] cipher.
//
// The trait supports core AEAD operations, including:
//
// - Key initialization via the `from_key` method to derive a cipher instance from a 32-byte key.
// - Authenticated encryption via the `encrypt` method to securely encrypt data with a nonce and
//   additional associated data (AAD).
// - Authenticated decryption via the `decrypt` method to securely decrypt data using the provided
//   nonce and AAD.
//
// ## Usage
//
// The `AeadCipher` trait can be implemented for an AEAD cipher used to encrypt and decrypt Noise
// protocol messages. This crate provides an implementation for [`ChaCha20Poly1305`].

use chacha20poly1305::{
    aead::{Buffer, Error},
    AeadInPlace, ChaCha20Poly1305, ChaChaPoly1305, KeyInit,
};

// Defines the interface for AEAD ciphers.
//
// The [`AeadCipher`] trait provides a standard interface for initializing AEAD ciphers, and for
// performing encryption and decryption operations with additional Authenticated Associated Data
// (AAD). It is utilized by the [`crate::handshake::HandshakeOp`] trait to secure the handshake
// process.
//
// The `T: Buffer` represents the data buffer to be encrypted or decrypted. The buffer must
// implement the [`Buffer`] trait, which provides necessary operations for in-place encryption and
// decryption.
pub trait AeadCipher {
    // Creates a new instance of the cipher from a 32-byte key.
    //
    // Initializes the AEAD cipher with the provided key (`k`), preparing it for
    // encryption and decryption operations.
    fn from_key(k: [u8; 32]) -> Self;

    // Encrypts the data in place using the provided 12-byte `nonce` and AAD (`ad`).
    //
    // Performs authenticated encryption on the provided mutable data buffer (`data`), modifying
    // it in place to contain the ciphertext. The encryption is performed using the provided nonce
    // and AAD, which ensures that the data has not been tampered with during transit.
    fn encrypt<T: Buffer>(
        &mut self,
        nonce: &[u8; 12],
        ad: &[u8],
        data: &mut T,
    ) -> Result<(), Error>;

    // Decrypts the data in place using the provided 12-byte nonce (`n`) and AAD (`ad`).
    //
    // Performs authenticated decryption on the provided mutable data buffer, modifying it in
    // place to contain the plaintext. The decryption is performed using the provided nonce and
    // AAD, ensuring that the data has not been tampered with during transit.
    fn decrypt<T: Buffer>(
        &mut self,
        nonce: &[u8; 12],
        ad: &[u8],
        data: &mut T,
    ) -> Result<(), Error>;
}

impl AeadCipher for ChaCha20Poly1305 {
    fn from_key(k: [u8; 32]) -> Self {
        ChaChaPoly1305::new(&k.into())
    }

    fn encrypt<T: Buffer>(
        &mut self,
        nonce: &[u8; 12],
        ad: &[u8],
        data: &mut T,
    ) -> Result<(), Error> {
        self.encrypt_in_place(nonce.into(), ad, data)
    }

    fn decrypt<T: Buffer>(
        &mut self,
        nonce: &[u8; 12],
        ad: &[u8],
        data: &mut T,
    ) -> Result<(), Error> {
        self.decrypt_in_place(nonce.into(), ad, data)
    }
}
