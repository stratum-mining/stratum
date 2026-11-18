// # Cipher State Management
//
// Defines the [`CipherState`] trait and [`Cipher`] type, which manage the state of the AEAD cipher
// used in the Noise protocol. This includes managing the encryption key, nonce, and cipher
// instance itself, facilitating secure encryption and decryption during communication.
//
// The [`CipherState`] trait abstracts the management of core elements for AEAD ciphers:
// - Manages the encryption key lifecycle used by the AEAD cipher.
// - Generates and tracks unique nonces for each encryption operation, preventing replay attacks.
// - Initializes the [`ChaCha20Poly1305`] cipher for secure communication.
//
// The trait provides methods for encrypting and decrypting data using additional associated data
// (AAD) and securely erasing sensitive cryptographic material when no longer needed.
//
// ## Usage
//
// The [`CipherState`] trait is used by the [`crate::handshake::HandshakeOp`] trait to manage
// stateful encryption and decryption tasks during the Noise protocol handshake. By implementing
// [`CipherState`], the handshake process securely manages cryptographic material and transforms
// messages exchanged between the initiator and responder.
//
// Once the Noise handshake is complete, the [`crate::Initiator`] and [`crate::Responder`] use
// [`Cipher`] instances to perform symmetric encryption and decryption. These ciphers, initialized
// and managed through the [`CipherState`] trait, ensure ongoing communication remains confidential
// and authenticated.
//
// The [`CipherState`] trait and [`Cipher`] type manage secure data handling, key management, and
// nonce tracking throughout the communication session.

use crate::aed_cipher::AeadCipher;
use aead::{Buffer, Error};
use zeroize::Zeroize;

// The `CipherState` trait manages AEAD ciphers for secure communication, handling the encryption
// key, nonce, and cipher instance, ensuring proper key and nonce management.
//
// Key responsibilities:
// - **Key management**: Set and retrieve the 32-byte encryption key.
// - **Nonce management**: Track unique nonces for encryption operations.
// - **Cipher handling**: Initialize and manage AEAD ciphers for secure data encryption.
//
// Used in protocols like Noise, `CipherState` ensures secure communication by managing
// cryptographic material during and after handshakes.
pub trait CipherState<Cipher_: AeadCipher>
where
    Self: Sized,
{
    // Retrieves a mutable reference to the 32-byte encryption key (`k`).
    fn get_k(&mut self) -> &mut Option<[u8; 32]>;

    // Sets the 32-byte encryption key to the optionally provided value (`k`).
    //
    // Allows the encryption key to be explicitly set, typically after it has been derived or
    // initialized during the handshake process. If `None`, the encryption key is unset.
    fn set_k(&mut self, k: Option<[u8; 32]>);

    // Retrieves the current nonce (`n`) used for encryption.
    //
    // The nonce is a counter that is incremented with each encryption/decryption operations to
    // ensure that each encryption operation with the same key produces a unique ciphertext.
    fn get_n(&self) -> u64;

    // Sets the nonce (`n`) to the provided value.
    //
    // Allows the nonce to be explicitly set, typically after it has been initialized, incremented
    // during the encryption process, or reset.
    fn set_n(&mut self, n: u64);

    // Retrieves a mutable reference to the optional cipher instance.
    //
    // Provides access to the underlying AEAD cipher instance used for encryption and decryption
    // operations.
    fn get_cipher(&mut self) -> &mut Option<Cipher_>;

    // Converts the current 64-bit nonce value (`n`) to a 12-byte array.
    //
    // Converts the 64-bit nonce value  to a 12-byte array suitable for use with AEAD ciphers,
    // which typically expect a 96-bit (12-byte) nonce. The result is a correctly formatted nonce
    // for use in encryption and decryption operations.
    fn nonce_to_bytes(&self) -> [u8; 12] {
        let mut res = [0u8; 12];
        let n = self.get_n();
        let bytes = n.to_le_bytes();
        let len = res.len();
        res[4..].copy_from_slice(&bytes[..(len - 4)]);
        res
    }

    // Encrypts the provided `data` in place using the cipher and AAD (`ad`).
    //
    // Performs authenticated encryption on the provided `data` buffer, modifying it in place to
    // contain the ciphertext. The encryption is performed using the current nonce and the AAD.
    // The nonce is incremented after each successful encryption.
    fn encrypt_with_ad<T: Buffer>(&mut self, ad: &[u8], data: &mut T) -> Result<(), Error> {
        // The Noise spec reserves nonce 2^64-1: once the counter reaches it, fail instead of
        // using it or wrapping back to 0.
        if self.get_n() == u64::MAX {
            return Err(Error);
        }
        let n = self.nonce_to_bytes();
        self.set_n(self.get_n() + 1);
        if let Some(c) = self.get_cipher() {
            match c.encrypt(&n, ad, data) {
                Ok(_) => Ok(()),
                Err(e) => {
                    self.set_n(self.get_n() - 1);
                    Err(e)
                }
            }
        } else {
            self.set_n(self.get_n() - 1);
            Ok(())
        }
    }

    // Decrypts the data in place using the cipher and AAD (`ad`).
    //
    // Performs authenticated decryption on the provided `data` buffer, modifying it in place to
    // contain the plaintext. The decryption is performed using the current nonce and the provided
    // AAD. The nonce is incremented after each successful decryption.
    fn decrypt_with_ad<T: Buffer>(&mut self, ad: &[u8], data: &mut T) -> Result<(), Error> {
        if self.get_n() == u64::MAX {
            return Err(Error);
        }
        let n = self.nonce_to_bytes();
        self.set_n(self.get_n() + 1);
        if let Some(c) = self.get_cipher() {
            match c.decrypt(&n, ad, data) {
                Ok(_) => Ok(()),
                Err(e) => {
                    self.set_n(self.get_n() - 1);
                    Err(e)
                }
            }
        } else {
            self.set_n(self.get_n() - 1);
            Ok(())
        }
    }
}

