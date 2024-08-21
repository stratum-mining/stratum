use aes_gcm::Aes256Gcm;
use chacha20poly1305::{aead::Buffer, AeadInPlace, ChaCha20Poly1305, ChaChaPoly1305, KeyInit};

pub trait AeadCipher {
    fn from_key(k: [u8; 32]) -> Self;

    fn encrypt<T: Buffer>(
        &mut self,
        nonce: &[u8; 12],
        ad: &[u8],
        data: &mut T,
    ) -> Result<(), aes_gcm::Error>;

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
