use std::ptr;

use crate::aed_cipher::AeadCipher;
use aes_gcm::Aes256Gcm;
use chacha20poly1305::{aead::Buffer, ChaCha20Poly1305};

pub trait CipherState<Cipher_: AeadCipher>
where
    Self: Sized,
{
    fn get_k(&mut self) -> &mut Option<[u8; 32]>;
    fn set_k(&mut self, k: Option<[u8; 32]>);
    fn get_n(&self) -> u64;
    fn set_n(&mut self, n: u64);
    fn get_cipher(&mut self) -> &mut Option<Cipher_>;

    fn nonce_to_bytes(&self) -> [u8; 12] {
        let mut res = [0u8; 12];
        let n = self.get_n();
        let bytes = n.to_le_bytes();
        let len = res.len();
        res[4..].copy_from_slice(&bytes[..(len - 4)]);
        res
    }

    fn into_aesg(mut self) -> Option<Cipher<Aes256Gcm>> {
        #[allow(clippy::clone_on_copy)]
        let k = self.get_k().clone()?;
        let c = Aes256Gcm::from_key(k);
        Some(Cipher::from_cipher(c))
    }

    fn into_chacha(mut self) -> Option<Cipher<ChaCha20Poly1305>> {
        #[allow(clippy::clone_on_copy)]
        let k = self.get_k().clone()?;
        let c = ChaCha20Poly1305::from_key(k);
        Some(Cipher::from_cipher(c))
    }

    fn encrypt_with_ad<T: Buffer>(
        &mut self,
        ad: &[u8],
        data: &mut T,
    ) -> Result<(), aes_gcm::Error> {
        self.set_n(self.get_n() + 1);
        let n = self.nonce_to_bytes();
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

    fn decrypt_with_ad<T: Buffer>(
        &mut self,
        ad: &[u8],
        data: &mut T,
    ) -> Result<(), aes_gcm::Error> {
        self.set_n(self.get_n() + 1);
        let n = self.nonce_to_bytes();
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

#[allow(clippy::large_enum_variant)]
pub enum GenericCipher {
    ChaCha20Poly1305(Cipher<ChaCha20Poly1305>),
    #[allow(dead_code)]
    Aes256Gcm(Cipher<Aes256Gcm>),
}

impl Drop for GenericCipher {
    fn drop(&mut self) {
        self.erase_k();
    }
}

impl GenericCipher {
    pub fn encrypt<T: Buffer>(&mut self, msg: &mut T) -> Result<(), aes_gcm::Error> {
        match self {
            GenericCipher::ChaCha20Poly1305(c) => c.encrypt_with_ad(&[], msg),
            GenericCipher::Aes256Gcm(c) => c.encrypt_with_ad(&[], msg),
        }
    }
    pub fn decrypt<T: Buffer>(&mut self, msg: &mut T) -> Result<(), aes_gcm::Error> {
        match self {
            GenericCipher::ChaCha20Poly1305(c) => c.decrypt_with_ad(&[], msg),
            GenericCipher::Aes256Gcm(c) => c.decrypt_with_ad(&[], msg),
        }
    }
    pub fn erase_k(&mut self) {
        match self {
            GenericCipher::ChaCha20Poly1305(c) => {
                if let Some(k) = c.k.as_mut() {
                    for b in k {
                        unsafe { ptr::write_volatile(b, 0) };
                    }
                    c.k = None;
                }
            }
            GenericCipher::Aes256Gcm(c) => {
                if let Some(k) = c.k.as_mut() {
                    for b in k {
                        unsafe { ptr::write_volatile(b, 0) };
                    }
                    c.k = None;
                }
            }
        }
    }

    #[allow(dead_code)]
    pub fn into_aesg(mut self) -> GenericCipher {
        match &mut self {
            GenericCipher::ChaCha20Poly1305(c) => {
                let c = Cipher::from_cipher(Aes256Gcm::from_key(c.get_k().unwrap()));
                self.erase_k();
                GenericCipher::Aes256Gcm(c)
            }
            GenericCipher::Aes256Gcm(_) => {
                self.erase_k();
                self
            }
        }
    }
}

impl CipherState<Aes256Gcm> for GenericCipher {
    fn get_k(&mut self) -> &mut Option<[u8; 32]> {
        match self {
            GenericCipher::Aes256Gcm(c) => c.get_k(),
            _ => unreachable!(),
        }
    }

    fn set_k(&mut self, k: Option<[u8; 32]>) {
        match self {
            GenericCipher::Aes256Gcm(c) => c.set_k(k),
            _ => unreachable!(),
        }
    }

    fn get_n(&self) -> u64 {
        match self {
            GenericCipher::Aes256Gcm(c) => c.get_n(),
            _ => unreachable!(),
        }
    }

    fn set_n(&mut self, n: u64) {
        match self {
            GenericCipher::Aes256Gcm(c) => c.set_n(n),
            _ => unreachable!(),
        }
    }

    fn get_cipher(&mut self) -> &mut Option<Aes256Gcm> {
        match self {
            GenericCipher::Aes256Gcm(c) => c.get_cipher(),
            _ => unreachable!(),
        }
    }
}

pub struct Cipher<C: AeadCipher> {
    k: Option<[u8; 32]>,
    n: u64,
    cipher: Option<C>,
}

// Make sure that Cipher is not sync so we do not need to worry about what other memory accessor see
// after that we zeroize k is send cause if we send it the original thread can not access
// anymore it
//impl<C: AeadCipher> !Sync for Cipher<C> {}
//impl<C: AeadCipher> !Copy for Cipher<C> {}

impl<C: AeadCipher> Cipher<C> {
    /// Internal use only, we need k for handshake
    pub fn from_key_and_cipher(k: [u8; 32], c: C) -> Self {
        Self {
            k: Some(k),
            n: 0,
            cipher: Some(c),
        }
    }

    /// At the end of the handshake we return a cipher with hidden key
    #[allow(dead_code)]
    pub fn from_cipher(c: C) -> Self {
        Self {
            k: None,
            n: 0,
            cipher: Some(c),
        }
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