// Represents the state of an AEAD cipher, including the optional 32-byte encryption key (`k`),
// nonce (`n`), and optional cipher instance (`cipher`).
//
// Manages the cryptographic state required to perform AEAD encryption and decryption operations.
// It stores the optional encryption key, the nonce, and the optional cipher instance itself. The
// [`CipherState`] trait is implemented to provide a consistent interface for managing cipher
// state across different AEAD ciphers.
pub struct Cipher<C: AeadCipher> {
    // Optional 32-byte encryption key.
    k: Option<[u8; 32]>,
    // Nonce value.
    n: u64,
    // Optional cipher instance.
    cipher: Option<C>,
}

// Nonce uniqueness rests on exclusive access, not on thread affinity: `Cipher` is both `Send` and
// `Sync`, so sharing one instance across threads is allowed and still requires the caller to
// synchronize. What prevents two encryptions from reusing a nonce is that `encrypt`/`decrypt` take
// `&mut self` and that the type is deliberately not `Clone`, so the key and its nonce counter can
// never be duplicated or advanced from two places at once.
//
// The handshake key `k` is cleared as soon as the handshake no longer needs it (see `erase_k`),
// so it does not outlive its use even though the cipher itself lives for the whole session.
impl<C: AeadCipher> Cipher<C> {
    // Internal use only, we need k for handshake
    pub fn from_key_and_cipher(mut k: [u8; 32], c: C) -> Self {
        let state = Self {
            k: Some(k),
            n: 0,
            cipher: Some(c),
        };
        k.zeroize();
        state
    }

    // Encrypts data in place using an empty additional associated data buffer.
    pub fn encrypt<T: Buffer>(&mut self, msg: &mut T) -> Result<(), Error> {
        self.encrypt_with_ad(&[], msg)
    }

    // Decrypts data in place using an empty additional associated data buffer.
    pub fn decrypt<T: Buffer>(&mut self, msg: &mut T) -> Result<(), Error> {
        self.decrypt_with_ad(&[], msg)
    }

    // Securely erases the stored encryption key.
    pub fn erase_k(&mut self) {
        self.k.zeroize();
    }
}

impl<C: AeadCipher> Drop for Cipher<C> {
    fn drop(&mut self) {
        self.erase_k();
    }
}

impl<C: AeadCipher> CipherState<C> for Cipher<C> {
    fn get_k(&mut self) -> &mut Option<[u8; 32]> {
        &mut self.k
    }
    fn get_n(&self) -> u64 {
        self.n
    }
    fn set_n(&mut self, n: u64) {
        self.n = n;
    }
    fn get_cipher(&mut self) -> &mut Option<C> {
        &mut self.cipher
    }

    fn set_k(&mut self, k: Option<[u8; 32]>) {
        self.k = k;
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::aed_cipher::ChaCha20Poly1305;

    #[test]
    fn exhausted_nonce_fails_instead_of_wrapping() {
        let key = [7u8; 32];
        let cipher = ChaCha20Poly1305::from_key(key);
        let mut cipher = Cipher::from_key_and_cipher(key, cipher);
        cipher.set_n(u64::MAX);

        let mut data = alloc::vec![1u8, 2, 3];
        assert!(cipher.encrypt(&mut data).is_err());
        assert!(cipher.decrypt(&mut data).is_err());
        assert_eq!(cipher.get_n(), u64::MAX);
    }
}
