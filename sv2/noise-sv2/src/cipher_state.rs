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

use core::ptr;

use crate::aed_cipher::AeadCipher;
use chacha20poly1305::aead::{Buffer, Error};

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
#[derive(Clone)]
pub struct Cipher<C: AeadCipher> {
    // Optional 32-byte encryption key.
    k: Option<[u8; 32]>,
    // Nonce value.
    n: u64,
    // Optional cipher instance.
    cipher: Option<C>,
}

// Ensures that the `Cipher` type is not `Sync`, which prevents multiple threads from
// simultaneously accessing the same instance of `Cipher`. This eliminates the need to handle
// potential issues related to visibility of changes across threads.
//
// After sending the `k` value, we immediately clear it to prevent the original thread from
// accessing the value again, thereby enhancing security by ensuring the sensitive data is no
// longer available in memory.
//
// The `Cipher` struct is neither `Sync` nor `Copy` due to its `cipher` field, which implements
// the `AeadCipher` trait. This trait requires mutable access, making the entire struct non-`Sync`
// and non-`Copy`, even though the key and nonce are simple types.
impl<C: AeadCipher> Cipher<C> {
    // Internal use only, we need k for handshake
    pub fn from_key_and_cipher(k: [u8; 32], c: C) -> Self {
        Self {
            k: Some(k),
            n: 0,
            cipher: Some(c),
        }
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
        if let Some(k) = self.k.as_mut() {
            for b in k {
                unsafe { ptr::write_volatile(b, 0) };
            }
            self.k = None;
        }
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
