//! Implement Sv2 noise https://github.com/stratum-mining/sv2-spec/blob/main/04-Protocol-Security.md#4-protocol-security

// #![feature(negative_impls)]

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

const PARITY: secp256k1::Parity = secp256k1::Parity::Even;

pub struct NoiseCodec {
    encryptor: GenericCipher,
    decryptor: GenericCipher,
}

impl std::fmt::Debug for NoiseCodec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NoiseCodec").finish()
    }
}

impl NoiseCodec {
    pub fn encrypt<T: Buffer>(&mut self, msg: &mut T) -> Result<(), aes_gcm::Error> {
        self.encryptor.encrypt(msg)
    }
    pub fn decrypt<T: Buffer>(&mut self, msg: &mut T) -> Result<(), aes_gcm::Error> {
        self.decryptor.decrypt(msg)
    }
}

pub use error::Error;
pub use initiator::Initiator;
pub use responder::Responder;
