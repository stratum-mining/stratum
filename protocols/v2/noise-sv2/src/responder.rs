use std::{ptr, time::Duration};

use crate::{
    cipher_state::{Cipher, CipherState, GenericCipher},
    error::Error,
    handshake::HandshakeOp,
    signature_message::SignatureNoiseMessage,
    NoiseCodec,
};
use aes_gcm::KeyInit;
use chacha20poly1305::ChaCha20Poly1305;
use secp256k1::{KeyPair, Secp256k1, SecretKey};

const VERSION: u16 = 0;

pub struct Responder {
    handshake_cipher: Option<ChaCha20Poly1305>,
    k: Option<[u8; 32]>,
    n: u64,
    // Chaining key
    ck: [u8; 32],
    // Handshake hash
    h: [u8; 32],
    // ephemeral keypair
    e: KeyPair,
    // Static pub keypair
    s: KeyPair,
    c1: Option<GenericCipher>,
    c2: Option<GenericCipher>,
    cert_validity: u32,
}

impl std::fmt::Debug for Responder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Responder").finish()
    }
}

// Make sure that Respoder is not sync so we do not need to worry about what other memory accessor see
// after that we zeroize k is send cause if we send it the original thread can not access
// anymore it
//impl<C: AeadCipher> !Sync for Responder<C> {}
//impl<C: AeadCipher> !Copy for Responder<C> {}

impl CipherState<ChaCha20Poly1305> for Responder {
    fn get_k(&mut self) -> &mut Option<[u8; 32]> {
        &mut self.k
    }
    fn get_n(&self) -> u64 {
        self.n
    }
    fn set_n(&mut self, n: u64) {
        self.n = n;
    }

    fn set_k(&mut self, k: Option<[u8; 32]>) {
        self.k = k;
    }
    fn get_cipher(&mut self) -> &mut Option<ChaCha20Poly1305> {
        &mut self.handshake_cipher
    }
}

impl HandshakeOp<ChaCha20Poly1305> for Responder {
    fn name(&self) -> String {
        "Responder".to_string()
    }
    fn get_h(&mut self) -> &mut [u8; 32] {
        &mut self.h
    }

    fn get_ck(&mut self) -> &mut [u8; 32] {
        &mut self.ck
    }

    fn set_h(&mut self, data: [u8; 32]) {
        self.h = data;
    }

    fn set_ck(&mut self, data: [u8; 32]) {
        self.ck = data;
    }

    fn set_handshake_cipher(&mut self, cipher: ChaCha20Poly1305) {
        self.handshake_cipher = Some(cipher);
    }
}

impl Responder {
    pub fn from_authority_kp(
        public: &[u8; 32],
        private: &[u8; 32],
        cert_validity: Duration,
    ) -> Result<Box<Self>, Error> {
        let secp = Secp256k1::new();
        let secret = SecretKey::from_slice(private).map_err(|_| Error::InvalidRawPrivateKey)?;
        let kp = KeyPair::from_secret_key(&secp, &secret);
        let pub_ = kp.x_only_public_key().0.serialize();
        if public == &pub_[..] {
            Ok(Self::new(kp, cert_validity.as_secs() as u32))
        } else {
            Err(Error::InvalidRawPublicKey)
        }
    }

    pub fn new(s: KeyPair, cert_validity: u32) -> Box<Self> {
        let mut self_ = Self {
            handshake_cipher: None,
            k: None,
            n: 0,
            ck: [0; 32],
            h: [0; 32],
            e: Self::generate_key(),
            s,
            c1: None,
            c2: None,
            cert_validity,
        };
        Self::initialize_self(&mut self_);
        Box::new(self_)
    }

