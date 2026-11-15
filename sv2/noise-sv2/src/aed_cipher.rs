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

use aead::{Buffer, Error};
use chacha20_poly1305::{Key, Nonce};
use zeroize::Zeroize;

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

// Noise transport cipher backed by the `chacha20-poly1305` crate.
//
// The crate builds a fresh AEAD per (key, nonce) pair, so this wrapper only holds the 32-byte key
// and wipes it on drop.
pub struct ChaCha20Poly1305 {
    key: [u8; 32],
}

impl Drop for ChaCha20Poly1305 {
    fn drop(&mut self) {
        self.key.zeroize();
    }
}

// Length of the Poly1305 authentication tag appended to every ciphertext.
const TAG_LEN: usize = 16;

impl AeadCipher for ChaCha20Poly1305 {
    fn from_key(k: [u8; 32]) -> Self {
        Self { key: k }
    }

    fn encrypt<T: Buffer>(
        &mut self,
        nonce: &[u8; 12],
        ad: &[u8],
        data: &mut T,
    ) -> Result<(), Error> {
        let aead = chacha20_poly1305::ChaCha20Poly1305::new(Key::new(self.key), Nonce::new(*nonce));
        let tag = aead.encrypt(data.as_mut(), Some(ad));
        data.extend_from_slice(&tag)
    }

    fn decrypt<T: Buffer>(
        &mut self,
        nonce: &[u8; 12],
        ad: &[u8],
        data: &mut T,
    ) -> Result<(), Error> {
        let msg_len = data.len().checked_sub(TAG_LEN).ok_or(Error)?;
        let mut tag = [0u8; TAG_LEN];
        tag.copy_from_slice(&data.as_ref()[msg_len..]);
        let aead = chacha20_poly1305::ChaCha20Poly1305::new(Key::new(self.key), Nonce::new(*nonce));
        aead.decrypt(&mut data.as_mut()[..msg_len], tag, Some(ad))
            .map_err(|_| Error)?;
        data.truncate(msg_len);
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    // RFC 8439 section 2.8.2 AEAD test vector.
    #[test]
    fn rfc8439_vector_roundtrip() {
        let key: [u8; 32] = core::array::from_fn(|i| 0x80 + i as u8);
        let nonce = [
            0x07, 0, 0, 0, 0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47,
        ];
        let ad = [
            0x50, 0x51, 0x52, 0x53, 0xc0, 0xc1, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7,
        ];
        let plaintext = b"Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.";
        let expected_tag = [
            0x1a, 0xe1, 0x0b, 0x59, 0x4f, 0x09, 0xe2, 0x6a, 0x7e, 0x90, 0x2e, 0xcb, 0xd0, 0x60,
            0x06, 0x91,
        ];

        let mut cipher = ChaCha20Poly1305::from_key(key);
        let mut data = plaintext.to_vec();
        cipher.encrypt(&nonce, &ad, &mut data).unwrap();
        assert_eq!(data.len(), plaintext.len() + TAG_LEN);
        assert_eq!(&data[..2], &[0xd3, 0x1a]);
        assert_eq!(&data[plaintext.len()..], &expected_tag);

        cipher.decrypt(&nonce, &ad, &mut data).unwrap();
        assert_eq!(&data[..], &plaintext[..]);

        let mut short = alloc::vec![0u8; TAG_LEN - 1];
        assert!(cipher.decrypt(&nonce, &ad, &mut short).is_err());
    }
}
