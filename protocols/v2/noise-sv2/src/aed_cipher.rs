// # AEAD Cipher
//
// Defines the [`AeadCipher`] trait which standardizes the interface for Authenticated Encryption
// with Associated Data (AEAD) ciphers used within the Noise protocol implementation. The trait is
// implemented for two common ciphers, [`ChaCha20Poly1305`] and [`Aes256Gcm`], providing encryption
// and decryption functionality with authenticated data.
//
// ## Overview
//
// AEAD ciphers provide both confidentiality and integrity by encrypting data and creating an
// authentication tag in a single step. The integrity of additional associated data (AAD) can also
// be verified without including it in the encrypted output.
//
// ## Usage
//
// The [`AeadCipher`] trait is used by the [`crate::handshake::HandshakeOp`] trait to perform
// cryptographic operations during the Noise protocol handshake, ensuring secure communication
// between parties.

use aes_gcm::Aes256Gcm;
use chacha20poly1305::{aead::Buffer, AeadInPlace, ChaCha20Poly1305, ChaChaPoly1305, KeyInit};

// Defines the interface for AEAD ciphers.
//
// The [`AeadCipher`] trait provides a standard interface for initializing AEAD ciphers, and for
// performing encryption and decryption operations with additional Authenticated Associated Data (AAD). This trait is implemented
// by either the [`ChaCha20Poly1305`] or [`Aes256Gcm`] specific cipher types, allowing them to be
// used interchangeably in cryptographic protocols. It is utilized by the
// [`crate::handshake::HandshakeOp`] trait to secure the handshake process.
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
    ) -> Result<(), aes_gcm::Error>;

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
    ) -> Result<(), aes_gcm::Error>;
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
    ) -> Result<(), aes_gcm::Error> {
        self.encrypt_in_place(nonce.into(), ad, data)
    }

    fn decrypt<T: Buffer>(
        &mut self,
        nonce: &[u8; 12],
        ad: &[u8],
        data: &mut T,
    ) -> Result<(), aes_gcm::Error> {
        self.decrypt_in_place(nonce.into(), ad, data)
    }
}

impl AeadCipher for Aes256Gcm {
    fn from_key(k: [u8; 32]) -> Self {
        Aes256Gcm::new(&k.into())
    }

    fn encrypt<T: Buffer>(
        &mut self,
        nonce: &[u8; 12],
        ad: &[u8],
        data: &mut T,
    ) -> Result<(), aes_gcm::Error> {
        self.encrypt_in_place(nonce.into(), ad, data)
    }

    fn decrypt<T: Buffer>(
        &mut self,
        nonce: &[u8; 12],
        ad: &[u8],
        data: &mut T,
    ) -> Result<(), aes_gcm::Error> {
        self.decrypt_in_place(nonce.into(), ad, data)
    }
}