    /// #### 4.5.1.2 Responder
    ///
    /// 1. receives ephemeral public key message (32 bytes plaintext public key)
    /// 2. parses received public key as `re.public_key`
    /// 3. calls `MixHash(re.public_key)`
    /// 4. calls `DecryptAndHash()` on remaining bytes (i.e. on empty data with empty _k_, thus effectively only calls `MixHash()` on empty data)
    ///
    /// #### 4.5.2.1 Responder
    ///
    /// 1. initializes empty output buffer
    /// 2. generates ephemeral keypair `e`, appends `e.public_key` to the buffer (32 bytes plaintext public key)
    /// 3. calls `MixHash(e.public_key)`
    /// 4. calls `MixKey(ECDH(e.private_key, re.public_key))`
    /// 5. appends `EncryptAndHash(s.public_key)` (32 bytes encrypted public key, 16 bytes MAC)
    /// 6. calls `MixKey(ECDH(s.private_key, re.public_key))`
    /// 7. appends `EncryptAndHash(SIGNATURE_NOISE_MESSAGE)` to the buffer
    /// 8. submits the buffer for sending to the initiator
    /// 9. return pair of CipherState objects, the first for encrypting transport messages from initiator to responder, and the second for messages in the other direction:
    ///    1. sets `temp_k1, temp_k2 = HKDF(ck, zerolen, 2)`
    ///    2. creates two new CipherState objects `c1` and `c2`
    ///    3. calls `c1.InitializeKey(temp_k1)` and `c2.InitializeKey(temp_k2)`
    ///    4. returns the pair `(c1, c2)`
    ///
    /// ##### Message format of NX-handshake part 2
    ///
    /// | Field name              | Description                                                                                                                                                    |
    /// | ----------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
    /// | PUBKEY                  | Responder's plaintext ephemeral public key                                                                                                                     |
    /// | PUBKEY                  | Responder's encrypted static public key                                                                                                                        |
    /// | MAC                     | Message authentication code for responder's static public key                                                                                                  |
    /// | SIGNATURE_NOISE_MESSAGE | Signed message containing Responder's static key. Signature is issued by authority that is generally known to operate the server acting as the noise responder |
    /// | MAC                     | Message authentication code for SIGNATURE_NOISE_MESSAGE                                                                                                        |
    ///
    /// Message length: 170 bytes
    pub fn step_1(&mut self, re_pub: [u8; 32]) -> Result<([u8; 170], NoiseCodec), aes_gcm::Error> {
        // 4.5.1.2 Responder
        Self::mix_hash(self, &re_pub[..]);
        Self::decrypt_and_hash(self, &mut vec![])?;

        // 4.5.2.1 Responder
        let mut out = [0; 170];
        let serialized = self.e.x_only_public_key().0.serialize();
        out[..32].copy_from_slice(&serialized[..32]);

        // 3. calls `MixHash(e.public_key)`
        Self::mix_hash(self, &serialized);

        // 4. calls `MixKey(ECDH(e.private_key, re.public_key))`
        let e_private_key = self.e.secret_bytes();
        let ecdh = Self::ecdh(&e_private_key[..], &re_pub[..]);
        Self::mix_key(self, &ecdh);

        // 5. appends `EncryptAndHash(s.public_key)` (32 bytes encrypted public key, 16 bytes MAC)
        let mut encrypted_static_pub_k = vec![0; 32];
        let static_pub_k = self.s.x_only_public_key().0.serialize();
        encrypted_static_pub_k[..32].copy_from_slice(&static_pub_k[..32]);
        self.encrypt_and_hash(&mut encrypted_static_pub_k)?;
        out[32..(32 + 16 + 32)].copy_from_slice(&encrypted_static_pub_k[..(32 + 16)]);

        // 6. calls `MixKey(ECDH(s.private_key, re.public_key))`
        let s_private_key = self.s.secret_bytes();
        let ecdh = Self::ecdh(&s_private_key[..], &re_pub[..]);
        Self::mix_key(self, &ecdh[..]);

        // 7. appends `EncryptAndHash(SIGNATURE_NOISE_MESSAGE)` to the buffer
        let valid_from = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let not_valid_after = valid_from as u32 + self.cert_validity;
        let signature_noise_message =
            self.get_signature(VERSION, valid_from as u32, not_valid_after);
        let mut signature_part = Vec::with_capacity(74 + 16);
        signature_part.extend_from_slice(&signature_noise_message[..]);
        Self::encrypt_and_hash(self, &mut signature_part)?;
        for i in (32 + 48)..(32 + 48 + 74 + 16) {
            out[i] = signature_part[i - (32 + 48)];
        }

        // 9. return pair of CipherState objects, the first for encrypting transport messages from initiator to responder, and the second for messages in the other direction:
        let ck = Self::get_ck(self);
        let (temp_k1, temp_k2) = Self::hkdf_2(ck, &[]);
        let c1 = ChaCha20Poly1305::new(&temp_k1.into());
        let c2 = ChaCha20Poly1305::new(&temp_k2.into());
        let c1: Cipher<ChaCha20Poly1305> = Cipher::from_key_and_cipher(temp_k1, c1);
        let c2: Cipher<ChaCha20Poly1305> = Cipher::from_key_and_cipher(temp_k2, c2);
        let to_send = out;
        self.c1 = None;
        self.c2 = None;
        let mut encryptor = GenericCipher::ChaCha20Poly1305(c2);
        let mut decryptor = GenericCipher::ChaCha20Poly1305(c1);
        encryptor.erase_k();
        decryptor.erase_k();
        let codec = crate::NoiseCodec {
            encryptor,
            decryptor,
        };
        Ok((to_send, codec))
    }

    fn get_signature(&self, version: u16, valid_from: u32, not_valid_after: u32) -> [u8; 74] {
        let mut ret = [0; 74];
        let version = version.to_le_bytes();
        let valid_from = valid_from.to_le_bytes();
        let not_valid_after = not_valid_after.to_le_bytes();
        ret[0] = version[0];
        ret[1] = version[1];
        ret[2] = valid_from[0];
        ret[3] = valid_from[1];
        ret[4] = valid_from[2];
        ret[5] = valid_from[3];
        ret[6] = not_valid_after[0];
        ret[7] = not_valid_after[1];
        ret[8] = not_valid_after[2];
        ret[9] = not_valid_after[3];
        SignatureNoiseMessage::sign(&mut ret, &self.s);
        ret
    }

    fn erase(&mut self) {
        if let Some(k) = self.k.as_mut() {
            for b in k {
                unsafe { ptr::write_volatile(b, 0) };
            }
        }
        for mut b in self.ck {
            unsafe { ptr::write_volatile(&mut b, 0) };
        }
        for mut b in self.h {
            unsafe { ptr::write_volatile(&mut b, 0) };
        }
        if let Some(c1) = self.c1.as_mut() {
            c1.erase_k()
        }
        if let Some(c2) = self.c2.as_mut() {
            c2.erase_k()
        }
        self.e.non_secure_erase();
        self.s.non_secure_erase();
    }
}

impl Drop for Responder {
    fn drop(&mut self) {
        self.erase();
    }
}
